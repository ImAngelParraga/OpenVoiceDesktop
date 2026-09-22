use anyhow::{anyhow, bail, Result};
use std::collections::HashSet;
use std::fmt;
use std::time::{Duration, Instant};

use crate::core::config::{BehaviorConfig, Config};
use crate::core::state::RecordingState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinatorInput {
    ToggleShortcut,
    ToggleCli,
    ToggleTray,
    PushToTalkPressed,
    PushToTalkReleased,
    CancelShortcut,
    CancelCli,
    CancelTray,
    DebugShortcut,
    LanguageShortcut,
    Signal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinatorAction {
    Toggle,
    Cancel,
    Debug,
    CycleLanguage,
    IgnoreDebounce,
    IgnoreReleaseGrace,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutAction {
    Toggle,
    PushToTalk,
    Cancel,
    Debug,
    CycleLanguage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutEvent {
    Pressed,
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParsedShortcut {
    canonical: String,
}

impl ParsedShortcut {
    pub fn parse(value: &str) -> Result<Self> {
        let mut modifiers = Vec::new();
        let mut key = None;

        for (index, part) in value.split('+').enumerate() {
            let part = part.trim();
            if part.is_empty() {
                bail!("shortcut contains an empty key part");
            }
            let normalized = part.to_ascii_lowercase();
            if is_modifier(&normalized) {
                if index == value.split('+').count() - 1 {
                    bail!("shortcut must include a non-modifier key");
                }
                let modifier = canonical_modifier(&normalized).unwrap();
                if modifiers.contains(&modifier) {
                    bail!("shortcut repeats modifier {modifier}");
                }
                modifiers.push(modifier);
            } else if key.replace(part.to_string()).is_some() {
                bail!("shortcut must contain exactly one key");
            }
        }

        let key = key.ok_or_else(|| anyhow!("shortcut must include a key"))?;
        validate_key(&key)?;
        modifiers.sort_unstable_by_key(|modifier| modifier_order(modifier));

        let mut canonical = modifiers.join("+");
        if !canonical.is_empty() {
            canonical.push('+');
        }
        canonical.push_str(&canonical_key(&key));
        Ok(Self { canonical })
    }

    pub fn as_str(&self) -> &str {
        &self.canonical
    }
}

impl fmt::Display for ParsedShortcut {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutBinding {
    pub shortcut: ParsedShortcut,
    pub action: ShortcutAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutBindings {
    bindings: Vec<ShortcutBinding>,
}

impl ShortcutBindings {
    pub fn from_config(config: &Config) -> Result<Self> {
        Self::from_behavior(&config.behavior, &config.debug.debug_shortcut)
    }

    pub fn from_behavior(behavior: &BehaviorConfig, debug_shortcut: &str) -> Result<Self> {
        let primary_action = if behavior.push_to_talk {
            ShortcutAction::PushToTalk
        } else {
            ShortcutAction::Toggle
        };
        let bindings = vec![
            ShortcutBinding {
                shortcut: ParsedShortcut::parse(&behavior.shortcut)?,
                action: primary_action,
            },
            ShortcutBinding {
                shortcut: ParsedShortcut::parse(&behavior.cancel_shortcut)?,
                action: ShortcutAction::Cancel,
            },
            ShortcutBinding {
                shortcut: ParsedShortcut::parse(&behavior.language_shortcut)?,
                action: ShortcutAction::CycleLanguage,
            },
        ];
        #[cfg(not(target_os = "windows"))]
        let mut bindings = bindings;
        #[cfg(not(target_os = "windows"))]
        bindings.push(ShortcutBinding {
            shortcut: ParsedShortcut::parse(debug_shortcut)?,
            action: ShortcutAction::Debug,
        });
        #[cfg(target_os = "windows")]
        let _ = debug_shortcut;
        let mut seen = HashSet::new();
        for binding in &bindings {
            if !seen.insert(&binding.shortcut) {
                bail!("shortcut conflict: {}", binding.shortcut);
            }
        }
        Ok(Self { bindings })
    }

    pub fn bindings(&self) -> &[ShortcutBinding] {
        &self.bindings
    }

    pub fn input_for(
        &self,
        shortcut: &ParsedShortcut,
        event: ShortcutEvent,
    ) -> Option<CoordinatorInput> {
        self.bindings
            .iter()
            .find(|binding| &binding.shortcut == shortcut)
            .and_then(|binding| match (binding.action, event) {
                (ShortcutAction::Toggle, ShortcutEvent::Pressed) => {
                    Some(CoordinatorInput::ToggleShortcut)
                }
                (ShortcutAction::PushToTalk, ShortcutEvent::Pressed) => {
                    Some(CoordinatorInput::PushToTalkPressed)
                }
                (ShortcutAction::PushToTalk, ShortcutEvent::Released) => {
                    Some(CoordinatorInput::PushToTalkReleased)
                }
                (ShortcutAction::Cancel, ShortcutEvent::Pressed) => {
                    Some(CoordinatorInput::CancelShortcut)
                }
                (ShortcutAction::Debug, ShortcutEvent::Pressed) => {
                    Some(CoordinatorInput::DebugShortcut)
                }
                (ShortcutAction::CycleLanguage, ShortcutEvent::Pressed) => {
                    Some(CoordinatorInput::LanguageShortcut)
                }
                _ => None,
            })
    }
}

pub trait ShortcutRegistrar {
    fn register(&mut self, shortcut: &ParsedShortcut, action: ShortcutAction) -> Result<()>;
    fn unregister(&mut self, shortcut: &ParsedShortcut) -> Result<()>;
}

#[derive(Debug)]
pub struct ShortcutRuntime<R> {
    registrar: R,
    bindings: Option<ShortcutBindings>,
}

impl<R: ShortcutRegistrar> ShortcutRuntime<R> {
    pub fn new(registrar: R) -> Self {
        Self {
            registrar,
            bindings: None,
        }
    }

    pub fn register_config(&mut self, config: &Config) -> Result<()> {
        self.replace(ShortcutBindings::from_config(config)?)
    }

    pub fn replace(&mut self, candidate: ShortcutBindings) -> Result<()> {
        let previous = self.bindings.clone();
        if let Some(previous) = &previous {
            for binding in previous.bindings.iter().rev() {
                self.registrar.unregister(&binding.shortcut)?;
            }
        }

        let mut registered = Vec::new();
        for binding in candidate.bindings() {
            if let Err(error) = self.registrar.register(&binding.shortcut, binding.action) {
                for shortcut in registered.iter().rev() {
                    let _ = self.registrar.unregister(shortcut);
                }
                if let Some(previous) = &previous {
                    for old in previous.bindings() {
                        let _ = self.registrar.register(&old.shortcut, old.action);
                    }
                }
                return Err(anyhow!("registering {} failed: {error}", binding.shortcut));
            }
            registered.push(binding.shortcut.clone());
        }
        self.bindings = Some(candidate);
        Ok(())
    }

    pub fn bindings(&self) -> Option<&ShortcutBindings> {
        self.bindings.as_ref()
    }

    pub fn into_registrar(self) -> R {
        self.registrar
    }
}

fn is_modifier(value: &str) -> bool {
    canonical_modifier(value).is_some()
}

fn canonical_modifier(value: &str) -> Option<&'static str> {
    match value {
        "ctrl" | "control" => Some("Ctrl"),
        "alt" | "option" => Some("Alt"),
        "shift" => Some("Shift"),
        "super" | "meta" | "win" | "windows" | "cmd" | "command" => Some("Super"),
        _ => None,
    }
}

fn modifier_order(value: &str) -> usize {
    match value {
        "Ctrl" => 0,
        "Alt" => 1,
        "Shift" => 2,
        "Super" => 3,
        _ => usize::MAX,
    }
}

fn canonical_key(value: &str) -> String {
    match value.to_ascii_lowercase().as_str() {
        "space" => "Space".to_string(),
        "escape" | "esc" => "Escape".to_string(),
        "enter" | "return" => "Enter".to_string(),
        "tab" => "Tab".to_string(),
        "backspace" => "Backspace".to_string(),
        "left" | "right" | "up" | "down" => {
            let mut chars = value.chars();
            chars.next().unwrap().to_ascii_uppercase().to_string() + chars.as_str()
        }
        _ => value.to_ascii_uppercase(),
    }
}

fn validate_key(value: &str) -> Result<()> {
    let lower = value.to_ascii_lowercase();
    let valid_named = matches!(
        lower.as_str(),
        "space"
            | "escape"
            | "esc"
            | "enter"
            | "return"
            | "tab"
            | "backspace"
            | "left"
            | "right"
            | "up"
            | "down"
    );
    let valid_function = lower.len() >= 2
        && lower.starts_with('f')
        && lower[1..]
            .parse::<u8>()
            .is_ok_and(|number| (1..=24).contains(&number));
    let valid_simple = lower.len() == 1 && lower.chars().next().unwrap().is_ascii_alphanumeric();
    if valid_named || valid_function || valid_simple {
        Ok(())
    } else {
        bail!("unsupported shortcut key {value:?}")
    }
}

#[derive(Debug, Clone)]
pub struct ShortcutCoordinator {
    debounce: Duration,
    release_grace: Duration,
    last_toggle_at: Option<Instant>,
}

impl ShortcutCoordinator {
    pub fn new(debounce: Duration, release_grace: Duration) -> Self {
        Self {
            debounce,
            release_grace,
            last_toggle_at: None,
        }
    }

    pub fn action(
        &mut self,
        input: CoordinatorInput,
        state: RecordingState,
        now: Instant,
    ) -> CoordinatorAction {
        match input {
            CoordinatorInput::CancelShortcut
            | CoordinatorInput::CancelCli
            | CoordinatorInput::CancelTray => CoordinatorAction::Cancel,
            CoordinatorInput::DebugShortcut => CoordinatorAction::Debug,
            CoordinatorInput::LanguageShortcut => CoordinatorAction::CycleLanguage,
            CoordinatorInput::Signal => CoordinatorAction::Quit,
            CoordinatorInput::PushToTalkReleased => {
                if state == RecordingState::Recording {
                    self.record_toggle(now)
                } else {
                    CoordinatorAction::IgnoreReleaseGrace
                }
            }
            CoordinatorInput::ToggleShortcut
            | CoordinatorInput::ToggleCli
            | CoordinatorInput::ToggleTray
            | CoordinatorInput::PushToTalkPressed => self.record_toggle(now),
        }
    }

    fn record_toggle(&mut self, now: Instant) -> CoordinatorAction {
        if self
            .last_toggle_at
            .is_some_and(|last| now.duration_since(last) < self.debounce)
        {
            return CoordinatorAction::IgnoreDebounce;
        }
        self.last_toggle_at = Some(now);
        CoordinatorAction::Toggle
    }

    pub fn release_grace(&self) -> Duration {
        self.release_grace
    }
}

impl Default for ShortcutCoordinator {
    fn default() -> Self {
        Self::new(Duration::from_millis(120), Duration::from_millis(180))
    }
}
