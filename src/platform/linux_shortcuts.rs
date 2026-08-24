//! GNOME custom keybindings for Linux.
//!
//! Wayland gives the compositor sole ownership of global hotkeys, so nothing in
//! this process can grab `Ctrl+Space` the way `RegisterHotKey` does on Windows.
//! GNOME's own mechanism is a *custom keybinding*: an accelerator plus a shell
//! command it runs. That has one consequence worth remembering — the command
//! runs whether or not the target program is already running, which is how a
//! leftover binding from another dictation app relaunches it on every press.
//!
//! Keeping these entries in sync with `behavior.shortcut` is what makes the
//! shortcut fields in Settings mean anything on Linux.

use anyhow::{Context, Result};
use std::process::Command;

use crate::core::config::Config;
use crate::core::shortcut::ParsedShortcut;

const MEDIA_KEYS_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
const CUSTOM_KEYBINDING_SCHEMA: &str =
    "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";
const CUSTOM_KEYBINDING_PREFIX: &str =
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/";

/// One GNOME custom keybinding this app owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GnomeKeybinding {
    /// Last path segment under `custom-keybindings/`; stable across changes so
    /// a rebind edits the existing entry instead of piling up duplicates.
    pub slug: &'static str,
    pub name: String,
    pub command: String,
    pub accelerator: String,
}

impl GnomeKeybinding {
    pub fn path(&self) -> String {
        format!("{CUSTOM_KEYBINDING_PREFIX}{}/", self.slug)
    }
}

/// Translate a canonical `ParsedShortcut` (`Ctrl+Space`) into the accelerator
/// syntax `gtk_accelerator_parse` expects (`<Control>space`).
pub fn gnome_accelerator(shortcut: &ParsedShortcut) -> Result<String> {
    let canonical = shortcut.as_str();
    let mut parts = canonical.split('+').collect::<Vec<_>>();
    let key = parts.pop().context("shortcut has no key")?;
    let mut accelerator = String::new();
    for modifier in parts {
        accelerator.push_str(match modifier {
            "Ctrl" => "<Control>",
            "Alt" => "<Alt>",
            "Shift" => "<Shift>",
            "Super" => "<Super>",
            other => anyhow::bail!("unsupported shortcut modifier {other:?}"),
        });
    }
    accelerator.push_str(&gnome_key_name(key));
    Ok(accelerator)
}

/// GDK key names differ from this project's canonical spelling for exactly the
/// keys below; everything else is a lowercase character or an `F<n>` name.
fn gnome_key_name(key: &str) -> String {
    match key {
        "Space" => "space".to_string(),
        "Enter" => "Return".to_string(),
        "Backspace" => "BackSpace".to_string(),
        "Escape" | "Tab" | "Left" | "Right" | "Up" | "Down" => key.to_string(),
        other if other.len() > 1 && other.starts_with('F') => other.to_string(),
        other => other.to_ascii_lowercase(),
    }
}

/// The bindings this app owns. Only actions that need an external launcher are
/// registered: `cancel` is `Escape`, which must stay a normal key everywhere
/// else in the desktop, and the debug shortcut is a developer affordance.
pub fn desired_bindings(config: &Config, executable: &str) -> Result<Vec<GnomeKeybinding>> {
    let toggle = ParsedShortcut::parse(&config.behavior.shortcut)
        .context("parsing behavior.shortcut for the GNOME keybinding")?;
    let language = ParsedShortcut::parse(&config.behavior.language_shortcut)
        .context("parsing behavior.language_shortcut for the GNOME keybinding")?;
    Ok(vec![
        GnomeKeybinding {
            slug: "openvoice-toggle",
            name: "OpenVoiceDesktop Toggle".to_string(),
            command: format!("{executable} toggle"),
            accelerator: gnome_accelerator(&toggle)?,
        },
        GnomeKeybinding {
            slug: "openvoice-language",
            name: "OpenVoiceDesktop Language".to_string(),
            command: format!("{executable} cycle-language"),
            accelerator: gnome_accelerator(&language)?,
        },
    ])
}

