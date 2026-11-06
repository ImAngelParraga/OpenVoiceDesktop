use anyhow::{bail, Result};
use openvoice_desktop::core::config::Config;
use openvoice_desktop::core::shortcut::{
    CoordinatorAction, CoordinatorInput, ParsedShortcut, ShortcutAction, ShortcutCoordinator,
    ShortcutEvent, ShortcutRegistrar, ShortcutRuntime,
};
use openvoice_desktop::core::state::RecordingState;
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[test]
fn coordinator_debounces_toggle_sources_and_keeps_cancel_immediate() {
    let mut coordinator =
        ShortcutCoordinator::new(Duration::from_millis(120), Duration::from_millis(180));
    let now = Instant::now();

    assert_eq!(
        coordinator.action(CoordinatorInput::ToggleShortcut, RecordingState::Idle, now),
        CoordinatorAction::Toggle
    );
    assert_eq!(
        coordinator.action(
            CoordinatorInput::ToggleCli,
            RecordingState::Idle,
            now + Duration::from_millis(50)
        ),
        CoordinatorAction::IgnoreDebounce
    );
    assert_eq!(
        coordinator.action(
            CoordinatorInput::CancelTray,
            RecordingState::Transcribing,
            now + Duration::from_millis(60)
        ),
        CoordinatorAction::Cancel
    );
}

#[test]
fn coordinator_maps_push_to_talk_release_to_stop_only_while_recording() {
    let mut coordinator = ShortcutCoordinator::default();
    let now = Instant::now();

    assert_eq!(
        coordinator.action(
            CoordinatorInput::PushToTalkReleased,
            RecordingState::Idle,
            now
        ),
        CoordinatorAction::IgnoreReleaseGrace
    );
    assert_eq!(
        coordinator.action(
            CoordinatorInput::PushToTalkReleased,
            RecordingState::Recording,
            now
        ),
        CoordinatorAction::Toggle
    );
    assert_eq!(coordinator.release_grace(), Duration::from_millis(180));
}

#[test]
fn shortcuts_default_to_ctrl_space_and_map_press_release_events() {
    let config = Config::default();
    let bindings =
        openvoice_desktop::core::shortcut::ShortcutBindings::from_config(&config).unwrap();
    let primary = ParsedShortcut::parse("control + space").unwrap();
    assert_eq!(primary.as_str(), "Ctrl+Space");
    assert_eq!(bindings.bindings()[0].action, ShortcutAction::Toggle);
    assert_eq!(
        bindings.input_for(&primary, ShortcutEvent::Pressed),
        Some(CoordinatorInput::ToggleShortcut)
    );

    let mut push_to_talk = config;
    push_to_talk.behavior.push_to_talk = true;
    let bindings =
        openvoice_desktop::core::shortcut::ShortcutBindings::from_config(&push_to_talk).unwrap();
    assert_eq!(
        bindings.input_for(&primary, ShortcutEvent::Pressed),
        Some(CoordinatorInput::PushToTalkPressed)
    );
    assert_eq!(
        bindings.input_for(&primary, ShortcutEvent::Released),
        Some(CoordinatorInput::PushToTalkReleased)
    );
    let language = ParsedShortcut::parse("Ctrl+Alt+L").unwrap();
    assert_eq!(
        bindings.input_for(&language, ShortcutEvent::Pressed),
        Some(CoordinatorInput::LanguageShortcut)
    );
}

#[test]
fn shortcut_runtime_rolls_back_previous_registration_on_conflict() {
    let mut runtime = ShortcutRuntime::new(FakeRegistrar::default());
    let original = Config::default();
    runtime.register_config(&original).unwrap();

    let mut conflicting = original.clone();
    conflicting.behavior.cancel_shortcut = conflicting.behavior.shortcut.clone();
    assert!(runtime.register_config(&conflicting).is_err());
    assert_eq!(
        runtime.bindings().unwrap().bindings()[0].shortcut.as_str(),
        "Ctrl+Space"
    );
    assert_eq!(runtime.into_registrar().registered.len(), 5);
}

#[derive(Default, Debug)]
struct FakeRegistrar {
    registered: HashSet<String>,
}

impl ShortcutRegistrar for FakeRegistrar {
    fn register(&mut self, shortcut: &ParsedShortcut, _action: ShortcutAction) -> Result<()> {
        if !self.registered.insert(shortcut.to_string()) {
            bail!("conflict");
        }
        Ok(())
    }

    fn unregister(&mut self, shortcut: &ParsedShortcut) -> Result<()> {
        self.registered.remove(shortcut.as_str());
        Ok(())
    }
}

#[test]
fn pause_shortcut_maps_press_only_and_push_to_talk_release_finishes_paused_capture() {
    let mut config = Config::default();
    config.behavior.pause_shortcut = "Ctrl+Shift+P".into();
    let bindings =
        openvoice_desktop::core::shortcut::ShortcutBindings::from_config(&config).unwrap();
    let pause = ParsedShortcut::parse("Ctrl+Shift+P").unwrap();
    assert_eq!(
        bindings.input_for(&pause, ShortcutEvent::Pressed),
        Some(CoordinatorInput::PauseShortcut)
    );
    assert_eq!(bindings.input_for(&pause, ShortcutEvent::Released), None);
    let mut coordinator = ShortcutCoordinator::default();
    assert_eq!(
        coordinator.action(
            CoordinatorInput::PauseShortcut,
            RecordingState::Recording,
            Instant::now()
        ),
        CoordinatorAction::PauseResume
    );
    assert_eq!(
        coordinator.action(
            CoordinatorInput::PushToTalkReleased,
            RecordingState::Paused,
            Instant::now()
        ),
        CoordinatorAction::Toggle
    );
}
