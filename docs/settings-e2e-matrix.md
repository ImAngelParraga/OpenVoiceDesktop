# Settings end-to-end matrix

Automated baseline: `cargo test --test settings_e2e_tests`. Windows native-window smoke: `powershell -ExecutionPolicy Bypass -File scripts/test-settings-ui.ps1` after release build.

GNOME Wayland manual pass:

1. Launch daemon and `openvoice-desktop settings`; second Settings launch focuses same window.
2. Verify every enabled field renders saved value; unavailable fields show reason.
3. Change shortcut, microphone, overlay, output, VAD, history, logging; Save reports applied/restart-required accurately.
4. Exercise Test Endpoint, history copy/retry/save/delete, open recordings/log/app-data, tray Cancel, Copy Last Transcript, Test Endpoint, Open Logs.
5. Confirm `wl-copy`/`wtype` path, missing-tool error, missing-device fallback, shortcut conflict rollback.
6. Confirm tokens/transcripts absent from logs and errors sanitized.
