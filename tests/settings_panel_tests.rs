use openvoice_desktop::core::config::{Config, PasteMethod, Theme};
use openvoice_desktop::core::settings_panel::{
    apply_settings_form, parse_form_urlencoded, render_settings_panel,
};

#[test]
fn settings_panel_renders_visible_sections_and_save_form() {
    let html = render_settings_panel(&Config::default(), false);

    assert!(html.contains("<form method=\"post\">"));
    assert!(html.contains("OpenVoiceDesktop Settings"));
    assert!(html.contains("General"));
    assert!(html.contains("ASR Endpoint"));
    assert!(html.contains("Advanced"));
    assert!(html.contains("History"));
    assert!(html.contains("Debug"));
    assert!(html.contains("About"));
    assert!(html.contains("name=\"asr.base_url\""));
    assert!(html.contains("Save Settings"));
}

#[test]
fn settings_panel_form_updates_persisted_config_fields() {
    let mut config = Config::default();
    let fields = parse_form_urlencoded(
        "asr.base_url=https%3A%2F%2Fasr.example.test&asr.model=large-v3&asr.timeout_seconds=45&behavior.push_to_talk=true&output.paste_method=direct-type&ui.theme=dark",
    );

    apply_settings_form(&mut config, &fields).unwrap();

    assert_eq!(config.asr.base_url, "https://asr.example.test");
    assert_eq!(config.asr.model, "large-v3");
    assert_eq!(config.asr.timeout_seconds, 45);
    assert!(config.behavior.push_to_talk);
    assert_eq!(config.output.paste_method, PasteMethod::DirectType);
    assert_eq!(config.ui.theme, Theme::Dark);
}
