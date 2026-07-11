use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use openvoice_desktop::core::config::Config;
use openvoice_desktop::core::settings_inventory::{
    ControlState, InventoryKind, SETTINGS_INVENTORY,
};
use openvoice_desktop::core::settings_service::{SaveSettingsRequest, SettingsService};

const SETTINGS_JS: &str = include_str!("../ui/settings.js");
const SHELL_RS: &str = include_str!("../src/settings_shell.rs");
const WINDOWS_RS: &str = include_str!("../src/platform/windows.rs");

#[test]
fn working_settings_end_to_end_matrix() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut config = Config::default();
    config.history.recordings_dir = Some(dir.path().join("recordings"));
    config.debug.log_directory = Some(dir.path().join("logs"));
    config.save_to(&path).unwrap();
    let service = SettingsService::new(&path);
    let document = service.get().unwrap();
    let rendered = flatten_paths(serde_json::to_value(&document.config).unwrap());
    let result_fields = document
        .fields
        .iter()
        .map(|field| field.field.as_str())
        .collect::<BTreeSet<_>>();

    for entry in SETTINGS_INVENTORY {
        if entry.kind != InventoryKind::Field
            || !matches!(entry.state, ControlState::Working { .. })
        {
            continue;
        }
        assert!(rendered.contains(entry.id), "not rendered: {}", entry.id);
        assert!(
            result_fields.contains(entry.id),
            "no save result: {}",
            entry.id
        );
    }

    let calls = AtomicUsize::new(0);
    let mut changed = document.config;
    changed.app.show_overlay = !changed.app.show_overlay;
    changed.output.paste_delay_ms += 1;
    changed.text.vad_enabled = !changed.text.vad_enabled;
    changed.history.limit += 1;
    changed.debug.live_log_viewer = !changed.debug.live_log_viewer;
    let result = service
        .save_with(SaveSettingsRequest { config: changed }, |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(result.fields.iter().all(|field| !field.field.is_empty()));
    assert!(Config::load_from(&path).is_ok());
}

#[test]
fn working_actions_end_to_end_matrix() {
    let combined = format!("{SETTINGS_JS}\n{SHELL_RS}\n{WINDOWS_RS}");
    for entry in SETTINGS_INVENTORY {
        if entry.kind != InventoryKind::Action
            || !matches!(entry.state, ControlState::Working { .. })
        {
            continue;
        }
        let needles: &[&str] = match entry.id {
            "test-endpoint" | "tray.test-endpoint" => &["test_endpoint"],
            "open-recordings-folder" => &["open_recordings_folder"],
            "delete-transcript" => &["delete_transcript"],
            "retry-transcript" => &["retry_transcript"],
            "save-transcript" => &["save_transcript"],
            "open-log-directory" | "tray.open-logs" => &["open_log_directory", "open_logs_folder"],
            "open-app-data-directory" => &["open_app_data_directory"],
            "tray.settings" => &["open_settings_panel"],
            "tray.copy-last-transcript" => &["CopyLastTranscript"],
            "tray.cancel" => &["TrayCommand::Cancel"],
            "tray.open-config-folder" => &["open_config_folder"],
            "tray.quit" => &["TrayCommand::Quit"],
            other => panic!("missing action matrix row: {other}"),
        };
        assert!(
            needles.iter().any(|needle| combined.contains(needle)),
            "unwired action: {}",
            entry.id
        );
    }
    assert!(!combined.contains("Action available in settings UI."));
}

#[test]
fn readonly_and_platform_options_have_render_contracts() {
    for value in [
        "about.version",
        "about.license",
        "about.repository_url",
        "about.config_path",
        "about.app_data_path",
        "about.logs_path",
        "about.acknowledgements",
    ] {
        assert!(SETTINGS_JS.contains(value), "{value}");
    }
    for options in [
        "audio.selected_microphone",
        "audio.selected_output_device",
        "output.paste_method",
        "output.typing_tool",
        "history.retention",
        "ui.language",
        "ui.theme",
    ] {
        assert!(SETTINGS_JS.contains(options), "{options}");
    }
}

fn flatten_paths(value: serde_json::Value) -> BTreeSet<String> {
    fn visit(prefix: &str, value: &serde_json::Value, output: &mut BTreeSet<String>) {
        if let serde_json::Value::Object(map) = value {
            for (key, child) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                if child.is_object() {
                    visit(&path, child, output);
                } else {
                    output.insert(path);
                }
            }
        }
    }
    let mut output = BTreeSet::new();
    visit("", &value, &mut output);
    output
}
