# Windows 10/11 V1

OpenVoiceDesktop on Windows runs as a resident tray app. It registers `Ctrl+Space` itself, so no external shortcut setup is required.

## Install

Run PowerShell:

```powershell
iwr https://raw.githubusercontent.com/ImAngelParraga/OpenVoiceDesktop/main/scripts/install.ps1 -OutFile install.ps1
powershell -ExecutionPolicy Bypass -File .\install.ps1 -Start
```

The installer downloads the latest prebuilt release, installs `openvoice-desktop.exe` to `%LOCALAPPDATA%\OpenVoiceDesktop`, creates `%APPDATA%\openvoice-desktop\config.toml` if missing, and adds a per-user Startup folder shortcut.

V1 binaries are unsigned. Windows SmartScreen may show a warning until the project has a code-signing certificate.

## Configure

Edit:

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

- Start the tray app: `%LOCALAPPDATA%\OpenVoiceDesktop\openvoice-desktop.exe daemon`
- Toggle recording: `Ctrl+Space`
- First press starts recording.
- Second press stops, uploads WAV audio to `{base_url}/v1/audio/transcriptions`, and pastes the returned transcript.

The Windows adapter saves the current clipboard text, sets the transcript, sends `Ctrl+V`, then restores the previous clipboard after the configured delay.

## Uninstall

```powershell
Remove-Item "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Startup\OpenVoiceDesktop.lnk" -ErrorAction SilentlyContinue
Remove-Item "$env:LOCALAPPDATA\OpenVoiceDesktop" -Recurse -Force
```

Remove the stored token:

```powershell
cmdkey /delete:openvoice-desktop/asr-token
```
