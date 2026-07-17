use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::ffi::c_void;
use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::sync::{
    atomic::{AtomicBool, AtomicU8, Ordering},
    Mutex, OnceLock,
};
use tokio::sync::mpsc;
use unicode_segmentation::UnicodeSegmentation;
use windows_sys::core::{GUID, HRESULT};
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreatePen, CreateRoundRectRgn, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint,
    FillRect, InvalidateRect, RoundRect, SelectObject, SetBkMode, SetTextColor, SetWindowRgn,
    DT_CENTER, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows_sys::Win32::Media::Audio::{eMultimedia, eRender};
use windows_sys::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, SendInput, UnregisterHotKey, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYEVENTF_KEYUP, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, VK_CONTROL, VK_ESCAPE,
    VK_LEFT, VK_RETURN, VK_SHIFT, VK_SPACE, VK_V,
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
    TPM_LEFTALIGN, WM_APP, WM_COMMAND, WM_DESTROY, WM_HOTKEY, WM_LBUTTONUP, WM_PAINT, WM_RBUTTONUP,
    WM_TIMER, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW,
    WS_POPUP,
};

use crate::core::audio::{play_audio_feedback, AudioFeedbackPlan};
use crate::core::config::{RecordingOutputMode, Theme, TypingTool};
use crate::core::platform::{reduced_output_volume, AudioOutputState, PlatformAdapter};

const LOGO_ICO: &[u8] = include_bytes!("../../assets/logo.ico");
const HOTKEY_ID: i32 = 1;
const CANCEL_HOTKEY_ID: i32 = 2;
const LANGUAGE_HOTKEY_ID: i32 = 3;
const TRAY_ID: u32 = 1;
const WM_TRAYICON: u32 = WM_APP + 1;
const WM_STATUS_POPUP: u32 = WM_APP + 2;
const WM_STATUS_HIDE: u32 = WM_APP + 3;
const STATUS_POPUP_HIDE_TIMER_ID: usize = 1;
const STATUS_POPUP_ANIMATION_TIMER_ID: usize = 2;
const STATUS_POPUP_WIDTH: i32 = 220;
const STATUS_POPUP_HEIGHT: i32 = 40;
const MENU_SETTINGS: usize = 1001;
const MENU_COPY_LAST_TRANSCRIPT: usize = 1002;
const MENU_CANCEL: usize = 1003;
const MENU_TEST_ENDPOINT: usize = 1004;
const MENU_OPEN_LOGS: usize = 1005;
const MENU_OPEN_CONFIG: usize = 1006;
const MENU_QUIT: usize = 1007;

static COMMAND_TX: OnceLock<mpsc::UnboundedSender<TrayCommand>> = OnceLock::new();
static TRAY_HWND: OnceLock<HWND> = OnceLock::new();
static TRAY_ICON_VISIBLE: AtomicBool = AtomicBool::new(false);
static TRAY_STATE: AtomicU8 = AtomicU8::new(0);
static STATUS_POPUP_HWND: OnceLock<HWND> = OnceLock::new();
static STATUS_POPUP_TEXT: OnceLock<Mutex<String>> = OnceLock::new();
static STATUS_POPUP_TIMEOUT_MS: OnceLock<Mutex<Option<u32>>> = OnceLock::new();
static STATUS_POPUP_MODE: OnceLock<Mutex<StatusPopupMode>> = OnceLock::new();
static STATUS_POPUP_FRAME: OnceLock<Mutex<u32>> = OnceLock::new();
static RECORDING_LANGUAGE: OnceLock<Mutex<String>> = OnceLock::new();
static TRAY_THEME: OnceLock<Mutex<Theme>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusPopupMode {
    Text,
    RecordingBars,
}

#[derive(Debug, Clone, Copy)]
pub enum TrayCommand {
    Toggle,
    CycleLanguage,
    Settings,
    CopyLastTranscript,
    Cancel,
    TestEndpoint,
    OpenLogs,
    OpenConfigFolder,
    ReloadLifecycle { show_tray_icon: bool },
    Quit,
}

pub fn set_tray_state(state: crate::core::settings_ui::TrayIconState) {
    let code = match state {
        crate::core::settings_ui::TrayIconState::Idle => 0,
        crate::core::settings_ui::TrayIconState::Recording => 1,
        crate::core::settings_ui::TrayIconState::Processing => 2,
        crate::core::settings_ui::TrayIconState::Error => 3,
    };
    TRAY_STATE.store(code, Ordering::Relaxed);
}

