# Windows 10/11 V1

OpenVoiceDesktop on Windows runs as a resident tray app. It registers `Ctrl+Space` itself, so no external shortcut setup is required.

## Install

Because the repository is private, sign in to GitHub before opening its **Releases** page. Download `OpenVoiceDesktop_<version>_x64-setup.exe`, then double-click it.

Setup installs for the current user, so no administrator prompt is needed. It creates a Start menu shortcut and Windows uninstall entry, then opens the first-run panel. Configure endpoint, token, microphone, shortcut, and autostart there.

Silent install of an already-downloaded setup file:

```powershell
.\OpenVoiceDesktop_<version>_x64-setup.exe /S
```

Running a newer setup upgrades the installed app. Config, history, logs, and Windows Credential Manager token remain unchanged. Release assets include a `.sha256` checksum.

V1 binaries are unsigned. Windows SmartScreen may show a warning until the project has a code-signing certificate.

## Configure

Use **OpenVoiceDesktop** from the Start menu to open first-run setup. Advanced users can edit:

```powershell
notepad "$env:APPDATA\openvoice-desktop\config.toml"
```

Set your OpenAI-compatible ASR endpoint:

```toml
[asr]
base_url = "http://127.0.0.1:9000"
model = "whisper-1"
language = "en"
```

Store the bearer token in Windows Credential Manager:

```powershell
cmdkey /generic:openvoice-desktop/asr-token /user:asr /pass:YOUR_TOKEN
```

Token lookup order is:

1. `OPENVOICE_DESKTOP_TOKEN`
2. Windows Credential Manager target `openvoice-desktop/asr-token`
3. `asr.bearer_token` in config as a fallback

## Use

- Start the app from the Start menu. Completed setup launches the resident tray app.
- Toggle recording: `Ctrl+Space`
- First press starts recording.
- Second press stops, uploads WAV audio to `{base_url}/v1/audio/transcriptions`, and pastes the returned transcript.
- `Ctrl+Space` shows a compact bottom-of-screen status popup: animated bars while recording, `processing` while uploading/transcribing, and short feedback for empty transcript, busy, or error states. Successful insertion stays quiet. It does not use Windows notification balloons.

If `Ctrl+Space` does nothing, another Windows feature or app may have captured that shortcut. Open Settings from the tray and choose a different shortcut.

The Windows adapter saves the current clipboard text, sets the transcript, sends `Ctrl+V`, then restores the previous clipboard after the configured delay.
Paste behavior follows `[output]`: `clipboard-paste` restores or leaves the clipboard according to `clipboard_handling`, `copy-only` never sends `Ctrl+V`, and `direct-type` uses the native `SendInput` path.
Paste errors are surfaced as user-facing error feedback while the transcript remains available through the clipboard/history path.

## Uninstall

Open **Settings → Apps → Installed apps**, select **OpenVoiceDesktop**, then choose **Uninstall**. App binaries, Start menu shortcut, and autostart task are removed. Config, history, logs, and stored token remain for later reinstall.

To remove the retained token too:

```powershell
cmdkey /delete:openvoice-desktop/asr-token
```
