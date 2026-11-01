# Configuration

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