fn current_tray_state() -> crate::core::settings_ui::TrayIconState {
    match TRAY_STATE.load(Ordering::Relaxed) {
        1 => crate::core::settings_ui::TrayIconState::Recording,
        2 => crate::core::settings_ui::TrayIconState::Processing,
        3 => crate::core::settings_ui::TrayIconState::Error,
        _ => crate::core::settings_ui::TrayIconState::Idle,
    }
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

    async fn type_text_with_tool(&self, text: &str, tool: TypingTool) -> Result<()> {
        match tool {
            TypingTool::Auto | TypingTool::SendInput => self.type_text(text).await,
            _ => Err(anyhow!("typing tool {tool:?} is unavailable on Windows")),
        }
    }

    async fn trigger_submit_with_tool(&self, tool: TypingTool) -> Result<()> {
        match tool {
            TypingTool::Auto | TypingTool::SendInput => send_enter(),
            _ => Err(anyhow!("typing tool {tool:?} is unavailable on Windows")),
        }
    }

    async fn select_inserted_text(&self, text: &str, tool: TypingTool) -> Result<()> {
        match tool {
            TypingTool::Auto | TypingTool::SendInput => {
                send_shift_left(text.graphemes(true).count())
            }
            _ => Err(anyhow!(
                "typing tool {tool:?} cannot select text on Windows"
            )),
        }
    }

    async fn notify(&self, title: &str, body: &str) -> Result<()> {
        show_status_popup(&format!("{title}: {body}"), Some(4_000))
    }

    async fn adjust_output_for_recording(
        &self,
        mode: RecordingOutputMode,
        volume: f32,
    ) -> Result<Option<AudioOutputState>> {
        if mode == RecordingOutputMode::Unchanged {
            return Ok(None);
        }
        let state = tokio::task::spawn_blocking(|| windows_output_state(None, None)).await??;
        let adjusted = tokio::task::spawn_blocking(move || match mode {
            RecordingOutputMode::Unchanged => Ok(state),
            RecordingOutputMode::Reduced => {
                windows_output_state(None, Some(reduced_output_volume(state.volume, volume)))
            }
            RecordingOutputMode::Muted => windows_output_state(Some(true), None),
        })
        .await?;
        if let Err(error) = adjusted {
            let _ = tokio::task::spawn_blocking(move || {
                windows_output_state(Some(state.muted), Some(state.volume))
            })
            .await;
            return Err(error);
        }
        Ok(Some(state))
    }

    async fn restore_output_after_recording(&self, state: AudioOutputState) -> Result<()> {
        tokio::task::spawn_blocking(move || {
            windows_output_state(Some(state.muted), Some(state.volume))
        })
        .await??;
        Ok(())
    }

    async fn play_audio_feedback(&self, plan: &AudioFeedbackPlan) -> Result<()> {
        tokio::task::spawn_blocking({
            let plan = plan.clone();
            move || play_audio_feedback(&plan)
        })
        .await??;
        Ok(())
    }
}

#[repr(C)]
struct DeviceEnumeratorVtable {
    query_interface: usize,
    add_ref: usize,
    release: usize,
    enum_audio_endpoints: usize,
    get_default_audio_endpoint:
        unsafe extern "system" fn(*mut c_void, i32, i32, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
struct DeviceVtable {
    query_interface: usize,
    add_ref: usize,
    release: usize,
    activate: unsafe extern "system" fn(
        *mut c_void,
        *const GUID,
        u32,
        *const c_void,
        *mut *mut c_void,
    ) -> HRESULT,
}

#[repr(C)]
struct EndpointVolumeVtable {
    query_interface: usize,
    add_ref: usize,
    release: usize,
    register_control_change_notify: usize,
    unregister_control_change_notify: usize,
    get_channel_count: usize,
    set_master_volume_level: usize,
    set_master_volume_level_scalar:
        unsafe extern "system" fn(*mut c_void, f32, *const GUID) -> HRESULT,
    get_master_volume_level: usize,
    get_master_volume_level_scalar: unsafe extern "system" fn(*mut c_void, *mut f32) -> HRESULT,
    channel_volume_methods: [usize; 4],
    set_mute: unsafe extern "system" fn(*mut c_void, i32, *const GUID) -> HRESULT,
    get_mute: unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT,
}

const CLSID_MM_DEVICE_ENUMERATOR: GUID = GUID {
    data1: 0xbcde0395,
    data2: 0xe52f,
    data3: 0x467c,
    data4: [0x8e, 0x3d, 0xc4, 0x57, 0x92, 0x91, 0x69, 0x2e],
};
const IID_MM_DEVICE_ENUMERATOR: GUID = GUID {
    data1: 0xa95664d2,
    data2: 0x9614,
    data3: 0x4f35,
    data4: [0xa7, 0x46, 0xde, 0x8d, 0xb6, 0x36, 0x17, 0xe6],
};
const IID_AUDIO_ENDPOINT_VOLUME: GUID = GUID {
    data1: 0x5cdf2c82,
    data2: 0x841e,
    data3: 0x4546,
    data4: [0x97, 0x22, 0x0c, 0xf7, 0x40, 0x78, 0x22, 0x9a],
};

fn windows_output_state(
    set_muted: Option<bool>,
    set_volume: Option<f32>,
) -> Result<AudioOutputState> {
    let init = unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) };
    if init < 0 {
        return Err(anyhow!(
            "initializing Windows audio COM session failed: HRESULT {init:#x}"
        ));
    }

