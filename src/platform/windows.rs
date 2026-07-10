use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::sync::{Mutex, OnceLock};
use tokio::sync::mpsc;
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, InvalidateRect,
    SetBkMode, SetTextColor, DT_CENTER, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, PAINTSTRUCT,
    TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, SendInput, UnregisterHotKey, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYEVENTF_KEYUP, MOD_CONTROL, VK_CONTROL, VK_SPACE, VK_V,
};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconFromResourceEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
    DestroyMenu, DispatchMessageW, GetCursorPos, GetMessageW, GetSystemMetrics, KillTimer,
    LoadIconW, MessageBoxW, PostMessageW, PostQuitMessage, RegisterClassW, SetForegroundWindow,
    SetTimer, SetWindowPos, ShowWindow, TrackPopupMenu, TranslateMessage, CS_HREDRAW, CS_VREDRAW,
    CW_USEDEFAULT, HMENU, HWND_TOPMOST, IDI_APPLICATION, MB_ICONERROR, MB_OK, MSG, SM_CXSCREEN,
    SM_CYSCREEN, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE, TPM_BOTTOMALIGN,
    TPM_LEFTALIGN, WM_APP, WM_COMMAND, WM_DESTROY, WM_HOTKEY, WM_PAINT, WM_RBUTTONUP, WM_TIMER,
    WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW, WS_POPUP,
};

use crate::core::platform::PlatformAdapter;

const LOGO_ICO: &[u8] = include_bytes!("../../assets/logo.ico");
const HOTKEY_ID: i32 = 1;
const TRAY_ID: u32 = 1;
const WM_TRAYICON: u32 = WM_APP + 1;
const WM_STATUS_POPUP: u32 = WM_APP + 2;
const STATUS_POPUP_TIMER_ID: usize = 1;
const MENU_OPEN_CONFIG: usize = 1002;
const MENU_QUIT: usize = 1003;

