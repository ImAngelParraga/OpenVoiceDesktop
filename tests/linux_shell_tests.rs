#![cfg(target_os = "linux")]

const MAIN_RS: &str = include_str!("../src/main.rs");
const DAEMON_RS: &str = include_str!("../src/daemon.rs");
const SETTINGS_SHELL_RS: &str = include_str!("../src/settings_shell.rs");
const SETTINGS_JS: &str = include_str!("../ui/settings.js");

#[test]
fn settings_never_runs_inside_a_tokio_runtime() {
    // `tauri_plugin_single_instance` builds a *blocking* zbus connection on
    // Linux; doing that under `#[tokio::main]` panicked with "Cannot start a
    // runtime from within a runtime" and took the whole Settings window with it.
    assert!(
        !MAIN_RS.contains("#[tokio::main]"),
        "main must build its runtime explicitly so `settings` can run outside it"
    );
    assert!(
        MAIN_RS.contains("if matches!(command, Some(Command::Settings)) {")
            && MAIN_RS.contains("return settings_shell::run(cli.start_hidden);"),
        "settings must be dispatched before the tokio runtime is created"
    );
    let runtime_build = MAIN_RS
        .find("Builder::new_multi_thread()")
        .expect("main builds a multi-thread runtime");
    let settings_dispatch = MAIN_RS
        .find("return settings_shell::run(cli.start_hidden);")
        .expect("main dispatches settings");
    assert!(
        settings_dispatch < runtime_build,
        "the settings early return must come before the runtime is built"
    );
}

#[test]
fn the_shell_asks_for_xwayland_so_the_overlay_can_be_positioned() {
    // Mutter ignores client-side placement on Wayland, so a native Wayland
    // overlay cannot sit centered above other windows.
    assert!(
        SETTINGS_SHELL_RS.contains("fn prefer_x11_backend_for_overlay()"),
        "the shell must be able to select the X11 backend"
    );
    assert!(
        SETTINGS_SHELL_RS.contains("std::env::set_var(\"GDK_BACKEND\", \"x11\")"),
        "the X11 preference must be applied through GDK_BACKEND"
    );
    assert!(
        SETTINGS_SHELL_RS.contains("if std::env::var_os(\"GDK_BACKEND\").is_some()"),
        "an explicitly chosen backend must win over this default"
    );
    assert!(
        SETTINGS_SHELL_RS.contains("fn import_graphical_session_environment()")
            && SETTINGS_SHELL_RS.contains("[\"--user\", \"show-environment\"]"),
        "a login-race retry must refresh graphical variables from the user manager"
    );
    let import = SETTINGS_SHELL_RS
        .find("import_graphical_session_environment();")
        .expect("shell imports the current graphical environment");
    let prefer = SETTINGS_SHELL_RS
        .find("prefer_x11_backend_for_overlay();")
        .expect("shell selects XWayland");
    assert!(
        import < prefer,
        "the graphical environment must be refreshed before choosing a GTK backend"
    );
}

#[test]
fn the_shell_imports_the_x_authority_cookie_and_drops_a_stale_one() {
    // XWayland refuses a client with no cookie ("Authorization required, but no
    // authorization protocol specified") and GTK init then panics, so the
    // display variables alone are not enough to reach the X server.
    assert!(
        SETTINGS_SHELL_RS.contains("\"XAUTHORITY\","),
        "XAUTHORITY must be imported alongside the display variables"
    );
    assert!(
        SETTINGS_SHELL_RS.contains("fn stale_xauthority()")
            && SETTINGS_SHELL_RS.contains("std::env::remove_var(\"XAUTHORITY\")"),
        "a cookie path left over from a previous login must be dropped before importing"
    );
    let drop_stale = SETTINGS_SHELL_RS
        .find("std::env::remove_var(\"XAUTHORITY\")")
        .expect("shell drops a stale cookie");
    let show_environment = SETTINGS_SHELL_RS
        .find("[\"--user\", \"show-environment\"]")
        .expect("shell reads the user manager environment");
    assert!(
        drop_stale < show_environment,
        "the stale cookie must be dropped before the import, or it would be kept"
    );
}

#[test]
fn the_shell_keeps_wayland_when_x11_has_no_cookie() {
    // Forcing GDK_BACKEND=x11 without a usable cookie cannot open the display
    // at all; an overlay Mutter places badly still beats a shell that panics.
    assert!(
        SETTINGS_SHELL_RS.contains("fn x11_authority_is_reachable()"),
        "the shell must check that an X11 connection can authenticate"
    );
    assert!(
        SETTINGS_SHELL_RS.contains(".join(\".Xauthority\")"),
        "a plain X11 session authenticates through ~/.Xauthority"
    );
    let check = SETTINGS_SHELL_RS
        .find("if !x11_authority_is_reachable() {")
        .expect("the backend choice consults the cookie");
    let set_backend = SETTINGS_SHELL_RS
        .find("std::env::set_var(\"GDK_BACKEND\", \"x11\")")
        .expect("the shell selects XWayland");
    assert!(
        check < set_backend,
        "the cookie must be checked before GDK_BACKEND is forced to x11"
    );
}

#[test]
fn the_daemon_owns_the_overlay_shell_process() {
    assert!(
        DAEMON_RS.contains("fn spawn_overlay_shell()"),
        "nothing else starts the shell on Linux, so the daemon must"
    );
    assert!(
        DAEMON_RS.contains("crate::settings_shell::START_HIDDEN_FLAG"),
        "the overlay shell must start with the Settings window hidden"
    );
    assert!(
        DAEMON_RS.contains(".kill_on_drop(true)"),
        "a restarted daemon must not leave an orphaned webview behind"
    );

    let exits = DAEMON_RS.matches("cleanup_socket(&socket_path);").count();
    let stops = DAEMON_RS
        .matches("stop_overlay_shell(&mut overlay_shell).await;")
        .count();
    assert!(
        stops >= exits,
        "every daemon exit path must stop the overlay shell: {stops} stops for {exits} exits"
    );
}

#[test]
fn a_hidden_relaunch_does_not_pop_the_settings_window() {
    assert!(
        SETTINGS_SHELL_RS.contains("if args.iter().any(|argument| argument == START_HIDDEN_FLAG)"),
        "single-instance activation must ignore the daemon's own hidden relaunch"
    );
}

#[test]
fn the_daemon_registers_the_configured_shortcut_with_gnome() {
    assert!(
        DAEMON_RS.contains("crate::core::lifecycle::apply_shortcuts(&shortcut_config)"),
        "daemon startup must reconcile the GNOME keybinding with the saved shortcut"
    );
    assert!(
        DAEMON_RS.contains("crate::core::lifecycle::apply_shortcuts(&loaded.config)"),
        "a config reload must re-register the shortcut, or a Save would not take effect"
    );
}

#[test]
fn the_endpoint_check_is_not_presented_as_a_transcription() {
    // It uploads a silent clip, so the model's reply is noise and the latency
    // is the real signal. A bare number reads like a failed dictation.
    for (surface, text) in [("tray", DAEMON_RS), ("settings UI", SETTINGS_JS)] {
        assert!(
            text.contains("Endpoint check"),
            "{surface} must label the endpoint check as a check"
        );
        assert!(
            text.contains("microphone"),
            "{surface} must say the check does not use the microphone"
        );
    }
    assert!(
        !SETTINGS_JS.contains("`HTTP ${result.http_status}; ${result.latency_ms} ms;"),
        "the bare HTTP/latency/body string is what read as a transcript"
    );
}