    let result = (|| unsafe {
        let mut enumerator = std::ptr::null_mut();
        check_hresult(
            CoCreateInstance(
                &CLSID_MM_DEVICE_ENUMERATOR,
                std::ptr::null_mut(),
                CLSCTX_ALL,
                &IID_MM_DEVICE_ENUMERATOR,
                &mut enumerator,
            ),
            "creating Windows audio device enumerator",
        )?;
        let enumerator_vtable = *(enumerator as *mut *mut DeviceEnumeratorVtable);
        let mut device = std::ptr::null_mut();
        check_hresult(
            ((*enumerator_vtable).get_default_audio_endpoint)(
                enumerator,
                eRender,
                eMultimedia,
                &mut device,
            ),
            "getting Windows default output device",
        )?;
        let device_vtable = *(device as *mut *mut DeviceVtable);
        let mut endpoint = std::ptr::null_mut();
        check_hresult(
            ((*device_vtable).activate)(
                device,
                &IID_AUDIO_ENDPOINT_VOLUME,
                CLSCTX_ALL,
                std::ptr::null(),
                &mut endpoint,
            ),
            "activating Windows endpoint volume",
        )?;
        let endpoint_vtable = *(endpoint as *mut *mut EndpointVolumeVtable);
        let mut muted = 0_i32;
        let mut volume = 0.0_f32;
        check_hresult(
            ((*endpoint_vtable).get_mute)(endpoint, &mut muted),
            "reading Windows output mute state",
        )?;
        check_hresult(
            ((*endpoint_vtable).get_master_volume_level_scalar)(endpoint, &mut volume),
            "reading Windows output volume",
        )?;
        if let Some(set_volume) = set_volume {
            check_hresult(
                ((*endpoint_vtable).set_master_volume_level_scalar)(
                    endpoint,
                    set_volume.clamp(0.0, 1.0),
                    std::ptr::null(),
                ),
                "setting Windows output volume",
            )?;
        }
        if let Some(set_muted) = set_muted {
            check_hresult(
                ((*endpoint_vtable).set_mute)(endpoint, i32::from(set_muted), std::ptr::null()),
                "setting Windows output mute state",
            )?;
        }
        release_com(endpoint);
        release_com(device);
        release_com(enumerator);
        Ok(AudioOutputState {
            muted: muted != 0,
            volume: volume.clamp(0.0, 1.0),
        })
    })();
    unsafe { CoUninitialize() };
    result
}

unsafe fn release_com(value: *mut c_void) {
    if value.is_null() {
        return;
    }
    let vtable = *(value as *mut *mut usize);
    let release: unsafe extern "system" fn(*mut c_void) -> u32 =
        std::mem::transmute(vtable.add(2).read());
    release(value);
}

fn check_hresult(value: HRESULT, operation: &str) -> Result<()> {
    if value >= 0 {
        Ok(())
    } else {
        Err(anyhow!("{operation} failed: HRESULT {value:#x}"))
    }
}

pub fn run_tray_loop(tx: mpsc::UnboundedSender<TrayCommand>) -> Result<()> {
    run_tray_loop_with_config(tx, true, false, Theme::System, "Ctrl+Alt+L")
}