/// The cancel binding, held only while a dictation is in flight.
///
/// `Escape` cannot live in `desired_bindings`: a permanent custom keybinding on
/// it would swallow the key for the whole session, and Escape has to keep
/// dismissing dialogs everywhere else. GNOME owns every hotkey under Wayland,
/// though, so the only way the key can reach this app at all is a real
/// keybinding - which is why the daemon adds this one when recording starts and
/// removes it the moment the dictation ends.
pub fn cancel_binding(config: &Config, executable: &str) -> Result<GnomeKeybinding> {
    let cancel = ParsedShortcut::parse(&config.behavior.cancel_shortcut)
        .context("parsing behavior.cancel_shortcut for the GNOME keybinding")?;
    Ok(GnomeKeybinding {
        slug: CANCEL_SLUG,
        name: "OpenVoiceDesktop Cancel".to_string(),
        command: format!("{executable} cancel"),
        accelerator: gnome_accelerator(&cancel)?,
    })
}

pub const CANCEL_SLUG: &str = "openvoice-cancel";

/// Merge this app's paths into the existing `custom-keybindings` list without
/// disturbing entries other applications own.
pub fn merged_keybinding_paths(existing: &[String], owned: &[GnomeKeybinding]) -> Vec<String> {
    let mut paths: Vec<String> = existing.to_vec();
    for binding in owned {
        let path = binding.path();
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}

/// Drop one path from the `custom-keybindings` list, leaving every other
/// entry - and their order - alone.
pub fn without_keybinding_path(existing: &[String], path: &str) -> Vec<String> {
    existing
        .iter()
        .filter(|entry| entry.as_str() != path)
        .cloned()
        .collect()
}

/// Parse the GVariant string array `gsettings get` prints.
pub fn parse_gsettings_string_list(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or("");
    inner
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| item.trim_matches('\'').trim_matches('"').to_string())
        .collect()
}

/// Render the GVariant literal `gsettings set` expects for a string array.
pub fn format_gsettings_string_list(values: &[String]) -> String {
    let items = values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{items}]")
}

/// Write the bindings into GNOME. Never fatal: a non-GNOME session simply has
/// no such schema, and the daemon must keep dictating through the CLI.
pub fn apply(config: &Config) -> Result<()> {
    let executable = std::env::current_exe()
        .context("locating OpenVoiceDesktop executable")?
        .to_string_lossy()
        .into_owned();
    let owned = desired_bindings(config, &executable)?;

    let existing =
        parse_gsettings_string_list(&gsettings_get(MEDIA_KEYS_SCHEMA, "custom-keybindings")?);
    for binding in &owned {
        let target = format!("{CUSTOM_KEYBINDING_SCHEMA}:{}", binding.path());
        gsettings_set(&target, "name", &binding.name)?;
        gsettings_set(&target, "command", &binding.command)?;
        gsettings_set(&target, "binding", &binding.accelerator)?;
    }
    let merged = merged_keybinding_paths(&existing, &owned);
    if merged != existing {
        gsettings_set_raw(
            MEDIA_KEYS_SCHEMA,
            "custom-keybindings",
            &format_gsettings_string_list(&merged),
        )?;
    }
    warn_on_conflicts(&existing, &owned);
    Ok(())
}

/// Claim the cancel accelerator for the duration of one dictation. Blocking:
/// each `gsettings` call is a process.
pub fn grab_cancel(config: &Config) -> Result<()> {
    let executable = std::env::current_exe()
        .context("locating OpenVoiceDesktop executable")?
        .to_string_lossy()
        .into_owned();
    let binding = cancel_binding(config, &executable)?;
    let target = format!("{CUSTOM_KEYBINDING_SCHEMA}:{}", binding.path());
    gsettings_set(&target, "name", &binding.name)?;
    gsettings_set(&target, "command", &binding.command)?;
    gsettings_set(&target, "binding", &binding.accelerator)?;
    let existing =
        parse_gsettings_string_list(&gsettings_get(MEDIA_KEYS_SCHEMA, "custom-keybindings")?);
    let merged = merged_keybinding_paths(&existing, std::slice::from_ref(&binding));
    if merged != existing {
        gsettings_set_raw(
            MEDIA_KEYS_SCHEMA,
            "custom-keybindings",
            &format_gsettings_string_list(&merged),
        )?;
    }
    Ok(())
}

/// Give the cancel accelerator back to the desktop. Also called once at daemon
/// start, because a daemon killed mid-recording would otherwise leave the key
/// grabbed for the rest of the session.
pub fn release_cancel() -> Result<()> {
    let path = format!("{CUSTOM_KEYBINDING_PREFIX}{CANCEL_SLUG}/");
    let existing =
        parse_gsettings_string_list(&gsettings_get(MEDIA_KEYS_SCHEMA, "custom-keybindings")?);
    let remaining = without_keybinding_path(&existing, &path);
    if remaining != existing {
        gsettings_set_raw(
            MEDIA_KEYS_SCHEMA,
            "custom-keybindings",
            &format_gsettings_string_list(&remaining),
        )?;
    }
    Ok(())
}

