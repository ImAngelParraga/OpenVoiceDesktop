# Windows 10/11 V1

OpenVoiceDesktop on Windows runs as a resident tray app. It registers the configured recording shortcut itself (`Ctrl+Space` by default), so no external shortcut setup is required.

## Install

Because the repository is private, sign in to GitHub before opening its **Releases** page. Download `OpenVoiceDesktop_<version>_x64-setup.exe`, then double-click it.

Setup installs for the current user, so no administrator prompt is needed. It creates a Start menu shortcut and Windows uninstall entry, then opens the first-run panel. Configure endpoint, token, microphone, shortcut, and autostart there.

Silent install of an already-downloaded setup file:

```powershell
.\OpenVoiceDesktop_<version>_x64-setup.exe /S
```

Running a newer setup upgrades the installed app. Config, history, logs, and Windows Credential Manager token remain unchanged. Release assets include a `.sha256` checksum.

V1 binaries are unsigned. Windows SmartScreen may show a warning until the project has a code-signing certificate.

## Build on Ubuntu

This repository runs no CI. Ubuntu cross-builds the Windows MSVC executable and
NSIS setup locally using Tauri's `cargo-xwin` path, and the release assets are
uploaded from that same host by `scripts/release-local.sh`.

One-time setup:

```bash
./scripts/setup-windows-cross.sh
```

Default setup builds a reusable Docker image and needs no `sudo`. To install
the same pinned toolchain directly on an Ubuntu host instead, use
`./scripts/setup-windows-cross.sh --native`.

Build, including the local Rust test suite:

```bash
./scripts/build-windows-local.sh
```

For a rebuild after tests already passed:

```bash
./scripts/build-windows-local.sh --skip-tests
```

Output:

```text
target/x86_64-pc-windows-msvc/release/bundle/nsis/*-setup.exe
target/x86_64-pc-windows-msvc/release/bundle/nsis/*-setup.exe.sha256
```

Linux cross-compilation does not execute or smoke-test the Windows application,
and there is no CI runner to do it either. Native coverage means copying the
setup file to a Windows machine and running `scripts/test-installer.ps1` there.
After installation, run the interactive runtime matrix from a PowerShell prompt:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\test-windows-runtime.ps1 -ExePath "$env:LOCALAPPDATA\Programs\OpenVoiceDesktop\openvoice-desktop.exe"
```

Use the executable path reported by the installer if it differs. The matrix
records PASS/FAIL/NOT RUN and observations for W01–W10 in
`target/windows-runtime-smoke.md`; any failure or unrun case exits nonzero.
Do not include tokens or private transcript text in observations. The worksheet
does not run itself during cross-build, so a release stays unsmoked until a
Windows operator completes it.
Release notes state that the Windows asset is unsmoked for this reason.

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
- Toggle recording: configured shortcut (`Ctrl+Space` by default).
- First press starts recording.
- Second press stops, uploads WAV audio to `{base_url}/v1/audio/transcriptions`, and pastes the returned transcript.
- `Ctrl+Space` shows a compact bottom-of-screen status popup: animated bars while recording, `processing` while uploading/transcribing, and short feedback for empty transcript, busy, or error states. Successful insertion stays quiet. It does not use Windows notification balloons.

If the recording shortcut does nothing, another Windows feature or app may have captured it. Open Settings from the tray and choose a different shortcut. The cancel shortcut is registered only during dictation, so the default `Escape` remains available to other applications while OpenVoiceDesktop is idle. Push-to-talk mode stops recording when the configured recording chord is released.

The native status popup follows the saved overlay visibility, top/bottom position, compact/expanded style, and light/dark/system theme. It uses the work area and DPI of the monitor under the pointer when shown. Settings changes apply without restarting the resident app; a Save during dictation reports pending until the current dictation finishes.

### Native Windows checks after installing a new release

Cross-compilation cannot verify these interactions. On a Windows desktop, check each item and record pass/fail before calling the setup smoke-tested:

1. Start dictation with Settings closed. Change overlay visibility, position, style, and theme; Save after each change. Check the next popup, including on a secondary monitor with a different scale. Repeat with Settings open; only one popup should appear.
2. Change recording, cancel, and language shortcuts; Save and use each without restarting. Try a shortcut held by another app: Save should report the conflict, and the previous binding should still work. Confirm `Escape` works normally in other apps while idle.
3. Enable push-to-talk, Save, hold the recording chord, then release it. Recording should stop once. Press the chord briefly and release before startup finishes; it must not start another recording.
4. Use a slow ASR endpoint. During upload, query `openvoice-desktop status`, cancel with the configured shortcut or tray menu, and confirm no transcript is inserted. Save a setting during upload; it should report pending and take effect after dictation ends.
5. Change recording output mode or volume, Save, and start a new recording without restarting. Confirm output reduction applies and restores on stop. Test the actual playback endpoint used by the user.
6. Save without edits, then change one setting and Save again. Check “No changes” versus the single changed-field result. Success feedback should clear after six seconds; failed Save feedback should remain. Conflict or pending details should fit in the expandable area.
7. Change theme and interface language before Save. Appearance and navigation should change immediately; the title should remain Settings. Translation is currently partial: field labels and detailed messages remain English. Reopen Settings and check both selections persist.
8. Set an invalid numeric value and confirm the field shows an error before Save. Clear a previously set ASR language and confirm it saves as auto-detect. Change log level through its dropdown; turn live logs on and confirm lines refresh while Debug is open.
9. Connect or disconnect an audio device, then use **Refresh audio devices** without restarting Settings. Check new devices and default markers, and confirm a missing saved device remains visible with a fallback warning. Repeat refresh; if enumeration fails or hangs, Settings should remain open with its prior list.
10. During repeated playback-device failures, confirm recording output errors do not crash the app or steadily increase its process handle count. The Windows-only COM ownership unit tests cover release on each injected failure stage; this native check covers real endpoints.

The Windows adapter saves the current clipboard text, sets the transcript, sends `Ctrl+V`, then restores the previous clipboard after the configured delay.
Paste behavior follows `[output]`: `clipboard-paste` restores or leaves the clipboard according to `clipboard_handling`, `copy-only` never sends `Ctrl+V`, and `direct-type` uses the native `SendInput` path.
Paste errors are surfaced as user-facing error feedback while the transcript remains available through the clipboard/history path.

## Uninstall

Open **Settings → Apps → Installed apps**, select **OpenVoiceDesktop**, then choose **Uninstall**. App binaries, Start menu shortcut, and autostart registration are removed. Config, history, logs, and stored token remain for later reinstall.

To remove the retained token too:

```powershell
cmdkey /delete:openvoice-desktop/asr-token
```
