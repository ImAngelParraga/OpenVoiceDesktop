# Ubuntu / Debian (GNOME Wayland)

OpenVoiceDesktop on Linux runs as a resident user daemon (`openvoice-desktop daemon`)
plus a Tauri settings window (`openvoice-desktop settings`). There is no tray icon on
Linux, so the desktop entry opens Settings and the daemon is managed by systemd.

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

GNOME owns global hotkeys, so bind them in **Settings → Keyboard → Custom Shortcuts**:

| Name | Command | Shortcut |
| --- | --- | --- |
| OpenVoiceDesktop Toggle | `/usr/bin/openvoice-desktop toggle` | `Ctrl+Space` |
| OpenVoiceDesktop Language | `/usr/bin/openvoice-desktop cycle-language` | `Ctrl+Alt+L` |

Paste fallback order is `wtype`, `dotool`, `ydotool`, then `xdotool`. If none can
type into the focused app, the transcript stays in the clipboard and a notification
is shown.

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
