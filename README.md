# OpenVoiceDesktop

Desktop dictation client for an OpenAI-compatible transcription server. Records your microphone, uploads audio, and pastes the returned text into the focused app. Supports Linux GNOME Wayland and Windows 10/11.

## OpenVoice family

Three projects for dictation across devices. Use them together for a self-hosted setup, or use each with other compatible clients and services.

| Project | Role |
| --- | --- |
| [Portable ASR](https://github.com/ImAngelParraga/portable-asr) | Self-hosted transcription server with optional transcript cleanup |
| [OpenVoiceIME](https://github.com/ImAngelParraga/OpenVoiceIME) | Android voice keyboard |
| [OpenVoiceDesktop](https://github.com/ImAngelParraga/OpenVoiceDesktop) | Dictation client for Linux and Windows |

[Explore the OpenVoice family](https://imangelparraga.github.io/OpenVoiceIME/#openvoice-family).

## Install

Download the package for your system from [GitHub Releases](https://github.com/ImAngelParraga/OpenVoiceDesktop/releases/latest). No source build or GitHub sign-in is required.

| Platform | Package | Install |
| --- | --- | --- |
| Ubuntu/Debian, x86-64 | `OpenVoiceDesktop_<version>_amd64.deb` | `sudo apt install ./OpenVoiceDesktop_<version>_amd64.deb` |
| Windows, x64 | `OpenVoiceDesktop_<version>_x64-setup.exe` | Run the installer for your user |

Replace `<version>` with the downloaded release version. Each asset has a `.sha256` checksum. Windows may show SmartScreen for unsigned builds.

## Configure and dictate

1. Open **OpenVoiceDesktop** and configure the ASR base URL, bearer token, model, microphone, and shortcut.
2. For [Portable ASR](https://github.com/ImAngelParraga/portable-asr#quickstart), use `http://127.0.0.1:9000` on the same PC, its configured token, and `whisper-1`. Use the server's reachable HTTPS URL from another PC.
3. Start the daemon; on Linux run `systemctl --user enable --now openvoice-desktop.service`. Enable autostart in Settings for later logins.
4. Press `Ctrl+Space` to start recording, then press it again to transcribe and paste.

`Ctrl+Alt+L` cycles configured transcription languages. Use the overlay Pause/Resume button or `Ctrl+Alt+P` to take a break within one recording: paused time is excluded from the audio. Change the pause/resume shortcut in Settings → General. The usual recording shortcut finishes and sends even while paused; Escape discards the whole recording. Choose a model from the server's refreshed model list, or enter a custom ID if discovery is unavailable. Cleanup runs on the server when configured there.

On GNOME, Settings manages the global shortcuts. Paste needs a helper supported by the desktop; `wtype` does not work on GNOME itself. Follow [Linux setup](docs/linux.md#shortcuts) to configure a compatible helper such as `ydotool`. If automatic paste fails, the transcript is left in the clipboard for manual paste.

## Privacy and configuration

Audio goes to your configured endpoint. Prefer the platform credential store or `OPENVOICE_DESKTOP_TOKEN` for the token. Transcript history and operational logs are local; review history settings on shared PCs. Configuration and credentials survive upgrades and uninstall.

[Configuration reference](docs/configuration.md) explains config paths and token lookup. [Linux guide](docs/linux.md) and [Windows guide](docs/windows.md) cover installation, updates, and troubleshooting.

## Build and contribute

For Linux development:

```bash
git clone https://github.com/ImAngelParraga/OpenVoiceDesktop.git
cd OpenVoiceDesktop
./scripts/install.sh --install-deps --install-rust --start
```

Build packages locally with `./scripts/build-linux-local.sh` or `./scripts/build-windows-local.sh`. See [development and releases](docs/development.md) for manual builds, local packaging, and verification. Releases are built locally; there is no remote CI build.
