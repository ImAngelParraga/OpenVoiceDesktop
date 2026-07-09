# OpenVoiceDesktop

Resident Linux desktop dictation client for your own OpenAI-compatible ASR endpoint.

OpenVoiceDesktop is for users who already run a self-hosted or custom speech-to-text service and want a lightweight desktop dictation client for it. Unlike many dictation apps that only support local models or a fixed list of cloud providers, OpenVoiceDesktop lets you set the ASR `base_url`, `model`, and bearer token directly.

V1 targets GNOME Wayland. The project is structured as shared Rust core plus platform adapters so Windows support can be added without rewriting recording, config, ASR, or state-machine code.

## Why This Exists

Linux has several Whisper-based dictation tools, but most are local-inference first or tied to known providers. OpenVoiceDesktop is intentionally client-only: it records audio, sends it to your configured `{base_url}/v1/audio/transcriptions` endpoint, and inserts the returned transcript into the focused app.

Use it when:

- You run your own ASR server, proxy, or private OpenAI-compatible transcription endpoint.
- You need custom `base_url` support instead of only OpenAI/Groq/other built-in providers.
- You want a small resident daemon with a fast toggle command for GNOME shortcuts.
- You want tokens kept out of logs and preferably stored through Secret Service.

## Flow

1. Start the daemon at login: `openvoice-desktop daemon`
2. Bind GNOME `Ctrl+Space` to: `openvoice-desktop toggle`
3. First press starts recording.
4. Second press stops recording, writes a 16 kHz mono 16-bit PCM WAV, uploads it to `{base_url}/v1/audio/transcriptions`, and pastes the returned `text`.

WAV is the default because short desktop dictation clips are usually under 10 seconds. The payload is about 32 KB/sec, so a 10 second phrase is roughly 320 KB plus the WAV header. Opus can be added later if benchmarks show it improves end-to-end latency for long clips or slow networks.

## Easy Install

On your Linux laptop:

```bash
git clone https://github.com/ImAngelParraga/OpenVoiceDesktop.git
cd OpenVoiceDesktop
./scripts/install.sh --install-deps --install-rust --start
```

That command installs required Linux packages, installs Rust with rustup if needed, builds the release binary, installs it to `~/.local/bin/openvoice-desktop`, creates `~/.config/openvoice-desktop/config.toml`, installs the user systemd service, and starts the daemon.

Then edit the config:

```bash
nano ~/.config/openvoice-desktop/config.toml
```

Set your ASR endpoint:

```toml
[asr]
base_url = "http://127.0.0.1:9000"
model = "whisper-1"
language = "en"
```

Store your token in Secret Service:

```bash
secret-tool store --label="OpenVoiceDesktop ASR token" service openvoice-desktop account asr-token
systemctl --user restart openvoice-desktop.service
```

Finally, create a GNOME custom keyboard shortcut:

- Name: `OpenVoiceDesktop Toggle`
- Command: `/home/YOUR_USER/.local/bin/openvoice-desktop toggle`
- Shortcut: `Ctrl+Space`

Replace `YOUR_USER` with your Linux username, or run `which openvoice-desktop` and use that path.

Check daemon logs:

```bash
journalctl --user -u openvoice-desktop.service -f
```

## Build Manually

```bash
cargo test
cargo build --release
```

The binary is `target/release/openvoice-desktop`.

## Config

Default path:

```bash
openvoice-desktop config-path
```

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
2. Secret Service via `secret-tool lookup service openvoice-desktop account asr-token`
3. `asr.bearer_token` in config, only when the file is private (`chmod 600`)

Store the token in Secret Service:

```bash
secret-tool store --label="OpenVoiceDesktop ASR token" service openvoice-desktop account asr-token
```

## GNOME Wayland Setup

Install helpers:

```bash
sudo apt install wl-clipboard wtype libsecret-tools
```

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

The core modules are independent of GNOME:

- `core::config`: TOML config, defaults, token resolution, plaintext permission checks
- `core::recorder`: recorder trait plus CPAL microphone recorder
- `core::audio`: 16 kHz mono PCM WAV writing
- `core::asr`: OpenAI-compatible multipart ASR client and response parsing
- `core::controller`: idle to recording to transcribing to insertion state machine
- `core::platform`: clipboard/paste/notification trait and restore helper
- `platform::linux`: GNOME Wayland command-backed adapter

Timing logs include record duration, WAV write time, upload/response time, and paste time. Bearer tokens are never printed.
