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

## Recover failed transcriptions

Enable History and choose recording retention **Failed only** (or **Always**)
before dictating. On an upload failure or timeout, History keeps the recording.
Choose **Retry transcription** to send it again using current ASR settings;
the original recording language is preserved and recovered text is copied to
the clipboard. Another failure keeps the audio for another attempt. Delete the
entry to delete its retained audio; the history limit also removes old entries.
No recordings are retained under **Never**, or with History disabled.

Transcription deadlines use `max(5 seconds, audio duration)`, capped by
`asr.timeout_seconds` (default 30 seconds). Short clips get five seconds; a
minute gets thirty seconds with the default cap. This conservative policy was
calibrated against 20 recent server-history measurements: seven clips under
five seconds had a 360 ms median and 694 ms maximum; a 66.84-second clip took
1,896 ms, while a 21.74-second outlier took 18,893 ms. The cap remains editable
for slower servers. These server measurements exclude client network overhead;
the request deadline includes upload and response-body reading.
