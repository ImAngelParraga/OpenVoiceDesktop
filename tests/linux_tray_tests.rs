#![cfg(target_os = "linux")]

use openvoice_desktop::core::settings_ui::{TrayIconState, TrayMenuAction, TrayMenuModel};
use openvoice_desktop::platform::linux_tray::{
    icon_color, render_icon, toggle_label, tooltip_description,
};

const DAEMON_RS: &str = include_str!("../src/daemon.rs");

fn pixels(state: TrayIconState, size: i32) -> Vec<(u8, u8, u8, u8)> {
    let icon = render_icon(state, size);
    icon.data
        .chunks_exact(4)
        .map(|pixel| (pixel[0], pixel[1], pixel[2], pixel[3]))
        .collect()
}

#[test]
fn linux_tray_icon_is_square_argb32_for_every_state() {
    for state in [
        TrayIconState::Idle,
        TrayIconState::Recording,
        TrayIconState::Processing,
        TrayIconState::Error,
    ] {
        for size in [22, 32, 48] {
            let icon = render_icon(state, size);
            assert_eq!(icon.width, size, "{state:?} icon width at {size}");
            assert_eq!(icon.height, size, "{state:?} icon height at {size}");
            assert_eq!(
                icon.data.len(),
                (size * size * 4) as usize,
                "{state:?} icon must carry one ARGB32 quad per pixel at {size}"
            );
        }
    }
}

#[test]
fn linux_tray_icon_draws_a_visible_glyph() {
    for state in [
        TrayIconState::Idle,
        TrayIconState::Recording,
        TrayIconState::Processing,
        TrayIconState::Error,
    ] {
        let pixels = pixels(state, 32);
        let opaque = pixels.iter().filter(|pixel| pixel.0 > 200).count();
        assert!(
            opaque > 40,
            "{state:?} icon must draw a solid glyph, only {opaque} opaque pixels"
        );
        let transparent = pixels.iter().filter(|pixel| pixel.0 == 0).count();
        assert!(
            transparent > pixels.len() / 2,
            "{state:?} icon must stay mostly transparent so the panel shows through"
        );
    }
}

#[test]
fn linux_tray_icon_color_separates_recording_from_processing() {
    assert_ne!(
        icon_color(TrayIconState::Recording),
        icon_color(TrayIconState::Processing),
        "recording and transcribing must not share a tray color"
    );
    assert_ne!(
        icon_color(TrayIconState::Idle),
        icon_color(TrayIconState::Recording),
        "idle must not look like recording"
    );
}

#[test]
fn linux_tray_error_reuses_idle_glyph_with_a_red_badge() {
    // Error must not be mistaken for recording, which is also red, so the glyph
    // keeps the idle color and only gains a badge.
    assert_eq!(
        icon_color(TrayIconState::Error),
        icon_color(TrayIconState::Idle)
    );

    let idle = pixels(TrayIconState::Idle, 32);
    let error = pixels(TrayIconState::Error, 32);
    let badge = error
        .iter()
        .zip(idle.iter())
        .filter(|(error, idle)| error.0 > 200 && *error != *idle)
        .count();
    assert!(
        badge > 10,
        "error icon must add a visible badge over the idle glyph, found {badge} pixels"
    );
}

#[test]
fn linux_tray_labels_report_the_current_state() {
    assert_eq!(toggle_label(TrayIconState::Idle), "Start Recording");
    assert_eq!(toggle_label(TrayIconState::Recording), "Stop Recording");

    assert!(tooltip_description(TrayIconState::Idle, None).contains("Ctrl+Space"));
    assert!(tooltip_description(TrayIconState::Recording, None).contains("Recording"));
    assert_eq!(
        tooltip_description(TrayIconState::Idle, Some("es")),
        "Idle - press Ctrl+Space to dictate (es)"
    );
}

#[test]
fn linux_tray_menu_matches_the_shared_model() {
    let model = TrayMenuModel::for_state(TrayIconState::Idle, false);
    let actions: Vec<TrayMenuAction> = model.items.iter().map(|item| item.action).collect();

    assert_eq!(
        actions,
        vec![
            TrayMenuAction::Settings,
            TrayMenuAction::CopyLastTranscript,
            TrayMenuAction::Cancel,
            TrayMenuAction::TestEndpoint,
            TrayMenuAction::OpenLogs,
            TrayMenuAction::OpenConfig,
            TrayMenuAction::Quit,
        ],
        "the Linux tray renders this model verbatim, so its order is the contract"
    );
}

#[test]
fn linux_daemon_starts_the_tray_only_when_the_setting_is_on() {
    assert!(
        DAEMON_RS.contains("let mut show_tray_icon = config.app.show_tray_icon;"),
        "daemon must read tray visibility from config before the config moves into the controller"
    );
    assert!(
        DAEMON_RS.contains("crate::platform::linux_tray::spawn("),
        "Linux daemon must own the StatusNotifierItem tray"
    );
}

#[test]
fn linux_daemon_removes_the_tray_before_it_exits() {
    let exits = DAEMON_RS.matches("cleanup_socket(&socket_path);").count();
    let shutdowns = DAEMON_RS.matches("shutdown_tray(&tray).await;").count();
    assert!(
        shutdowns >= exits,
        "every daemon exit path must retract the tray icon: {shutdowns} shutdowns for {exits} exits"
    );
}

#[test]
fn test_v60_linux_daemon_retries_the_tray_until_a_host_appears() {
    assert!(
        DAEMON_RS.contains("supervise.tick()"),
        "the daemon must re-attempt tray registration on a timer, because the \
         user unit starts before gnome-shell loads its AppIndicator extension"
    );
    assert!(
        DAEMON_RS.contains("if show_tray_icon && tray.is_none() {"),
        "the retry must fire exactly while the setting is on and no item is owned"
    );
    let quiet_retry =
        DAEMON_RS.contains("last_transcript_available(),\n                        false,");
    assert!(
        quiet_retry,
        "retries must not report the failure again, or a session with no tray \
         host would fill the journal every five seconds"
    );
}

#[test]
fn test_v60_linux_daemon_restarts_a_dead_overlay_shell_within_a_budget() {
    assert!(
        DAEMON_RS.contains("overlay_shell_exited(&mut overlay_shell)"),
        "the daemon must notice that the overlay shell process is gone"
    );
    assert!(
        DAEMON_RS.contains("MAX_OVERLAY_RESTARTS"),
        "a permanently broken overlay shell must stop being respawned"
    );
    assert!(
        DAEMON_RS.contains("overlay_restarts = 0;"),
        "reconciling the overlay setting must restore the restart budget"
    );
}

#[test]
fn test_v61_linux_tray_keeps_showing_the_last_failed_dictation() {
    assert!(
        DAEMON_RS.contains("last_dictation_failed"),
        "a failure returns the controller to Idle at once, so the tray needs the \
         failure carried separately to stay on the error icon"
    );
    assert!(
        DAEMON_RS.contains("TrayIconState::Error"),
        "the carried failure must select the error icon"
    );
    assert_ne!(
        icon_color(TrayIconState::Error),
        icon_color(TrayIconState::Recording),
        "error must not be mistakable for recording"
    );
}