static COMMAND_TX: OnceLock<mpsc::UnboundedSender<TrayCommand>> = OnceLock::new();
static TRAY_HWND: OnceLock<HWND> = OnceLock::new();
static STATUS_POPUP_HWND: OnceLock<HWND> = OnceLock::new();
static STATUS_POPUP_TEXT: OnceLock<Mutex<String>> = OnceLock::new();
static STATUS_POPUP_TIMEOUT_MS: OnceLock<Mutex<Option<u32>>> = OnceLock::new();

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
            clipboard
                .set_text(text)
                .context("setting Windows clipboard text")?;
            Ok(())
        })
        .await??;
        Ok(())
    }

    async fn trigger_paste(&self) -> Result<()> {
        send_ctrl_v()
    }

    async fn notify(&self, title: &str, body: &str) -> Result<()> {
        show_status_popup(&format!("{title}: {body}"), Some(4_000))
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
        register_status_popup_class(hinstance)?;

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
        TRAY_HWND
            .set(hwnd)
            .map_err(|_| anyhow!("Windows tray window was already initialized"))?;
        let popup_hwnd = create_status_popup_window(hinstance)?;
        STATUS_POPUP_HWND
            .set(popup_hwnd)
            .map_err(|_| anyhow!("Windows status popup was already initialized"))?;

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

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_HOTKEY => {
            send_command(TrayCommand::Toggle);
            0
        }
        WM_COMMAND => {
            match loword(wparam as usize) {
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
    data.hIcon = load_logo_icon().unwrap_or_else(|| LoadIconW(0, IDI_APPLICATION));
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

pub fn show_status_popup(text: &str, timeout_ms: Option<u32>) -> Result<()> {
    let hwnd = *STATUS_POPUP_HWND
        .get()
        .ok_or_else(|| anyhow!("Windows status popup is not initialized"))?;
    let text_lock = STATUS_POPUP_TEXT.get_or_init(|| Mutex::new(String::new()));
    *text_lock.lock().unwrap() = text.to_string();
    let timeout_lock = STATUS_POPUP_TIMEOUT_MS.get_or_init(|| Mutex::new(None));
    *timeout_lock.lock().unwrap() = timeout_ms;
    unsafe {
        if PostMessageW(hwnd, WM_STATUS_POPUP, 0, 0) == 0 {
            return Err(anyhow!("posting Windows status popup message failed"));
        }
    }
    Ok(())
}

unsafe fn register_status_popup_class(hinstance: isize) -> Result<()> {
    let class_name = wide("OpenVoiceDesktopStatusPopup");
    let wnd = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(status_popup_wnd_proc),
        hInstance: hinstance,
        lpszClassName: class_name.as_ptr(),
        hCursor: 0,
        ..std::mem::zeroed()
    };
    if RegisterClassW(&wnd) == 0 {
        return Err(anyhow!("registering Windows status popup class failed"));
    }
    Ok(())
}

unsafe fn create_status_popup_window(hinstance: isize) -> Result<HWND> {
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        wide("OpenVoiceDesktopStatusPopup").as_ptr(),
        wide("OpenVoiceDesktop status").as_ptr(),
        WS_POPUP,
        0,
        0,
        0,
        0,
        0,
        0,
        hinstance,
        std::ptr::null(),
    );
    if hwnd == 0 {
        return Err(anyhow!("creating Windows status popup failed"));
    }
    Ok(hwnd)
}

unsafe extern "system" fn status_popup_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_STATUS_POPUP => {
            position_status_popup(hwnd);
            InvalidateRect(hwnd, std::ptr::null(), 1);
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            KillTimer(hwnd, STATUS_POPUP_TIMER_ID);
            let timeout_ms = STATUS_POPUP_TIMEOUT_MS
                .get_or_init(|| Mutex::new(None))
                .lock()
                .unwrap()
                .unwrap_or(0);
            if timeout_ms > 0 {
                SetTimer(hwnd, STATUS_POPUP_TIMER_ID, timeout_ms, None);
            }
            0
        }
        WM_TIMER if wparam == STATUS_POPUP_TIMER_ID => {
            ShowWindow(hwnd, SW_HIDE);
            0
        }
        WM_PAINT => {
            paint_status_popup(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn position_status_popup(hwnd: HWND) {
    let width = 360;
    let height = 54;
    let x = (GetSystemMetrics(SM_CXSCREEN) - width) / 2;
    let y = GetSystemMetrics(SM_CYSCREEN) - height - 72;
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        x,
        y,
        width,
        height,
        SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );
}

unsafe fn paint_status_popup(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let rect = RECT {
        left: 0,
        top: 0,
        right: 360,
        bottom: 54,
    };
    let brush = CreateSolidBrush(rgb(17, 20, 24));
    FillRect(hdc, &rect, brush);
    DeleteObject(brush);
    SetBkMode(hdc, TRANSPARENT as i32);
    SetTextColor(hdc, rgb(244, 247, 250));
    let text = STATUS_POPUP_TEXT
        .get_or_init(|| Mutex::new(String::new()))
        .lock()
        .unwrap()
        .clone();
    let mut text_rect = RECT {
        left: 16,
        top: 0,
        right: 344,
        bottom: 54,
    };
    DrawTextW(
        hdc,
        wide(&text).as_ptr(),
        -1,
        &mut text_rect,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    EndPaint(hwnd, &ps);
}

fn load_logo_icon() -> Option<isize> {
    let image = select_ico_image(LOGO_ICO, 32)?;
    let icon = unsafe {
        CreateIconFromResourceEx(image.as_ptr(), image.len() as u32, 1, 0x0003_0000, 0, 0, 0)
    };
    if icon == 0 {
        None
    } else {
        Some(icon)
    }
}

fn select_ico_image(data: &[u8], preferred_size: u8) -> Option<&[u8]> {
    if data.len() < 6 || data[0..4] != [0, 0, 1, 0] {
        return None;
    }
    let count = u16::from_le_bytes([data[4], data[5]]) as usize;
    let mut selected = None;
    for index in 0..count {
        let offset = 6 + index * 16;
        let entry = data.get(offset..offset + 16)?;
        let size = if entry[0] == 0 {
            256
        } else {
            entry[0] as usize
        };
        let bytes = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
        let image_offset =
            u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;
        let image = data.get(image_offset..image_offset + bytes)?;
        if entry[0] == preferred_size {
            return Some(image);
        }
        if selected.is_none() || size == 32 {
            selected = Some(image);
        }
    }
    selected
}

fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
    red as u32 | ((green as u32) << 8) | ((blue as u32) << 16)
}

unsafe fn show_tray_menu(hwnd: HWND) {
    let menu: HMENU = CreatePopupMenu();
    if menu == 0 {
        return;
    }
    AppendMenuW(
        menu,
        0,
        MENU_OPEN_CONFIG,
        wide("Open Config Folder").as_ptr(),
    );
    AppendMenuW(menu, 0, MENU_QUIT, wide("Quit").as_ptr());

    let mut point = std::mem::zeroed();
    GetCursorPos(&mut point);
    SetForegroundWindow(hwnd);
    TrackPopupMenu(
        menu,
        TPM_LEFTALIGN | TPM_BOTTOMALIGN,
        point.x,
        point.y,
        0,
        hwnd,
        std::ptr::null(),
    );
    DestroyMenu(menu);
}

fn open_config_folder() -> Result<()> {
    let dir = dirs::config_dir()
        .ok_or_else(|| anyhow!("could not locate config directory"))?
        .join("openvoice-desktop");
    std::fs::create_dir_all(&dir)?;
    std::process::Command::new("explorer.exe")
        .arg(dir)
        .spawn()?;
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
