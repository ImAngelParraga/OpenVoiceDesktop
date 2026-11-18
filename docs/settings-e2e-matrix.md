# Settings end-to-end matrix

Automated baseline: `cargo test --test settings_e2e_tests`. Windows native-window smoke: `powershell -ExecutionPolicy Bypass -File scripts/test-settings-ui.ps1` after release build.

GNOME Wayland manual pass:

1. Launch daemon and `openvoice-desktop settings`; second Settings launch focuses same window.
2. Verify every enabled field renders saved value; unavailable fields show reason.
3. Change shortcut, microphone, overlay, output, VAD, history, logging; Save reports applied/restart-required accurately.
4. Exercise Test Endpoint, history copy/retry/save/delete, open recordings/log/app-data, tray Cancel, Copy Last Transcript, Test Endpoint, Open Logs.
5. Confirm `wl-copy`/`wtype` path, missing-tool error, missing-device fallback, shortcut conflict rollback.
6. Confirm tokens/transcripts absent from logs and errors sanitized.

## Native Linux regression checks

The v0.1.35 audit exercised the Tauri settings window with WebKitWebDriver and Xvfb, an isolated config and daemon, and a local ASR fixture. At 720 × 520, all six sections fit and the Save button stayed visible. The pass saved and read back 37 preferences, including custom sound paths, language, theme, output delays and history retention.

To repeat the regressions with a disposable profile:

1. Edit the ASR prompt. Store a token, open History and mark an entry saved. Return to ASR and check that the draft is intact. Test endpoint should ask you to save the ASR changes first.
2. Save and start a delayed endpoint check. Change and save the theme while the request runs. Reopen Settings and check both the theme and the returned diagnostic metrics.
3. Seed more than 25 history entries, page through them, delete one and retry a failed entry with retained audio. Check the recovered text in the clipboard. A failed entry without audio should explain why retry is unavailable.
4. Select the custom sound theme and enter the three WAV paths. Verify the values survive Save. On GNOME, push-to-talk and the debug shortcut should show why they are unavailable.
5. Check the native webview for rejected promises after Save. The panel should retain the section the user last selected, including when they navigate during Save.

The audit used fixture clipboard and file-manager helpers. Physical microphone capture, compositor paste and native Windows hotkeys still require their platform smoke checks.
