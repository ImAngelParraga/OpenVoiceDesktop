# OpenVoiceDesktop

Resident desktop dictation client for your own OpenAI-compatible ASR endpoint.

OpenVoiceDesktop is for users who already run a self-hosted or custom speech-to-text service and want a lightweight desktop dictation client for it. Unlike many dictation apps that only support local models or a fixed list of cloud providers, OpenVoiceDesktop lets you set the ASR `base_url`, `model`, and bearer token directly.

V1 targets GNOME Wayland on Linux and Windows 10/11. The project is structured as shared Rust core plus platform adapters so OS-specific hotkeys, tray behavior, clipboard insertion, and install flows stay isolated.

## Why This Exists

Linux has several Whisper-based dictation tools, but most are local-inference first or tied to known providers. OpenVoiceDesktop is intentionally client-only: it records audio, sends it to your configured `{base_url}/v1/audio/transcriptions` endpoint, and inserts the returned transcript into the focused app.

Use it when:

- You run your own ASR server, proxy, or private OpenAI-compatible transcription endpoint.
- You need custom `base_url` support instead of only OpenAI/Groq/other built-in providers.
- You want a small resident daemon/tray app with a fast global toggle hotkey.
- You want tokens kept out of logs and preferably stored through Secret Service.

## Flow

1. Start the resident app at login: `openvoice-desktop daemon`
2. Press `Ctrl+Space` to toggle recording.
3. First press starts recording.
4. Second press stops recording, writes a 16 kHz mono 16-bit PCM WAV, uploads it to `{base_url}/v1/audio/transcriptions`, and pastes the returned `text`.

Press `Ctrl+Alt+L` while idle or recording to cycle configured transcription languages. Recording overlay shows current language and offers same action. Selection applies when recording stops; audio remains one clip.

On Windows, the tray app shows a compact bottom-of-screen popup: animated bars while recording, `processing` while uploading/transcribing, and short feedback only for empty transcript, busy, or error states. Successful insertion stays quiet.

WAV is the default because short desktop dictation clips are usually under 10 seconds. The payload is about 32 KB/sec, so a 10 second phrase is roughly 320 KB plus the WAV header. Opus can be added later if benchmarks show it improves end-to-end latency for long clips or slow networks.

## Windows Easy Install

No terminal or source checkout required:

1. Sign in to GitHub and open this repository's **Releases** page.
2. Download `OpenVoiceDesktop_<version>_x64-setup.exe`.
3. Double-click it and finish setup.

Setup installs for the current user without an administrator prompt, creates a Start menu shortcut and Windows uninstaller, then opens the first-run setup panel. Configure the ASR endpoint, token, microphone, shortcut, and autostart there. Existing settings and credentials survive upgrades and uninstall.

For a silent install of an already-downloaded setup file:

```powershell
.\OpenVoiceDesktop_<version>_x64-setup.exe /S
```

Unsigned personal builds can trigger Windows SmartScreen. Verify the adjacent `.sha256` checksum from the release before running the setup file.

Source builds remain available for development:

```powershell
git clone https://github.com/ImAngelParraga/OpenVoiceDesktop.git
cd OpenVoiceDesktop
powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1 -BuildFromSource -Start
```

Windows usage and uninstall notes are in [docs/windows.md](docs/windows.md).

## Linux Easy Install

No terminal build or source checkout required on Ubuntu/Debian:

1. Sign in to GitHub and open this repository's **Releases** page.
2. Download `OpenVoiceDesktop_<version>_amd64.deb`.
3. Install it:

```bash
sudo apt install ./OpenVoiceDesktop_<version>_amd64.deb
```

That installs `/usr/bin/openvoice-desktop`, a launcher that opens the settings
window, and a systemd **user** unit at
`/usr/lib/systemd/user/openvoice-desktop.service`. The unit is not enabled until you
turn on autostart in Settings or run:

```bash
systemctl --user enable --now openvoice-desktop.service
```

Reinstalling a newer release upgrades in place and preserves config, history, logs,
and the Secret Service token. Each release ships a `.sha256` beside the package.

Open **OpenVoiceDesktop** from the app grid to configure the ASR endpoint, token,
shortcut, and autostart, or edit the config directly:

```bash
"${EDITOR:-nano}" "$(openvoice-desktop config-path)"
```

```toml
[asr]
base_url = "http://127.0.0.1:9000"
model = "whisper-1"
language = "en"
language_options = ["en", "es"]
```

Store your token in Secret Service:

```bash
secret-tool store --label="OpenVoiceDesktop ASR token" service openvoice-desktop account asr-token
systemctl --user restart openvoice-desktop.service
```

GNOME owns global hotkeys, so add two custom keyboard shortcuts in
**Settings → Keyboard**:

- `OpenVoiceDesktop Toggle` → `/usr/bin/openvoice-desktop toggle` → `Ctrl+Space`
- `OpenVoiceDesktop Language` → `/usr/bin/openvoice-desktop cycle-language` → `Ctrl+Alt+L`

The daemon registers these itself from Settings, and adds a third one for
`behavior.cancel_shortcut` (default `Escape`) only while a dictation is running, so
Escape keeps working everywhere else.

Check daemon logs:

```bash
journalctl --user -u openvoice-desktop.service -f
```

Full Linux install, update, uninstall, and packaging notes are in
[docs/linux.md](docs/linux.md).

### Linux Source Install

For development, the source flow installs to `~/.local/bin` and writes its own user
unit:

```bash
git clone https://github.com/ImAngelParraga/OpenVoiceDesktop.git
cd OpenVoiceDesktop
./scripts/install.sh --install-deps --install-rust --start
```

That command installs required Linux packages, installs Rust with rustup if needed,
builds the release binary, installs it to `~/.local/bin/openvoice-desktop`, creates
`~/.config/openvoice-desktop/config.toml`, installs the user systemd service, and
starts the daemon. A unit in `~/.config/systemd/user/` takes precedence over the
packaged one, so remove it if you later switch to the `.deb`.

## Build Manually

```bash
cargo test
cargo build --release --features desktop-shell
```

The binary is `target/release/openvoice-desktop`.

Build the Linux release `.deb` locally:

```bash
./scripts/setup-linux-build.sh  # one-time apt deps + pinned cargo-tauri
./scripts/build-linux-local.sh
```

The package and SHA-256 file are written under `target/release/bundle/deb/`.

Build the Windows NSIS installer from Ubuntu by cross-compiling:

```bash
./scripts/setup-windows-cross.sh  # one-time Docker image setup
./scripts/build-windows-local.sh
```

The installer and SHA-256 file are written under
`target/x86_64-pc-windows-msvc/release/bundle/nsis/`. Docker is the default, so
host package installation and administrator access are not required.

## Release

This repository runs **no CI**. Every published binary is compiled on the
maintainer's Ubuntu host and uploaded from it, so bump `version` in `Cargo.toml`
(and the package stanza in `Cargo.lock`), commit, push, then:

```bash
./scripts/release-local.sh            # both platforms
./scripts/release-local.sh --dry-run  # build and verify, publish nothing
```

The script refuses to publish from a dirty tree or a commit that is not on the
remote, runs the test suite, bundles the Debian package and the Windows setup,
checksums both, tags `v<version>`, and uploads the four assets to the GitHub
release. `--linux-only` and `--windows-only` release one platform at a time.

## Config

Default path:

```bash
openvoice-desktop config-path
```

Open settings panel:

```bash
openvoice-desktop settings
```

This starts a local browser panel for persisted settings and saves changes back
to the same TOML config. On Windows, tray `Settings` opens this panel.

Example:

```toml
[asr]
# Any OpenAI-compatible transcription endpoint root.
# Requests are sent to {base_url}/v1/audio/transcriptions.
base_url = "http://127.0.0.1:9000"
model = "whisper-1"
language = "en"
# prompt = "Optional vocabulary or formatting hint"

[audio]
sample_rate_hz = 16000
channels = 1

[behavior]
restore_clipboard = true
clipboard_restore_delay_ms = 250
```

Token lookup order:

1. `OPENVOICE_DESKTOP_TOKEN`
2. Platform secret store:
   - Linux: Secret Service via `secret-tool lookup service openvoice-desktop account asr-token`
   - Windows: Credential Manager target `openvoice-desktop/asr-token`
3. `asr.bearer_token` in config as a fallback. On Unix, plaintext token config files must be private (`chmod 600`).

Store the token in Secret Service:

```bash
secret-tool store --label="OpenVoiceDesktop ASR token" service openvoice-desktop account asr-token
```

## GNOME Wayland Setup

Install helpers:

```bash
sudo apt install wl-clipboard wtype libsecret-tools
```

Linux paste fallback order is `wtype`, `dotool`, `ydotool`, then `xdotool`.
`wl-copy`/`wl-paste` handle clipboard restore. Install `dotool`,
`ydotool`, or `xdotool` only if your desktop needs that fallback.

Install and enable the user service:

```bash
mkdir -p ~/.config/systemd/user
cp systemd/openvoice-desktop.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now openvoice-desktop.service
```

Create a custom keyboard shortcut in GNOME Settings:

- Name: `OpenVoiceDesktop Toggle`
- Command: `openvoice-desktop toggle`
- Shortcut: `Ctrl+Space`

If GNOME or the active app blocks synthetic paste, the transcript is left in the clipboard and a notification is shown. Paste manually with `Ctrl+V`.

## Development Notes

The core modules are independent of GNOME and Windows:

- `core::config`: TOML config, defaults, token resolution, plaintext permission checks
- `core::recorder`: recorder trait plus CPAL microphone recorder
- `core::audio`: 16 kHz mono PCM WAV writing
- `core::asr`: OpenAI-compatible multipart ASR client and response parsing
- `core::controller`: idle to recording to transcribing to insertion state machine
- `core::platform`: clipboard/paste/notification trait and restore helper
- `platform::linux`: GNOME Wayland command-backed adapter
- `platform::windows`: Windows tray, hotkey, clipboard paste, and status popup adapter

Timing logs include record duration, WAV write time, upload/response time, and paste time. Bearer tokens are never printed.

V1 intentionally excludes local model download/delete/select, GPU accelerator,
model unload, local headless transcription, and local streaming internals.
Configure remote ASR endpoint settings instead.
