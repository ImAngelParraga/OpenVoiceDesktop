use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::sync::{
    atomic::{AtomicBool, AtomicU8, Ordering},
    Arc, Mutex, OnceLock,
};
use std::time::Duration;
use tokio::sync::mpsc;
use windows_sys::core::{GUID, HRESULT};
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreatePen, CreateRoundRectRgn, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint,
    FillRect, GetMonitorInfoW, InvalidateRect, MonitorFromPoint, RoundRect, SelectObject,
    SetBkMode, SetTextColor, SetWindowRgn, DT_CENTER, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER,
    MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows_sys::Win32::Media::Audio::{eMultimedia, eRender};
use windows_sys::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, SendInput, UnregisterHotKey, INPUT, INPUT_0, INPUT_KEYBOARD,
    KEYBDINPUT, KEYEVENTF_KEYUP, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, VK_BACK,
    VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_F1, VK_LEFT, VK_LWIN, VK_MENU, VK_RETURN, VK_RIGHT, VK_RWIN,
    VK_SHIFT, VK_SPACE, VK_TAB, VK_UP, VK_V,
};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconFromResourceEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
    DestroyMenu, DispatchMessageW, GetClientRect, GetCursorPos, GetMessageW, IsWindowVisible,
    KillTimer, LoadCursorW, LoadIconW, MessageBoxW, PostMessageW, PostQuitMessage, RegisterClassW,
    SetForegroundWindow, SetTimer, SetWindowPos, ShowWindow, TrackPopupMenu, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, HMENU, HWND_TOPMOST, IDC_ARROW, IDC_HAND,
    IDI_APPLICATION, MB_ICONERROR, MB_OK, MSG, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE,
    SW_SHOWNOACTIVATE, TPM_BOTTOMALIGN, TPM_LEFTALIGN, WM_APP, WM_COMMAND, WM_DESTROY, WM_HOTKEY,
    WM_LBUTTONUP, WM_PAINT, WM_RBUTTONUP, WM_SETTINGCHANGE, WM_THEMECHANGED, WM_TIMER, WNDCLASSW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW, WS_POPUP,
};

use crate::core::audio::{play_audio_feedback, AudioFeedbackPlan};
use crate::core::config::{OverlayPosition, OverlayStyle, RecordingOutputMode, Theme, TypingTool};
use crate::core::platform::{
    post_paste_selection_plan, reduced_output_volume, AudioOutputState, PlatformAdapter,
};

const LOGO_ICO: &[u8] = include_bytes!("../../assets/logo.ico");
const HOTKEY_ID: i32 = 1;
const CANCEL_HOTKEY_ID: i32 = 2;
const LANGUAGE_HOTKEY_ID: i32 = 3;
const TRAY_ID: u32 = 1;
const WM_TRAYICON: u32 = WM_APP + 1;
const WM_STATUS_POPUP: u32 = WM_APP + 2;
const WM_STATUS_HIDE: u32 = WM_APP + 3;
const WM_STATUS_RECONFIGURE: u32 = WM_APP + 4;
const STATUS_POPUP_HIDE_TIMER_ID: usize = 1;
const STATUS_POPUP_ANIMATION_TIMER_ID: usize = 2;
const HOTKEY_TIMER_ID: usize = 3;
const WM_RELOAD_HOTKEYS: u32 = WM_APP + 5;
const STATUS_POPUP_WIDTH: i32 = 180;
const STATUS_POPUP_HEIGHT: i32 = 34;
const POST_PASTE_SELECTION_DELAY_MS: u64 = 50;
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
static POPUP_CONFIG: OnceLock<Mutex<WindowsPopupConfig>> = OnceLock::new();
static HOTKEYS: OnceLock<Mutex<ActiveHotkeys>> = OnceLock::new();
static HOTKEY_RELOADS: OnceLock<
    Mutex<VecDeque<(WindowsHotkeys, tokio::sync::oneshot::Sender<Result<()>>)>>,
> = OnceLock::new();
static STATUS_SNAPSHOT: OnceLock<Arc<crate::core::status::StatusSnapshot>> = OnceLock::new();
static CANCEL_FLAG: OnceLock<Arc<AtomicBool>> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
pub struct WindowsPopupConfig {
    pub show: bool,
    pub position: OverlayPosition,
    pub style: OverlayStyle,
    pub theme: Theme,
}

