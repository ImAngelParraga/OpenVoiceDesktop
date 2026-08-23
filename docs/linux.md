# Ubuntu / Debian (GNOME Wayland)

OpenVoiceDesktop on Linux runs as a resident user daemon (`openvoice-desktop daemon`)
plus a Tauri shell (`openvoice-desktop settings`) that owns the settings window and the
recording overlay. The daemon is managed by systemd, shows a tray indicator while it
runs, and starts the shell with its window hidden so the overlay is available; the
desktop entry brings that same window up.

## Install

Download `OpenVoiceDesktop_<version>_amd64.deb` from the repository's **Releases**
page, then install it:

```bash
sudo apt install ./OpenVoiceDesktop_<version>_amd64.deb
```

Double-clicking the file in Files/GNOME Software works too. `apt` pulls the runtime
dependencies (system webview, ALSA, `wl-clipboard`) and the recommended paste and
secret helpers (`wtype`, `libsecret-tools`, `libnotify-bin`).

The package installs:

| Path | Purpose |
| --- | --- |
| `/usr/bin/openvoice-desktop` | binary |
| `/usr/lib/systemd/user/openvoice-desktop.service` | systemd **user** unit |
| `/usr/share/applications/OpenVoiceDesktop.desktop` | launcher that opens Settings |

## Tray indicator

While the daemon runs it publishes a microphone indicator over the
StatusNotifierItem protocol. The icon is the answer to "is it running?": white
when idle, red while recording, amber while transcribing, and white with a red
badge after a failed dictation. Left click toggles recording; right click opens
Settings, Copy Last Transcript, Cancel, Test Endpoint, Open Logs, Open Config,
and Quit.

## Recording overlay

While a dictation is in flight a small panel appears near the top of the screen with
the state (`Recording` / `Transcribing`), a level meter, the transcription language,
and a cancel button. Clicking the language badge cycles it, same as `Ctrl+Alt+L`.

The panel belongs to the shell process the daemon starts, so it only appears while
`openvoice-desktop daemon` is running with `app.show_overlay = true`. On a Wayland
session the shell asks for the X11 backend, because Mutter ignores client window
placement and a native Wayland panel could not stay centered and above other windows.
Set `GDK_BACKEND` yourself to override that choice.

GNOME needs the AppIndicator extension for any tray at all — on Ubuntu it ships
enabled as `ubuntu-appindicators@ubuntu.com`:

```bash
gnome-extensions list --enabled | grep appindicator
```

If no StatusNotifier host is running, the daemon logs
`openvoice-desktop tray unavailable: ...` and keeps dictating without an icon.
Turn the indicator off with `app.show_tray_icon = false` in the config or
**Settings → Advanced → Show tray icon**; the change applies without a restart.

The unit is installed but not enabled. Turning on **autostart** in Settings runs
`systemctl --user enable openvoice-desktop.service`, which resolves against the
packaged unit, so no manual copy into `~/.config/systemd/user` is needed.

Start it now:

```bash
systemctl --user enable --now openvoice-desktop.service
```

## Update

Install the newer release the same way. Config, history, logs, and the Secret
Service token are untouched:

```bash
systemctl --user stop openvoice-desktop.service
sudo apt install ./OpenVoiceDesktop_<version>_amd64.deb
systemctl --user start openvoice-desktop.service
```

Each release ships a `.sha256` beside the package. Verify before installing:

```bash
sha256sum -c OpenVoiceDesktop_<version>_amd64.deb.sha256
```

## Configure

Open **OpenVoiceDesktop** from the app grid, or:

```bash
openvoice-desktop settings
```

Advanced users can edit the file directly:

```bash
"${EDITOR:-nano}" "$(openvoice-desktop config-path)"
```

Store the bearer token in Secret Service instead of plaintext config:

```bash
secret-tool store --label="OpenVoiceDesktop ASR token" service openvoice-desktop account asr-token
systemctl --user restart openvoice-desktop.service
```

Token lookup order is `OPENVOICE_DESKTOP_TOKEN`, Secret Service, then
`asr.bearer_token` in config.

## Shortcuts

GNOME owns global hotkeys — nothing in this process can grab a key the way Windows
does — so the shortcut you save in Settings is written into a GNOME custom keybinding.
The daemon reconciles them at startup and after every accepted save, so changing the
shortcut in Settings is enough and the old one stops working:

| dconf entry | Command | From |
| --- | --- | --- |
| `openvoice-toggle` | `openvoice-desktop toggle` | `behavior.shortcut` |
| `openvoice-language` | `openvoice-desktop cycle-language` | `behavior.language_shortcut` |

Each action keeps one entry, so a rebind edits it instead of adding another, and
custom shortcuts owned by other applications are left alone.

One consequence of GNOME's design is worth knowing: a custom keybinding runs a
**command**, not "notify the running app". A leftover binding pointing at another
dictation app therefore launches that app on every press even when it is not running.

Paste fallback order is `wtype`, `dotool`, `ydotool`, then `xdotool`, and
`output.typing_tool` pins one instead of auto-detecting.

**On GNOME, `wtype` cannot work.** Mutter does not implement the
`zwp_virtual_keyboard_v1` protocol, so `wtype` exits with *"Compositor does not
support the virtual keyboard protocol"* on every call and auto-detection falls
through to the next tool. Install `ydotool` and run its daemon, or pin it:

```bash
wtype -M ctrl -P v -p v -m ctrl   # prints the error above on GNOME
systemctl --user enable --now ydotoold
```

If none can type into the focused app, the transcript stays in the clipboard and a
notification is shown.

GNOME allows only one custom shortcut per key combination, and a leftover binding
from another dictation app keeps firing even after that app is closed — the
binding launches it again. Check what currently owns `Ctrl+Space` before blaming
OpenVoiceDesktop:

```bash
dconf dump /org/gnome/settings-daemon/plugins/media-keys/
```

Delete or rebind any entry whose `command` is not `openvoice-desktop`.

Custom shortcuts are not the only claimants. **IBus ships its input-method trigger
bound to `Control+space`**, the same combination as the default dictation shortcut,
so both react to one keypress:

```bash
gsettings get org.freedesktop.ibus.general.hotkey trigger
```

The daemon reports any such overlap at startup (`shortcut conflict: ... is also
claimed by ...`) but never edits another component's settings. Drop the entry
yourself, or move the dictation shortcut:

```bash
gsettings set org.freedesktop.ibus.general.hotkey trigger \
  "['Zenkaku_Hankaku', 'Alt+Kanji', 'Alt+grave', 'Hangul', 'Alt+Release+Alt_R']"
```

Logs:

```bash
journalctl --user -u openvoice-desktop.service -f
```

## Build the package locally

This repository runs no CI, so the published package is the one built here:

```bash
./scripts/setup-linux-build.sh   # one-time: apt build deps + pinned cargo-tauri
./scripts/build-linux-local.sh
```

Output:

```text
target/release/bundle/deb/*.deb
target/release/bundle/deb/*.deb.sha256
```

`build-linux-local.sh` runs `cargo test --features desktop-shell` and then verifies
the package payload with `scripts/verify-deb.sh` (binary, systemd user unit, desktop
`Exec`, declared dependencies) without needing root.

The install/upgrade/remove smoke does need root, and it writes to `/usr`, so run it
deliberately:

```bash
./scripts/test-deb-install.sh target/release/bundle/deb/*.deb
```

## Release

Releases are built and published from this host; nothing runs remotely.

```bash
# bump `version` in Cargo.toml and the package stanza in Cargo.lock, then commit
./scripts/release-local.sh --linux-only   # or omit the flag for both platforms
```

The script runs the test suite, bundles and verifies the package, checksums it,
tags `v<version>`, and uploads `OpenVoiceDesktop_<version>_amd64.deb` plus its
`.sha256` to the GitHub release. It refuses to publish from a dirty tree or a
commit that is not on the remote, so a published asset always maps to a pushed
commit. Add `--dry-run` to build and verify without publishing.

## Uninstall

```bash
systemctl --user disable --now openvoice-desktop.service
sudo apt remove open-voice-desktop
```

The apt package is named `open-voice-desktop` because Tauri kebab-cases the
product name; the binary, config directory, and systemd unit all keep
`openvoice-desktop`.

Config, history, logs, and the stored token remain for a later reinstall. To remove
the token too:

```bash
secret-tool clear service openvoice-desktop account asr-token
```

## Source install

The source flow in `scripts/install.sh` is unchanged and stays useful for
development. It installs to `~/.local/bin` and writes a user unit into
`~/.config/systemd/user/`, which takes precedence over the packaged unit. Remove
that file if you switch to the `.deb` and want the packaged unit to win.
