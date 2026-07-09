use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::sync::OnceLock;
use tokio::sync::mpsc;
use windows_sys::Win32::Foundation::{HMENU, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL, VK_SPACE,
    VK_V,
};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
    GetCursorPos, GetMessageW, LoadIconW, MessageBoxW, PostQuitMessage, RegisterClassW,
    RegisterHotKey, SetForegroundWindow, TrackPopupMenu, TranslateMessage, UnregisterHotKey,
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, IDI_APPLICATION, MB_ICONERROR,
    MB_OK, MOD_CONTROL, MSG, TPM_BOTTOMALIGN, TPM_LEFTALIGN, WM_APP, WM_COMMAND, WM_DESTROY,
    WM_HOTKEY, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};

use crate::core::platform::PlatformAdapter;

const HOTKEY_ID: i32 = 1;
const TRAY_ID: u32 = 1;
const WM_TRAYICON: u32 = WM_APP + 1;
const MENU_TOGGLE: usize = 1001;
const MENU_OPEN_CONFIG: usize = 1002;
const MENU_QUIT: usize = 1003;

static COMMAND_TX: OnceLock<mpsc::UnboundedSender<TrayCommand>> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
pub enum TrayCommand {
    Toggle,
    OpenConfigFolder,
    Quit,
}

#[derive(Debug, Clone, Default)]
pub struct WindowsPlatformAdapter;

#[async_trait]
impl PlatformAdapter for WindowsPlatformAdapter {
    async fn clipboard_text(&self) -> Result<Option<String>> {
        let text = tokio::task::spawn_blocking(|| -> Result<Option<String>> {
            let mut clipboard = arboard::Clipboard::new().context("opening Windows clipboard")?;
            match clipboard.get_text() {
                Ok(text) => Ok(Some(text)),
                Err(_) => Ok(None),
            }
        })
        .await??;
        Ok(text)
    }

    async fn set_clipboard_text(&self, text: &str) -> Result<()> {
        let text = text.to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut clipboard = arboard::Clipboard::new().context("opening Windows clipboard")?;
            clipboard.set_text(text).context("setting Windows clipboard text")?;
            Ok(())
        })
        .await??;
        Ok(())
    }

    async fn trigger_paste(&self) -> Result<()> {
        send_ctrl_v()
    }

    async fn notify(&self, title: &str, body: &str) -> Result<()> {
        let title = title.to_string();
        let body = body.to_string();
        tokio::task::spawn_blocking(move || message_box(&title, &body)).await??;
        Ok(())
    }
}

pub fn run_tray_loop(tx: mpsc::UnboundedSender<TrayCommand>) -> Result<()> {
    COMMAND_TX
        .set(tx)
        .map_err(|_| anyhow!("Windows tray command channel was already initialized"))?;

    unsafe {
        let class_name = wide("OpenVoiceDesktopWindow");
        let hinstance = GetModuleHandleW(std::ptr::null());
        let wnd = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance,
            lpszClassName: class_name.as_ptr(),
            hCursor: 0,
            ..std::mem::zeroed()
        };
        if RegisterClassW(&wnd) == 0 {
            return Err(anyhow!("registering Windows tray window class failed"));
        }

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            wide("OpenVoiceDesktop").as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            0,
            0,
            hinstance,
            std::ptr::null(),
        );
        if hwnd == 0 {
            return Err(anyhow!("creating Windows tray window failed"));
        }

        if RegisterHotKey(hwnd, HOTKEY_ID, MOD_CONTROL as u32, VK_SPACE as u32) == 0 {
            message_box(
                "OpenVoiceDesktop hotkey unavailable",
                "Ctrl+Space is already registered by another application.",
            )?;
        }

        add_tray_icon(hwnd)?;

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        remove_tray_icon(hwnd);
        UnregisterHotKey(hwnd, HOTKEY_ID);
        Ok(())
    }
}