impl Default for WindowsPopupConfig {
    fn default() -> Self {
        Self {
            show: true,
            position: OverlayPosition::Bottom,
            style: OverlayStyle::Compact,
            theme: Theme::System,
        }
    }
}

impl From<&crate::core::config::Config> for WindowsPopupConfig {
    fn from(config: &crate::core::config::Config) -> Self {
        Self {
            show: config.app.show_overlay,
            position: config.app.overlay_position,
            style: config.app.overlay_style,
            theme: config.ui.theme,
        }
    }
}

fn popup_config() -> WindowsPopupConfig {
    *POPUP_CONFIG
        .get_or_init(|| Mutex::new(WindowsPopupConfig::default()))
        .lock()
        .unwrap()
}

pub fn set_popup_config(config: WindowsPopupConfig) -> Result<()> {
    *POPUP_CONFIG
        .get_or_init(|| Mutex::new(WindowsPopupConfig::default()))
        .lock()
        .unwrap() = config;
    if let Some(hwnd) = STATUS_POPUP_HWND.get() {
        unsafe {
            if PostMessageW(*hwnd, WM_STATUS_RECONFIGURE, 0, 0) == 0 {
                return Err(anyhow!("posting Windows popup reconfiguration failed"));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HotkeySpec {
    modifiers: u32,
    key: u32,
    label: String,
}

impl HotkeySpec {
    fn held(&self) -> bool {
        unsafe {
            let down = |key: u16| GetAsyncKeyState(i32::from(key)) as u16 & 0x8000 != 0;
            down(self.key as u16)
                && (self.modifiers & MOD_CONTROL as u32 == 0 || down(VK_CONTROL))
                && (self.modifiers & MOD_ALT as u32 == 0 || down(VK_MENU))
                && (self.modifiers & MOD_SHIFT as u32 == 0 || down(VK_SHIFT))
                && (self.modifiers & MOD_WIN as u32 == 0 || down(VK_LWIN) || down(VK_RWIN))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowsHotkeys {
    toggle: HotkeySpec,
    cancel: HotkeySpec,
    language: HotkeySpec,
    push_to_talk: bool,
}

impl WindowsHotkeys {
    fn from_config(config: &crate::core::config::Config) -> Result<Self> {
        Ok(Self {
            toggle: windows_hotkey(&config.behavior.shortcut)?,
            cancel: windows_hotkey(&config.behavior.cancel_shortcut)?,
            language: windows_hotkey(&config.behavior.language_shortcut)?,
            push_to_talk: config.behavior.push_to_talk,
        })
    }
}

struct ActiveHotkeys {
    bindings: WindowsHotkeys,
    cancel_registered: bool,
    cancel_failure_reported: bool,
    push_active: bool,
}

unsafe fn register_hotkey(hwnd: HWND, id: i32, spec: &HotkeySpec) -> Result<()> {
    if RegisterHotKey(hwnd, id, spec.modifiers, spec.key) == 0 {
        return Err(anyhow!(
            "Windows shortcut {} is unavailable or already in use",
            spec.label
        ));
    }
    Ok(())
}

unsafe fn register_hotkey_set(hwnd: HWND, bindings: &WindowsHotkeys) -> Result<bool> {
    register_hotkey(hwnd, HOTKEY_ID, &bindings.toggle)?;
    if let Err(error) = register_hotkey(hwnd, LANGUAGE_HOTKEY_ID, &bindings.language) {
        UnregisterHotKey(hwnd, HOTKEY_ID);
        return Err(error);
    }
    let active = STATUS_SNAPSHOT
        .get()
        .is_some_and(|snapshot| snapshot.state() != crate::core::state::RecordingState::Idle);
    if active {
        if let Err(error) = register_hotkey(hwnd, CANCEL_HOTKEY_ID, &bindings.cancel) {
            UnregisterHotKey(hwnd, HOTKEY_ID);
            UnregisterHotKey(hwnd, LANGUAGE_HOTKEY_ID);
            return Err(error);
        }
    }
    Ok(active)
}

unsafe fn unregister_hotkey_set(hwnd: HWND) {
    UnregisterHotKey(hwnd, HOTKEY_ID);
    UnregisterHotKey(hwnd, CANCEL_HOTKEY_ID);
    UnregisterHotKey(hwnd, LANGUAGE_HOTKEY_ID);
}

unsafe fn replace_hotkey_set(hwnd: HWND, next: WindowsHotkeys) -> Result<()> {
    let Some(hotkeys) = HOTKEYS.get() else {
        return Err(anyhow!("Windows shortcuts are not initialized"));
    };
    let mut active = hotkeys.lock().unwrap();
    if active.bindings == next {
        return Ok(());
    }
    let previous = active.bindings.clone();
    unregister_hotkey_set(hwnd);
    match register_hotkey_set(hwnd, &next) {
        Ok(cancel_registered) => {
            active.bindings = next;
            active.cancel_registered = cancel_registered;
            active.cancel_failure_reported = false;
            active.push_active = false;
            Ok(())
        }
        Err(error) => {
            active.cancel_registered =
                register_hotkey_set(hwnd, &previous).unwrap_or_else(|rollback| {
                    eprintln!("openvoice-desktop shortcut rollback failed: {rollback}");
                    false
                });
            Err(error)
        }
    }
}

pub async fn apply_hotkey_config(config: &crate::core::config::Config) -> Result<()> {
    let next = WindowsHotkeys::from_config(config)?;
    let hwnd = *TRAY_HWND
        .get()
        .ok_or_else(|| anyhow!("Windows tray window is not initialized"))?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    HOTKEY_RELOADS
        .get_or_init(|| Mutex::new(VecDeque::new()))
        .lock()
        .unwrap()
        .push_back((next, tx));
    unsafe {
        if PostMessageW(hwnd, WM_RELOAD_HOTKEYS, 0, 0) == 0 {
            HOTKEY_RELOADS.get().unwrap().lock().unwrap().pop_back();
            return Err(anyhow!("posting Windows shortcut reload failed"));
        }
    }
    rx.await
        .map_err(|_| anyhow!("Windows shortcut reload stopped"))?
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusPopupMode {
    Text,
    RecordingBars,
}

#[derive(Debug, Clone, Copy)]
pub enum TrayCommand {
    Toggle,
    PushToTalkReleased,
    CycleLanguage,
    Settings,
    CopyLastTranscript,
    Cancel,
    TestEndpoint,
    OpenLogs,
    OpenConfigFolder,
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
                tokio::time::sleep(Duration::from_millis(POST_PASTE_SELECTION_DELAY_MS)).await;
                let plan = post_paste_selection_plan(text);
                send_left_selection(plan.cursor_left, plan.select_left)
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

struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self> {
        let result = unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) };
        check_hresult(result, "initializing Windows audio COM session")?;
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

#[derive(Default)]
struct ComObject(*mut c_void);

impl ComObject {
    fn out(&mut self) -> *mut *mut c_void {
        debug_assert!(self.0.is_null());
        &mut self.0
    }

    fn get(&self, operation: &str) -> Result<*mut c_void> {
        if self.0.is_null() {
            Err(anyhow!("{operation} returned a null COM interface"))
        } else {
            Ok(self.0)
        }
    }
}

impl Drop for ComObject {
    fn drop(&mut self) {
        unsafe { release_com(self.0) };
    }
}

fn windows_output_state(
    set_muted: Option<bool>,
    set_volume: Option<f32>,
) -> Result<AudioOutputState> {
    let _apartment = ComApartment::initialize()?;
    unsafe {
        let mut enumerator = ComObject::default();
        check_hresult(
            CoCreateInstance(
                &CLSID_MM_DEVICE_ENUMERATOR,
                std::ptr::null_mut(),
                CLSCTX_ALL,
                &IID_MM_DEVICE_ENUMERATOR,
                enumerator.out(),
            ),
            "creating Windows audio device enumerator",
        )?;
        let enumerator_ptr = enumerator.get("creating Windows audio device enumerator")?;
        let enumerator_vtable = *(enumerator_ptr as *mut *mut DeviceEnumeratorVtable);
        let mut device = ComObject::default();
        check_hresult(
            ((*enumerator_vtable).get_default_audio_endpoint)(
                enumerator_ptr,
                eRender,
                eMultimedia,
                device.out(),
            ),
            "getting Windows default output device",
        )?;
        let device_ptr = device.get("getting Windows default output device")?;
        let device_vtable = *(device_ptr as *mut *mut DeviceVtable);
        let mut endpoint = ComObject::default();
        check_hresult(
            ((*device_vtable).activate)(
                device_ptr,
                &IID_AUDIO_ENDPOINT_VOLUME,
                CLSCTX_ALL,
                std::ptr::null(),
                endpoint.out(),
            ),
            "activating Windows endpoint volume",
        )?;
        let endpoint_ptr = endpoint.get("activating Windows endpoint volume")?;
        let endpoint_vtable = *(endpoint_ptr as *mut *mut EndpointVolumeVtable);
        let mut muted = 0_i32;
        let mut volume = 0.0_f32;
        check_hresult(
            ((*endpoint_vtable).get_mute)(endpoint_ptr, &mut muted),
            "reading Windows output mute state",
        )?;
        check_hresult(
            ((*endpoint_vtable).get_master_volume_level_scalar)(endpoint_ptr, &mut volume),
            "reading Windows output volume",
        )?;
        if let Some(set_volume) = set_volume {
            check_hresult(
                ((*endpoint_vtable).set_master_volume_level_scalar)(
                    endpoint_ptr,
                    set_volume.clamp(0.0, 1.0),
                    std::ptr::null(),
                ),
                "setting Windows output volume",
            )?;
        }
        if let Some(set_muted) = set_muted {
            check_hresult(
                ((*endpoint_vtable).set_mute)(endpoint_ptr, i32::from(set_muted), std::ptr::null()),
                "setting Windows output mute state",
            )?;
        }
        Ok(AudioOutputState {
            muted: muted != 0,
            volume: volume.clamp(0.0, 1.0),
        })
    }
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

#[cfg(test)]
mod com_ownership_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[repr(C)]
    struct FakeComObject {
        vtable: *const usize,
        releases: *const AtomicUsize,
    }

    unsafe extern "system" fn fake_release(value: *mut c_void) -> u32 {
        let object = &*(value as *const FakeComObject);
        (*object.releases).fetch_add(1, Ordering::SeqCst);
        0
    }

    #[test]
    fn each_acquired_interface_releases_on_later_failure() {
        for fail_after in 1..=3 {
            let releases = std::array::from_fn::<_, 3, _>(|_| AtomicUsize::new(0));
            let vtable = [0, 0, fake_release as *const () as usize];
            let mut objects = std::array::from_fn::<_, 3, _>(|index| FakeComObject {
                vtable: vtable.as_ptr(),
                releases: &releases[index],
            });

            let result = (|| -> Result<()> {
                let mut enumerator = ComObject::default();
                unsafe { *enumerator.out() = (&mut objects[0] as *mut FakeComObject).cast() };
                if fail_after == 1 {
                    check_hresult(-1, "fake endpoint lookup")?;
                }
                let mut device = ComObject::default();
                unsafe { *device.out() = (&mut objects[1] as *mut FakeComObject).cast() };
                if fail_after == 2 {
                    check_hresult(-1, "fake activation")?;
                }
                let mut endpoint = ComObject::default();
                unsafe { *endpoint.out() = (&mut objects[2] as *mut FakeComObject).cast() };
                check_hresult(-1, "fake volume read")?;
                Ok(())
            })();

            assert!(result.is_err());
            for (index, counter) in releases.iter().enumerate() {
                assert_eq!(
                    counter.load(Ordering::SeqCst),
                    usize::from(index < fail_after),
                    "failure after interface {fail_after} leaked or double-released interface {index}"
                );
            }
        }
    }

    #[test]
    fn successful_hresult_with_null_interface_is_rejected() {
        let interface = ComObject::default();
        assert!(interface.get("fake allocation").is_err());
    }
}

pub fn run_tray_loop(tx: mpsc::UnboundedSender<TrayCommand>) -> Result<()> {
    run_tray_loop_with_config(
        tx,
        &crate::core::config::Config::default(),
        Arc::new(crate::core::status::StatusSnapshot::new()),
        Arc::new(AtomicBool::new(false)),
    )
}

pub fn run_tray_loop_with_config(
    tx: mpsc::UnboundedSender<TrayCommand>,
    config: &crate::core::config::Config,
    snapshot: Arc<crate::core::status::StatusSnapshot>,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()> {
    set_popup_config(WindowsPopupConfig::from(config))?;
    let bindings = WindowsHotkeys::from_config(config)?;
    STATUS_SNAPSHOT
        .set(snapshot)
        .map_err(|_| anyhow!("Windows status snapshot was already initialized"))?;
    CANCEL_FLAG
        .set(cancel_flag)
        .map_err(|_| anyhow!("Windows cancel flag was already initialized"))?;
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
            hCursor: LoadCursorW(0, IDC_ARROW),
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

        let cancel_registered = register_hotkey_set(hwnd, &bindings).map_err(|error| {
            let _ = message_box("OpenVoiceDesktop shortcut unavailable", &error.to_string());
            error
        })?;
        HOTKEYS
            .set(Mutex::new(ActiveHotkeys {
                bindings,
                cancel_registered,
                cancel_failure_reported: false,
                push_active: false,
            }))
            .map_err(|_| anyhow!("Windows shortcuts were already initialized"))?;
        if SetTimer(hwnd, HOTKEY_TIMER_ID, 40, None) == 0 {
            unregister_hotkey_set(hwnd);
            return Err(anyhow!("starting Windows shortcut lifecycle timer failed"));
        }

        if config.app.show_tray_icon {
            add_tray_icon(hwnd)?;
            TRAY_ICON_VISIBLE.store(true, Ordering::Relaxed);
        }
        if config.app.start_hidden {
            ShowWindow(hwnd, SW_HIDE);
        }

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        if TRAY_ICON_VISIBLE.swap(false, Ordering::Relaxed) {
            let _ = remove_tray_icon(hwnd);
        }
        KillTimer(hwnd, HOTKEY_TIMER_ID);
        unregister_hotkey_set(hwnd);
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

fn windows_hotkey(shortcut: &str) -> Result<HotkeySpec> {
    let parsed = crate::core::shortcut::ParsedShortcut::parse(shortcut)?;
    let mut parts = parsed.as_str().split('+').collect::<Vec<_>>();
    let key = parts.pop().ok_or_else(|| anyhow!("shortcut has no key"))?;
    let mut modifiers = MOD_NOREPEAT as u32;
    for modifier in parts {
        modifiers |= match modifier {
            "Ctrl" => MOD_CONTROL as u32,
            "Alt" => MOD_ALT as u32,
            "Shift" => MOD_SHIFT as u32,
            "Super" => MOD_WIN as u32,
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
        "Enter" => VK_RETURN as u32,
        "Tab" => VK_TAB as u32,
        "Backspace" => VK_BACK as u32,
        "Left" => VK_LEFT as u32,
        "Right" => VK_RIGHT as u32,
        "Up" => VK_UP as u32,
        "Down" => VK_DOWN as u32,
        value if value.starts_with('F') && value[1..].parse::<u32>().is_ok() => {
            VK_F1 as u32 + value[1..].parse::<u32>()? - 1
        }
        value if value.len() == 1 => value.as_bytes()[0].to_ascii_uppercase() as u32,
        _ => return Err(anyhow!("unsupported Windows shortcut key {key:?}")),
    };
    Ok(HotkeySpec {
        modifiers,
        key: virtual_key,
        label: parsed.as_str().to_string(),
    })
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

fn send_left_selection(cursor_left: usize, select_left: usize) -> Result<()> {
    if cursor_left == 0 && select_left == 0 {
        return Ok(());
    }
    let mut inputs = Vec::with_capacity((cursor_left + select_left) * 2 + 2);
    for _ in 0..cursor_left {
        inputs.push(keyboard_input(VK_LEFT as u16, 0));
        inputs.push(keyboard_input(VK_LEFT as u16, KEYEVENTF_KEYUP));
    }
    if select_left > 0 {
        inputs.push(keyboard_input(VK_SHIFT as u16, 0));
    }
    for _ in 0..select_left {
        inputs.push(keyboard_input(VK_LEFT as u16, 0));
        inputs.push(keyboard_input(VK_LEFT as u16, KEYEVENTF_KEYUP));
    }
    if select_left > 0 {
        inputs.push(keyboard_input(VK_SHIFT as u16, KEYEVENTF_KEYUP));
    }
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
                HOTKEY_ID => {
                    if let Some(hotkeys) = HOTKEYS.get() {
                        let mut active = hotkeys.lock().unwrap();
                        if !active.bindings.push_to_talk || !active.push_active {
                            active.push_active = active.bindings.push_to_talk;
                            send_command(TrayCommand::Toggle);
                        }
                    }
                }
                CANCEL_HOTKEY_ID => send_command(TrayCommand::Cancel),
                LANGUAGE_HOTKEY_ID => send_command(TrayCommand::CycleLanguage),
                _ => {}
            }
            0
        }
        WM_RELOAD_HOTKEYS => {
            if let Some(queue) = HOTKEY_RELOADS.get() {
                while let Some((next, reply)) = queue.lock().unwrap().pop_front() {
                    let _ = reply.send(replace_hotkey_set(hwnd, next));
                }
            }
            0
        }
        WM_TIMER if wparam == HOTKEY_TIMER_ID => {
            sync_hotkey_lifecycle(hwnd);
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
    if matches!(command, TrayCommand::Cancel) {
        if let Some(flag) = CANCEL_FLAG.get() {
            flag.store(true, Ordering::SeqCst);
        }
    }
    if let Some(tx) = COMMAND_TX.get() {
        let _ = tx.send(command);
    }
}

unsafe fn sync_hotkey_lifecycle(hwnd: HWND) {
    let Some(snapshot) = STATUS_SNAPSHOT.get() else {
        return;
    };
    let Some(hotkeys) = HOTKEYS.get() else {
        return;
    };
    let mut active = hotkeys.lock().unwrap();
    let wanted = snapshot.state() != crate::core::state::RecordingState::Idle;
    if wanted && !active.cancel_registered && !active.cancel_failure_reported {
        match register_hotkey(hwnd, CANCEL_HOTKEY_ID, &active.bindings.cancel) {
            Ok(()) => active.cancel_registered = true,
            Err(error) => {
                active.cancel_failure_reported = true;
                let _ = message_box(
                    "OpenVoiceDesktop cancel shortcut unavailable",
                    &error.to_string(),
                );
            }
        }
    } else if !wanted {
        if active.cancel_registered {
            UnregisterHotKey(hwnd, CANCEL_HOTKEY_ID);
            active.cancel_registered = false;
        }
        active.cancel_failure_reported = false;
    }
    if active.push_active && !active.bindings.toggle.held() {
        active.push_active = false;
        send_command(TrayCommand::PushToTalkReleased);
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

unsafe fn remove_tray_icon(hwnd: HWND) -> Result<()> {
    let mut data: NOTIFYICONDATAW = std::mem::zeroed();
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = TRAY_ID;
    if Shell_NotifyIconW(NIM_DELETE, &mut data) == 0 {
        return Err(anyhow!("removing Windows tray icon failed"));
    }
    Ok(())
}

pub fn show_status_popup(text: &str, timeout_ms: Option<u32>) -> Result<()> {
    if !popup_config().show {
        return Ok(());
    }
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
    if !popup_config().show {
        return Ok(());
    }
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
        hCursor: LoadCursorW(0, IDC_HAND),
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
            if !popup_config().show {
                ShowWindow(hwnd, SW_HIDE);
                return 0;
            }
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
        WM_STATUS_RECONFIGURE => {
            if !popup_config().show {
                KillTimer(hwnd, STATUS_POPUP_HIDE_TIMER_ID);
                KillTimer(hwnd, STATUS_POPUP_ANIMATION_TIMER_ID);
                ShowWindow(hwnd, SW_HIDE);
            } else if IsWindowVisible(hwnd) != 0 {
                position_status_popup(hwnd);
                InvalidateRect(hwnd, std::ptr::null(), 1);
            }
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
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            InvalidateRect(hwnd, std::ptr::null(), 1);
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
    let config = popup_config();
    let (logical_width, logical_height) = match config.style {
        OverlayStyle::Compact => (STATUS_POPUP_WIDTH, STATUS_POPUP_HEIGHT),
        OverlayStyle::Expanded => (300, 50),
    };
    let mut cursor = POINT { x: 0, y: 0 };
    GetCursorPos(&mut cursor);
    let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
    let mut info: MONITORINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if monitor == 0 || GetMonitorInfoW(monitor, &mut info) == 0 {
        return;
    }
    let mut dpi_x = 96;
    let mut dpi_y = 96;
    if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) < 0 {
        dpi_x = 96;
        dpi_y = 96;
    }
    let width = logical_width * dpi_x as i32 / 96;
    let height = logical_height * dpi_y as i32 / 96;
    let work = info.rcWork;
    let x = work.left + (work.right - work.left - width) / 2;
    let y = match config.position {
        OverlayPosition::Top => work.top + 32,
        OverlayPosition::Bottom => work.bottom - height - 64,
    };
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
    let mut rect: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut rect);
    let (background, border, foreground) = tray_palette(popup_config().theme);
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
        rect.bottom,
        rect.bottom,
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
        draw_recording_bars(hdc, foreground, rect.right, rect.bottom);
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
            right: rect.right - 14,
            bottom: rect.bottom,
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

unsafe fn draw_recording_bars(hdc: isize, foreground: COLORREF, width: i32, height: i32) {
    let frame = *STATUS_POPUP_FRAME
        .get_or_init(|| Mutex::new(0))
        .lock()
        .unwrap();
    let heights = [10, 16, 22, 28, 20, 14, 9];
    let bar_width = 6;
    let gap = 3;
    SetBkMode(hdc, TRANSPARENT as i32);
    SetTextColor(hdc, foreground);
    let language = RECORDING_LANGUAGE
        .get_or_init(|| Mutex::new("AUTO".to_string()))
        .lock()
        .unwrap()
        .clone();
    let mut language_rect = RECT {
        left: 10,
        top: 0,
        right: 52,
        bottom: height,
    };
    DrawTextW(
        hdc,
        wide(&language).as_ptr(),
        -1,
        &mut language_rect,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    let total_width = heights.len() as i32 * bar_width + (heights.len() as i32 - 1) * gap;
    let mut x = 56 + (width - 56 - total_width) / 2;
    let brush = CreateSolidBrush(foreground);
    for index in 0..heights.len() {
        let bar_height = heights[((index as u32 + frame) as usize) % heights.len()];
        let y = (height - bar_height) / 2;
        let rect = RECT {
            left: x,
            top: y,
            right: x + bar_width,
            bottom: y + bar_height,
        };
        FillRect(hdc, &rect, brush);
        x += bar_width + gap;
    }
    DeleteObject(brush);
}

fn tray_palette(theme: Theme) -> (COLORREF, COLORREF, COLORREF) {
    match theme {
        Theme::Light => (rgb(247, 248, 250), rgb(203, 213, 225), rgb(32, 36, 42)),
        Theme::System if system_uses_light_theme() => {
            (rgb(247, 248, 250), rgb(203, 213, 225), rgb(32, 36, 42))
        }
        Theme::Dark | Theme::System => (rgb(17, 20, 24), rgb(48, 56, 66), rgb(244, 247, 250)),
    }
}

fn system_uses_light_theme() -> bool {
    let mut value = 1u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize").as_ptr(),
            wide("AppsUseLightTheme").as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut value as *mut u32).cast(),
            &mut size,
        )
    };
    status != 0 || value != 0
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
        TrayCommand::Toggle | TrayCommand::PushToTalkReleased => Ok(true),
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
        TrayCommand::Quit => Ok(false),
    }
}

pub fn apply_lifecycle(show_tray_icon: bool, popup: WindowsPopupConfig) -> Result<()> {
    set_tray_visibility(show_tray_icon)?;
    set_popup_config(popup)
}

fn set_tray_visibility(visible: bool) -> Result<()> {
    let hwnd = *TRAY_HWND
        .get()
        .ok_or_else(|| anyhow!("Windows tray window is not initialized"))?;
    unsafe {
        if visible {
            if !TRAY_ICON_VISIBLE.load(Ordering::Relaxed) {
                add_tray_icon(hwnd)?;
                TRAY_ICON_VISIBLE.store(true, Ordering::Relaxed);
            }
            Ok(())
        } else {
            if TRAY_ICON_VISIBLE.load(Ordering::Relaxed) {
                remove_tray_icon(hwnd)?;
                TRAY_ICON_VISIBLE.store(false, Ordering::Relaxed);
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

pub fn report_deferred_reload_error() {
    std::thread::spawn(|| {
        let _ = message_box(
            "OpenVoiceDesktop settings could not be applied",
            "Saved settings could not be applied after dictation. Reopen Settings and save again, or restart OpenVoiceDesktop.",
        );
    });
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