/// Desktop components outside the custom-keybinding list also claim
/// accelerators, and they use their own spelling. IBus writes `Control+space`
/// where GNOME writes `<Control>space`.
pub fn ibus_hotkey_form(accelerator: &str) -> String {
    accelerator
        .replace("<Control>", "Control+")
        .replace("<Alt>", "Alt+")
        .replace("<Shift>", "Shift+")
        .replace("<Super>", "Super+")
}

/// Accelerators claimed by desktop components that are not custom keybindings.
/// IBus's `trigger` is the one that matters in practice: it ships bound to
/// `Control+space`, so a dictation shortcut on that combination is processed by
/// the input method on the very same keypress.
fn foreign_hotkey_claims(accelerator: &str) -> Vec<String> {
    let mut claims = Vec::new();
    let ibus_form = ibus_hotkey_form(accelerator);
    for (schema, key, label) in [
        (
            "org.freedesktop.ibus.general.hotkey",
            "trigger",
            "the IBus input-method trigger",
        ),
        (
            "org.freedesktop.ibus.general.hotkey",
            "triggers",
            "the IBus input-method trigger",
        ),
    ] {
        let Ok(value) = gsettings_get(schema, key) else {
            continue;
        };
        let entries = parse_gsettings_string_list(&value);
        if entries
            .iter()
            .any(|entry| entry == &ibus_form || entry == accelerator)
        {
            claims.push(format!("{label} (`gsettings get {schema} {key}`)"));
        }
    }
    for (schema, key, label) in [(
        "org.gnome.desktop.wm.keybindings",
        "switch-input-source",
        "the GNOME input-source switch",
    )] {
        let Ok(value) = gsettings_get(schema, key) else {
            continue;
        };
        if parse_gsettings_string_list(&value)
            .iter()
            .any(|entry| entry == accelerator)
        {
            claims.push(format!("{label} (`gsettings get {schema} {key}`)"));
        }
    }
    claims
}

/// GNOME silently lets two custom keybindings claim the same accelerator, and
/// the loser just stops working. Name the other entry instead of leaving the
/// user to guess why the shortcut runs someone else's program.
fn warn_on_conflicts(existing: &[String], owned: &[GnomeKeybinding]) {
    for binding in owned {
        for claim in foreign_hotkey_claims(&binding.accelerator) {
            eprintln!(
                "openvoice-desktop shortcut conflict: {} is also claimed by {claim}; both react to the same keypress",
                binding.accelerator
            );
        }
    }
    warn_on_custom_keybinding_conflicts(existing, owned);
}

fn warn_on_custom_keybinding_conflicts(existing: &[String], owned: &[GnomeKeybinding]) {
    let owned_paths: Vec<String> = owned.iter().map(GnomeKeybinding::path).collect();
    for path in existing {
        if owned_paths.contains(path) {
            continue;
        }
        let target = format!("{CUSTOM_KEYBINDING_SCHEMA}:{path}");
        let Ok(binding) = gsettings_get(&target, "binding") else {
            continue;
        };
        let binding = binding.trim().trim_matches('\'').to_string();
        if let Some(conflict) = owned.iter().find(|owned| owned.accelerator == binding) {
            let command = gsettings_get(&target, "command").unwrap_or_default();
            eprintln!(
                "openvoice-desktop shortcut conflict: {} is also bound to {} by {path}; \
                 GNOME may run that instead of {}",
                conflict.accelerator,
                command.trim(),
                conflict.command
            );
        }
    }
}

fn gsettings_get(schema: &str, key: &str) -> Result<String> {
    let output = Command::new("gsettings")
        .args(["get", schema, key])
        .output()
        .with_context(|| format!("running gsettings get {schema} {key}"))?;
    anyhow::ensure!(
        output.status.success(),
        "gsettings get {schema} {key} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn gsettings_set(schema: &str, key: &str, value: &str) -> Result<()> {
    gsettings_set_raw(schema, key, value)
}

fn gsettings_set_raw(schema: &str, key: &str, value: &str) -> Result<()> {
    let status = Command::new("gsettings")
        .args(["set", schema, key, value])
        .status()
        .with_context(|| format!("running gsettings set {schema} {key}"))?;
    anyhow::ensure!(status.success(), "gsettings set {schema} {key} failed");
    Ok(())
}