fn send_ctrl_v() -> Result<()> {
    let inputs = [
        keyboard_input(VK_CONTROL as u16, 0),
        keyboard_input(VK_V as u16, 0),
        keyboard_input(VK_V as u16, KEYEVENTF_KEYUP),
        keyboard_input(VK_CONTROL as u16, KEYEVENTF_KEYUP),
    ];
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent != inputs.len() as u32 {
        return Err(anyhow!("SendInput failed while sending Ctrl+V"));
    }
    Ok(())
}

fn keyboard_input(vk: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_HOTKEY => {
            send_command(TrayCommand::Toggle);
            0
        }
        WM_COMMAND => {
            match loword(wparam as usize) {
                MENU_TOGGLE => send_command(TrayCommand::Toggle),
                MENU_OPEN_CONFIG => send_command(TrayCommand::OpenConfigFolder),
                MENU_QUIT => {
                    send_command(TrayCommand::Quit);
                    PostQuitMessage(0);
                }
                _ => {}
            }
            0
        }
        WM_TRAYICON if lparam as u32 == WM_RBUTTONUP => {
            show_tray_menu(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn send_command(command: TrayCommand) {
    if let Some(tx) = COMMAND_TX.get() {
        let _ = tx.send(command);
    }
}

unsafe fn add_tray_icon(hwnd: HWND) -> Result<()> {
    let mut data: NOTIFYICONDATAW = std::mem::zeroed();
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = TRAY_ID;
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = WM_TRAYICON;
    data.hIcon = LoadIconW(0, IDI_APPLICATION);
    copy_tip(&mut data.szTip, "OpenVoiceDesktop");
    if Shell_NotifyIconW(NIM_ADD, &mut data) == 0 {
        return Err(anyhow!("adding Windows tray icon failed"));
    }
    Ok(())
}

unsafe fn remove_tray_icon(hwnd: HWND) {
    let mut data: NOTIFYICONDATAW = std::mem::zeroed();
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = TRAY_ID;
    let _ = Shell_NotifyIconW(NIM_DELETE, &mut data);
}

unsafe fn show_tray_menu(hwnd: HWND) {
    let menu: HMENU = CreatePopupMenu();
    if menu == 0 {
        return;
    }
    AppendMenuW(menu, 0, MENU_TOGGLE, wide("Toggle Recording").as_ptr());
    AppendMenuW(menu, 0, MENU_OPEN_CONFIG, wide("Open Config Folder").as_ptr());
    AppendMenuW(menu, 0, MENU_QUIT, wide("Quit").as_ptr());

    let mut point = std::mem::zeroed();
    GetCursorPos(&mut point);
    SetForegroundWindow(hwnd);
    TrackPopupMenu(menu, TPM_LEFTALIGN | TPM_BOTTOMALIGN, point.x, point.y, 0, hwnd, std::ptr::null());
    DestroyMenu(menu);
}

fn open_config_folder() -> Result<()> {
    let dir = dirs::config_dir()
        .ok_or_else(|| anyhow!("could not locate config directory"))?
        .join("openvoice-desktop");
    std::fs::create_dir_all(&dir)?;
    std::process::Command::new("explorer.exe").arg(dir).spawn()?;
    Ok(())
}

pub async fn handle_tray_command(command: TrayCommand) -> Result<bool> {
    match command {
        TrayCommand::Toggle => Ok(true),
        TrayCommand::OpenConfigFolder => {
            tokio::task::spawn_blocking(open_config_folder).await??;
            Ok(false)
        }
        TrayCommand::Quit => Ok(false),
    }
}

fn message_box(title: &str, body: &str) -> Result<()> {
    unsafe {
        MessageBoxW(
            0,
            wide(body).as_ptr(),
            wide(title).as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
    Ok(())
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(once(0)).collect()
}

fn copy_tip(dest: &mut [u16], value: &str) {
    let encoded = wide(value);
    for (slot, code_unit) in dest.iter_mut().zip(encoded.into_iter()) {
        *slot = code_unit;
    }
}

fn loword(value: usize) -> usize {
    value & 0xffff
}