pub fn run_tray_loop_with_config(
    tx: mpsc::UnboundedSender<TrayCommand>,
    show_tray_icon: bool,
    start_hidden: bool,
    theme: Theme,
    language_shortcut: &str,
) -> Result<()> {
    *TRAY_THEME
        .get_or_init(|| Mutex::new(Theme::System))
        .lock()
        .unwrap() = theme;
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
        if RegisterHotKey(
            hwnd,
            CANCEL_HOTKEY_ID,
            MOD_NOREPEAT as u32,
            VK_ESCAPE as u32,
        ) == 0
        {
            message_box(
                "OpenVoiceDesktop cancel shortcut unavailable",
                "Escape is already registered by another application.",
            )?;
        }
        let (language_modifiers, language_key) = windows_hotkey(language_shortcut)?;
        if RegisterHotKey(hwnd, LANGUAGE_HOTKEY_ID, language_modifiers, language_key) == 0 {
            message_box(
                "OpenVoiceDesktop language shortcut unavailable",
                &format!("{language_shortcut} is already registered by another application."),
            )?;
        }

        if show_tray_icon {
            add_tray_icon(hwnd)?;
            TRAY_ICON_VISIBLE.store(true, Ordering::Relaxed);
        }
        if start_hidden {
            ShowWindow(hwnd, SW_HIDE);
        }

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        if TRAY_ICON_VISIBLE.swap(false, Ordering::Relaxed) {
            remove_tray_icon(hwnd);
        }
        UnregisterHotKey(hwnd, HOTKEY_ID);
        UnregisterHotKey(hwnd, CANCEL_HOTKEY_ID);
        UnregisterHotKey(hwnd, LANGUAGE_HOTKEY_ID);
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

fn windows_hotkey(shortcut: &str) -> Result<(u32, u32)> {
    let parsed = crate::core::shortcut::ParsedShortcut::parse(shortcut)?;
    let mut parts = parsed.as_str().split('+').collect::<Vec<_>>();
    let key = parts.pop().ok_or_else(|| anyhow!("shortcut has no key"))?;
    let mut modifiers = MOD_NOREPEAT as u32;
    for modifier in parts {
        modifiers |= match modifier {
            "Ctrl" => MOD_CONTROL as u32,
            "Alt" => MOD_ALT as u32,
            "Shift" => MOD_SHIFT as u32,
            "Win" => MOD_WIN as u32,
            _ => {
                return Err(anyhow!(
                    "unsupported Windows shortcut modifier {modifier:?}"
                ))
            }
        };
    }
    let virtual_key = match key {
        "Space" => VK_SPACE as u32,
        "Escape" => VK_ESCAPE as u32,
        value if value.len() == 1 => value.as_bytes()[0].to_ascii_uppercase() as u32,
        _ => return Err(anyhow!("unsupported Windows shortcut key {key:?}")),
    };
    Ok((modifiers, virtual_key))
}

fn send_enter() -> Result<()> {
    let inputs = [
        keyboard_input(VK_RETURN as u16, 0),
        keyboard_input(VK_RETURN as u16, KEYEVENTF_KEYUP),
    ];
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent != inputs.len() as u32 {
        return Err(anyhow!("SendInput failed while sending Enter"));
    }
    Ok(())
}

fn send_shift_left(character_count: usize) -> Result<()> {
    if character_count == 0 {
        return Ok(());
    }
    let mut inputs = Vec::with_capacity(character_count * 2 + 2);
    inputs.push(keyboard_input(VK_SHIFT as u16, 0));
    for _ in 0..character_count {
        inputs.push(keyboard_input(VK_LEFT as u16, 0));
        inputs.push(keyboard_input(VK_LEFT as u16, KEYEVENTF_KEYUP));
    }
    inputs.push(keyboard_input(VK_SHIFT as u16, KEYEVENTF_KEYUP));
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent != inputs.len() as u32 {
        return Err(anyhow!("SendInput failed while selecting pasted text"));
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
            match wparam as i32 {
                HOTKEY_ID => send_command(TrayCommand::Toggle),
                CANCEL_HOTKEY_ID => send_command(TrayCommand::Cancel),
                LANGUAGE_HOTKEY_ID => send_command(TrayCommand::CycleLanguage),
                _ => {}
            }
            0
        }
        WM_COMMAND => {
            match loword(wparam as usize) {
                MENU_SETTINGS => send_command(TrayCommand::Settings),
                MENU_COPY_LAST_TRANSCRIPT => send_command(TrayCommand::CopyLastTranscript),
                MENU_CANCEL => send_command(TrayCommand::Cancel),
                MENU_TEST_ENDPOINT => send_command(TrayCommand::TestEndpoint),
                MENU_OPEN_LOGS => send_command(TrayCommand::OpenLogs),
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

pub fn request_tray_exit() {
    if let Some(hwnd) = TRAY_HWND.get() {
        unsafe {
            let _ = PostMessageW(*hwnd, WM_COMMAND, MENU_QUIT, 0);
        }
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
    let mode_lock = STATUS_POPUP_MODE.get_or_init(|| Mutex::new(StatusPopupMode::Text));
    *mode_lock.lock().unwrap() = StatusPopupMode::Text;
    unsafe {
        if PostMessageW(hwnd, WM_STATUS_POPUP, 0, 0) == 0 {
            return Err(anyhow!("posting Windows status popup message failed"));
        }
    }
    Ok(())
}

pub fn show_recording_popup() -> Result<()> {
    let hwnd = *STATUS_POPUP_HWND
        .get()
        .ok_or_else(|| anyhow!("Windows status popup is not initialized"))?;
    let timeout_lock = STATUS_POPUP_TIMEOUT_MS.get_or_init(|| Mutex::new(None));
    *timeout_lock.lock().unwrap() = None;
    let mode_lock = STATUS_POPUP_MODE.get_or_init(|| Mutex::new(StatusPopupMode::Text));
    *mode_lock.lock().unwrap() = StatusPopupMode::RecordingBars;
    let frame_lock = STATUS_POPUP_FRAME.get_or_init(|| Mutex::new(0));
    *frame_lock.lock().unwrap() = 0;
    unsafe {
        if PostMessageW(hwnd, WM_STATUS_POPUP, 0, 0) == 0 {
            return Err(anyhow!("posting Windows recording popup message failed"));
        }
    }
    Ok(())
}

pub fn set_recording_language(language: Option<&str>) {
    *RECORDING_LANGUAGE
        .get_or_init(|| Mutex::new("AUTO".to_string()))
        .lock()
        .unwrap() = language.unwrap_or("auto").to_ascii_uppercase();
}

pub fn hide_status_popup() -> Result<()> {
    let hwnd = *STATUS_POPUP_HWND
        .get()
        .ok_or_else(|| anyhow!("Windows status popup is not initialized"))?;
    unsafe {
        if PostMessageW(hwnd, WM_STATUS_HIDE, 0, 0) == 0 {
            return Err(anyhow!("posting Windows status popup hide message failed"));
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
            KillTimer(hwnd, STATUS_POPUP_HIDE_TIMER_ID);
            KillTimer(hwnd, STATUS_POPUP_ANIMATION_TIMER_ID);
            let mode = *STATUS_POPUP_MODE
                .get_or_init(|| Mutex::new(StatusPopupMode::Text))
                .lock()
                .unwrap();
            if mode == StatusPopupMode::RecordingBars {
                SetTimer(hwnd, STATUS_POPUP_ANIMATION_TIMER_ID, 120, None);
            }
            let timeout_ms = STATUS_POPUP_TIMEOUT_MS
                .get_or_init(|| Mutex::new(None))
                .lock()
                .unwrap()
                .unwrap_or(0);
            if timeout_ms > 0 {
                SetTimer(hwnd, STATUS_POPUP_HIDE_TIMER_ID, timeout_ms, None);
            }
            0
        }
        WM_STATUS_HIDE => {
            KillTimer(hwnd, STATUS_POPUP_HIDE_TIMER_ID);
            KillTimer(hwnd, STATUS_POPUP_ANIMATION_TIMER_ID);
            ShowWindow(hwnd, SW_HIDE);
            0
        }
        WM_TIMER if wparam == STATUS_POPUP_HIDE_TIMER_ID => {
            KillTimer(hwnd, STATUS_POPUP_HIDE_TIMER_ID);
            KillTimer(hwnd, STATUS_POPUP_ANIMATION_TIMER_ID);
            ShowWindow(hwnd, SW_HIDE);
            0
        }
        WM_TIMER if wparam == STATUS_POPUP_ANIMATION_TIMER_ID => {
            let frame_lock = STATUS_POPUP_FRAME.get_or_init(|| Mutex::new(0));
            let mut frame = frame_lock.lock().unwrap();
            *frame = (*frame).wrapping_add(1);
            InvalidateRect(hwnd, std::ptr::null(), 1);
            0
        }
        WM_PAINT => {
            paint_status_popup(hwnd);
            0
        }
        WM_LBUTTONUP => {
            let mode = *STATUS_POPUP_MODE
                .get_or_init(|| Mutex::new(StatusPopupMode::Text))
                .lock()
                .unwrap();
            if mode == StatusPopupMode::RecordingBars {
                send_command(TrayCommand::CycleLanguage);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn position_status_popup(hwnd: HWND) {
    let width = STATUS_POPUP_WIDTH;
    let height = STATUS_POPUP_HEIGHT;
    let x = (GetSystemMetrics(SM_CXSCREEN) - width) / 2;
    let y = GetSystemMetrics(SM_CYSCREEN) - height - 64;
    let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, height, height);
    if region != 0 {
        SetWindowRgn(hwnd, region, 1);
    }
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
        right: STATUS_POPUP_WIDTH,
        bottom: STATUS_POPUP_HEIGHT,
    };
    let (background, border, foreground) = tray_palette(
        *TRAY_THEME
            .get_or_init(|| Mutex::new(Theme::System))
            .lock()
            .unwrap(),
    );
    let brush = CreateSolidBrush(background);
    let pen = CreatePen(PS_SOLID, 1, border);
    let old_brush = SelectObject(hdc, brush);
    let old_pen = SelectObject(hdc, pen);
    RoundRect(
        hdc,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        STATUS_POPUP_HEIGHT,
        STATUS_POPUP_HEIGHT,
    );
    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_brush);
    DeleteObject(pen);
    DeleteObject(brush);

    let mode = *STATUS_POPUP_MODE
        .get_or_init(|| Mutex::new(StatusPopupMode::Text))
        .lock()
        .unwrap();
    if mode == StatusPopupMode::RecordingBars {
        draw_recording_bars(hdc, foreground);
    } else {
        SetBkMode(hdc, TRANSPARENT as i32);
        SetTextColor(hdc, foreground);
        let text = STATUS_POPUP_TEXT
            .get_or_init(|| Mutex::new(String::new()))
            .lock()
            .unwrap()
            .clone();
        let mut text_rect = RECT {
            left: 14,
            top: 0,
            right: STATUS_POPUP_WIDTH - 14,
            bottom: STATUS_POPUP_HEIGHT,
        };
        DrawTextW(
            hdc,
            wide(&text).as_ptr(),
            -1,
            &mut text_rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
    }
    EndPaint(hwnd, &ps);
}

unsafe fn draw_recording_bars(hdc: isize, foreground: COLORREF) {
    let frame = *STATUS_POPUP_FRAME
        .get_or_init(|| Mutex::new(0))
        .lock()
        .unwrap();
    let heights = [12, 18, 24, 28, 22, 16, 10, 14, 20, 26, 18, 12];
    let bar_width = 7;
    let gap = 4;
    SetBkMode(hdc, TRANSPARENT as i32);
    SetTextColor(hdc, foreground);
    let language = RECORDING_LANGUAGE
        .get_or_init(|| Mutex::new("AUTO".to_string()))
        .lock()
        .unwrap()
        .clone();
    let mut language_rect = RECT {
        left: 14,
        top: 0,
        right: 64,
        bottom: STATUS_POPUP_HEIGHT,
    };
    DrawTextW(
        hdc,
        wide(&language).as_ptr(),
        -1,
        &mut language_rect,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    let total_width = heights.len() as i32 * bar_width + (heights.len() as i32 - 1) * gap;
    let mut x = 70 + (STATUS_POPUP_WIDTH - 70 - total_width) / 2;
    let brush = CreateSolidBrush(foreground);
    for index in 0..heights.len() {
        let height = heights[((index as u32 + frame) as usize) % heights.len()];
        let y = (STATUS_POPUP_HEIGHT - height) / 2;
        let rect = RECT {
            left: x,
            top: y,
            right: x + bar_width,
            bottom: y + height,
        };
        FillRect(hdc, &rect, brush);
        x += bar_width + gap;
    }
    DeleteObject(brush);
}

fn tray_palette(theme: Theme) -> (COLORREF, COLORREF, COLORREF) {
    match theme {
        Theme::Light => (rgb(247, 248, 250), rgb(203, 213, 225), rgb(32, 36, 42)),
        Theme::Dark | Theme::System => (rgb(17, 20, 24), rgb(48, 56, 66), rgb(244, 247, 250)),
    }
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
    let has_last = crate::core::config::Config::load_default()
        .ok()
        .and_then(|loaded| crate::core::history::HistoryStore::from_config(&loaded.config).ok())
        .and_then(|store| store.last_transcript().ok().flatten())
        .is_some();
    let model = crate::core::settings_ui::TrayMenuModel::for_state(current_tray_state(), has_last);
    for item in model.items {
        let id = match item.action {
            crate::core::settings_ui::TrayMenuAction::Settings => MENU_SETTINGS,
            crate::core::settings_ui::TrayMenuAction::CopyLastTranscript => {
                MENU_COPY_LAST_TRANSCRIPT
            }
            crate::core::settings_ui::TrayMenuAction::Cancel => MENU_CANCEL,
            crate::core::settings_ui::TrayMenuAction::TestEndpoint => MENU_TEST_ENDPOINT,
            crate::core::settings_ui::TrayMenuAction::OpenLogs => MENU_OPEN_LOGS,
            crate::core::settings_ui::TrayMenuAction::OpenConfig => MENU_OPEN_CONFIG,
            crate::core::settings_ui::TrayMenuAction::Quit => MENU_QUIT,
        };
        AppendMenuW(
            menu,
            if item.enabled { 0 } else { 0x0000_0001 },
            id,
            wide(&item.label).as_ptr(),
        );
    }

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

fn open_logs_folder() -> Result<()> {
    let loaded = crate::core::config::Config::load_default()?;
    let path = crate::core::logging::FileLogger::from_config(&loaded.config)?
        .directory()
        .to_path_buf();
    std::fs::create_dir_all(&path)?;
    std::process::Command::new("explorer.exe")
        .arg(path)
        .spawn()?;
    Ok(())
}

fn open_settings_panel() -> Result<()> {
    let exe = std::env::current_exe().context("locating OpenVoiceDesktop executable")?;
    std::process::Command::new(exe).arg("settings").spawn()?;
    Ok(())
}

pub async fn handle_tray_command(command: TrayCommand) -> Result<bool> {
    match command {
        TrayCommand::Toggle => Ok(true),
        TrayCommand::Cancel => Ok(false),
        TrayCommand::CycleLanguage => Ok(false),
        TrayCommand::Settings => {
            tokio::task::spawn_blocking(open_settings_panel).await??;
            Ok(false)
        }
        TrayCommand::CopyLastTranscript => {
            let loaded = crate::core::config::Config::load_default()?;
            let store = crate::core::history::HistoryStore::from_config(&loaded.config)?;
            let text = store
                .last_transcript()?
                .ok_or_else(|| anyhow!("no transcript available"))?;
            WindowsPlatformAdapter.set_clipboard_text(&text).await?;
            show_status_popup("Last transcript copied.", Some(2_500)).ok();
            Ok(false)
        }
        TrayCommand::TestEndpoint => {
            let config = crate::core::config::Config::load_default()?.config;
            let diagnostic = crate::core::asr::test_endpoint(&config).await?;
            show_status_popup(
                &format!(
                    "HTTP {} - {} ms",
                    diagnostic.http_status, diagnostic.latency_ms
                ),
                Some(3_500),
            )
            .ok();
            Ok(false)
        }
        TrayCommand::OpenLogs => {
            tokio::task::spawn_blocking(open_logs_folder).await??;
            Ok(false)
        }
        TrayCommand::OpenConfigFolder => {
            tokio::task::spawn_blocking(open_config_folder).await??;
            Ok(false)
        }
        TrayCommand::ReloadLifecycle { show_tray_icon } => {
            set_tray_visibility(show_tray_icon)?;
            Ok(false)
        }
        TrayCommand::Quit => Ok(false),
    }
}

fn set_tray_visibility(visible: bool) -> Result<()> {
    let hwnd = *TRAY_HWND
        .get()
        .ok_or_else(|| anyhow!("Windows tray window is not initialized"))?;
    unsafe {
        if visible {
            if !TRAY_ICON_VISIBLE.swap(true, Ordering::Relaxed) {
                add_tray_icon(hwnd)?;
            }
            Ok(())
        } else {
            if TRAY_ICON_VISIBLE.swap(false, Ordering::Relaxed) {
                remove_tray_icon(hwnd);
            }
            Ok(())
        }
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
