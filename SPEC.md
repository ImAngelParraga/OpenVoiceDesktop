# OpenVoiceDesktop Handy-Style UX Spec

§G
G1: Build OpenVoiceDesktop into Handy-like desktop dictation app: fast visible feedback, polished settings, tray, overlay, configurable behavior.
G2: Keep primary transcription path remote OpenAI-compatible ASR: record audio, send `{base_url}/v1/audio/transcriptions`, paste returned text.
G3: Use Handy as UX/reference code, not fork base. Handy ref: `https://github.com/cjpais/Handy`, commit `1c4f21acd2bab81f75a0cd7556cd3bb7399ffd9a`, release `v0.9.1`.
G4: Settings is production control surface, not schema preview: every visible value/action must describe and control real runtime behavior.

§C
C1: No local offline model manager in v1. Do not add `transcribe-cpp`, `transcribe-rs`, ONNX, Vulkan, Metal, model downloads, or model unload controls unless later product decision changes.
C2: Remote ASR endpoint config stays first-class: `base_url`, `model`, token source, language, prompt, timeout/retry.
C3: Use Tauri 2 desktop shell with bundled frontend assets and system webview (WebView2 on Windows); core ASR/record/paste stays testable as Rust library. Do not open settings in external browser or expose production settings through localhost HTTP.
C4: GNOME Wayland and Windows 10/11 remain target platforms. macOS support can be designed but not required by current README.
C5: Secrets never logged. Token UI must store through platform secret store when possible; config plaintext token remains fallback only.
C6: Handy code references use `H:<path>` relative to Handy repo at pinned commit. New agent should clone with: `git clone https://github.com/cjpais/Handy /tmp/Handy-review && git -C /tmp/Handy-review checkout 1c4f21acd2bab81f75a0cd7556cd3bb7399ffd9a`.
C7: Preserve low dependency surface unless feature requires app shell. Avoid importing Handy local inference dependencies.
C8: UX must feel immediate even when remote ASR latency exists: state changes visible before network call starts.
C9: Prefer desktop webview window over platform-native widget toolkit. True WinUI widgets are out of scope unless webview accessibility or integration fails.
C10: No visible enabled control/action may be a placeholder. Unsupported optional features stay hidden or visibly disabled with exact availability reason.
C11: Settings save must either apply atomically to running daemon or explicitly mark field restart-required before save; silent stale runtime forbidden.

§I
I.cli: Existing CLI: `openvoice-desktop daemon|toggle|config-path`. Add Handy-like remote-control flags only if compatible: `--toggle-transcription`, `--cancel`, `--start-hidden`, `--debug`.
I.config: Existing TOML config. Add schema version and settings UI backing store. Current fields: `[asr]`, `[audio]`, `[behavior]`.
I.asr: Remote ASR client in `src/core/asr.rs`. Must keep multipart `file`, `model`, `temperature`, optional `language`, optional `prompt`, bearer auth.
I.controller: State machine in `src/core/controller.rs`. Extend with cancel/busy/error/feedback states, not local model states.
I.platform: Platform adapters in `src/platform/linux.rs` and `src/platform/windows.rs`. Extend paste, tray, notification, permission, autostart.
I.handy.settings: `H:src-tauri/src/settings.rs` contains settings schema and defaults. Use as settings inventory reference.
I.handy.sidebar: `H:src/components/Sidebar.tsx` defines sections: general, models, advanced, history, postprocessing, debug, about.
I.handy.general: `H:src/components/settings/general/GeneralSettings.tsx` covers shortcut, push-to-talk, cancel, model card, microphone, mute, audio feedback, output device, volume.
I.handy.advanced: `H:src/components/settings/advanced/AdvancedSettings.tsx` covers startup/tray/overlay/output/transcription/history/experimental.
I.handy.debug: `H:src/components/settings/debug/DebugSettings.tsx` covers log level, update checks, sound theme, word correction threshold, paste delays, recording buffer, always-on mic, clamshell mic, live logs.
I.handy.overlay: `H:src/overlay/RecordingOverlay.tsx`, `H:src/overlay/RecordingOverlay.css` implement recording/streaming/transcribing/processing overlay with waveform, timer, cancel.
I.handy.tray: `H:src-tauri/src/tray.rs` implements tray icon state, menu, copy last transcript, settings, updates, quit.
I.handy.shortcuts: `H:src-tauri/src/transcription_coordinator.rs`, `H:src-tauri/src/shortcut/mod.rs`, `H:src-tauri/src/shortcut/handy_keys.rs`, `H:src-tauri/src/shortcut/tauri_impl.rs` implement shortcut engines, push-to-talk, cancel, debounce.
I.handy.audio: `H:src-tauri/src/commands/audio.rs`, `H:src-tauri/src/managers/audio.rs`, `H:src-tauri/src/audio_toolkit/audio/recorder.rs` implement mic/output devices, always-on mic, mute, clamshell mic, audio levels.
I.handy.paste: `H:src-tauri/src/clipboard.rs` implements paste methods, typing tools, clipboard handling, external script, auto-submit.
I.handy.history: `H:src-tauri/src/managers/history.rs`, `H:src/components/settings/history/HistorySettings.tsx` implement SQLite history, recording retention, retry, saved flag, audio path.
I.handy.text: `H:src-tauri/src/audio_toolkit/text.rs` implements custom words, fuzzy correction, filler filtering.
I.handy.postprocess: `H:src-tauri/src/actions.rs`, `H:src-tauri/src/llm_client.rs`, `H:src/components/settings/post-processing/PostProcessingSettings.tsx` implement LLM post-processing. Optional for OpenVoiceDesktop.
I.handy.onboarding: `H:src/App.tsx`, `H:src/components/onboarding/*`, `H:src/components/AccessibilityPermissions.tsx` implement first-run permission/model flow. Adapt model step to ASR endpoint setup.
I.handy.updates: `H:src/components/update-checker/UpdateChecker.tsx`, `H:src/components/whats-new/*`, `H:src-tauri/src/shortcut/mod.rs` update-related settings. Optional until releases exist.
I.handy.i18n: `H:src/i18n/*`, `H:src-tauri/src/tray_i18n.rs`, locale JSON files implement app language and tray translations.
I.handy.portable: `H:src-tauri/src/portable.rs`, `H:src-tauri/src/cli.rs` implement portable mode and extra CLI. Optional.
I.shell: Tauri 2 owns single-instance lifecycle, settings window, tray, overlays, global shortcuts, native dialogs/openers, autostart, and app events. Existing Rust core remains library/backend.
I.settings.status: `W` = runtime-effective after daemon restart, `P` = partial/model-only, `D` = dead/misleading. Audit baseline 2026-07-10 follows.
I.settings.general: `D behavior.shortcut`, `D behavior.push_to_talk`, `D behavior.cancel_shortcut`, `D audio.selected_microphone`, `D audio.mute_while_recording`, `D audio.audio_feedback`, `D audio.selected_output_device`, `D audio.always_on_microphone`, `D audio.audio_feedback_volume`, `D audio.sound_theme`, `D audio.sample_rate_hz`, `D audio.channels`. Windows hotkey is hardcoded `Ctrl+Space`; recorder always uses default input and fixed 16 kHz mono output; sound/mute/always-on paths are not invoked.
I.settings.asr: `W asr.base_url`, `W asr.model`, `W asr.language`, `W asr.prompt`, `W asr.timeout_seconds` after restart; `P asr.bearer_token` because UI is plaintext password input, not token-source/status/secret-store workflow; `D test-endpoint` action.
I.settings.app: `D app.start_hidden`, `D app.autostart`, `D app.show_tray_icon`, `D app.show_overlay`, `D app.overlay_position`, `D app.overlay_style`, `D app.experimental`, `D ui.language`, `D ui.theme`, `D updates.check_for_updates`, `D updates.show_whats_new`, `D portable.enabled`, `D behavior.restore_clipboard`, `D behavior.clipboard_restore_delay_ms`. Overlay/theme/tray models exist but active Windows shell does not consume them.
I.settings.output: `P output.paste_method` because clipboard/direct/copy work but external-script does not execute; `D output.typing_tool`; `W output.clipboard_handling`, `W output.paste_delay_ms`, `W output.restore_delay_ms` after restart; `D output.auto_submit`, `D output.external_script_path`.
I.settings.text: `D text.custom_words`, `D text.fuzzy_correction_threshold`, `D text.translate_language_hint`, `D text.vad_enabled`, `D text.silence_trim_enabled`; `W text.filter_filler_words`, `W text.append_trailing_space` after restart. VAD helper exists but controller never calls it.
I.settings.postprocess: `D post_processing.enabled`, `D post_processing.base_url`, `D post_processing.model`, `D post_processing.bearer_token`, `D post_processing.timeout_seconds`; no chat request/runtime consumer.
I.settings.history: `D history.enabled`, `D history.limit`, `D history.retention`, `D history.recordings_dir`; `D open-recordings-folder`, `D delete-transcript`, `D retry-transcript`, `D save-transcript`; no persistence store or action handlers.
I.settings.debug: `D debug.debug_shortcut`, `D debug.log_level`, `D debug.live_log_viewer`, `D debug.paste_delay_ms`, `D debug.recording_buffer_ms`, `D debug.pre_recording_buffer_ms`, `D debug.post_recording_buffer_ms`, `D debug.structured_logs`, `D debug.log_directory`; `D diagnostics.store_last_latency_metrics`, `D diagnostics.last_upload_latency_ms`, `D diagnostics.last_http_status`, `D diagnostics.last_sanitized_body`; `P ipc.socket_path` because runtime uses it on Linux but panel cannot render/save it and Windows uses fixed named pipe; `D open-log-directory`, `D open-app-data-directory`.
I.settings.hidden: `D audio.custom_start_sound`, `D audio.custom_stop_sound`, `D audio.custom_error_sound`, `D updates.release_channel`, `D portable.data_dir`, `D onboarding.completed`, `D onboarding.show_permission_repair`; config fields exist but no complete UI/runtime workflow.
I.settings.about: `D version/config-path/app-data-path/logs-path/license/repository/handy-acknowledgement`; model exists but browser renderer skips every unbound/read-only control.
I.settings.crosscut: Browser panel skips all unbound `Action`/`ReadOnly` controls, lacks validation/ranges/options for many bound controls, does not enumerate devices, and writes config without notifying/reloading daemon.
I.tray.audit: Working: Settings, Cancel, Open Config Folder, Quit. Dead/misleading: Copy Last Transcript, Test Endpoint, Open Logs. Menu enable/state model is not connected to Win32 menu.

§R
R1: Tauri 2 Windows uses Microsoft Edge WebView2; Windows 10/11 normally provide runtime. `https://v2.tauri.app/start/prerequisites/`; `https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution`
R2: Tauri 2 global-shortcut plugin supports Windows/Linux registration plus pressed/released events, fitting toggle and push-to-talk. `https://v2.tauri.app/plugin/global-shortcut/`
R3: Tauri 2 official desktop surfaces cover system tray, autostart, logging, opener, single-instance, updater, window state. `https://v2.tauri.app/plugin/`

§V
V1: Remote ASR invariant: every transcription request goes to `{base_url.trim_end_matches('/')}/v1/audio/transcriptions`; no local model inference path used by default.
V2: Secrets invariant: bearer tokens/API keys redacted from logs, UI debug output, error messages, and history.
V3: Feedback invariant: user receives visible state within 100 ms of accepted toggle/cancel command, before audio write or network call.
V4: Busy invariant: second toggle during writing/transcribing/inserting does not start nested recording; returns busy or cancel-capable state.
V5: Cancel invariant: cancel stops active recording or suppresses pending paste/network result; final state becomes idle and overlay hides.
V6: Paste invariant: paste failure never loses transcript; text remains available through clipboard/history/error UI.
V7: Clipboard invariant: when restore enabled, original clipboard is restored after paste delay; when disabled/copy-only, user-selected behavior respected.
V8: Config invariant: settings schema migrations preserve existing TOML values and never overwrite user token/source silently.
V9: Platform invariant: Linux Wayland path prefers `wtype`/`wl-copy` when available; X11 path supports `xdotool`; Windows path uses native clipboard and `SendInput`.
V10: History invariant: completed transcript saved with timestamp, text, status, endpoint/model metadata, and optional audio file only according to retention policy.
V11: UI invariant: all settings visible in UI map to persisted config and can be changed without editing TOML, except explicit advanced raw config path.
V12: Test invariant: every new platform-independent feature has unit tests; platform behavior has adapter boundary tests or documented manual verification.
V13: Handy-reference invariant: every Handy-inspired feature task cites `H:<path>` so new agent can inspect upstream behavior.
V14: Visible settings invariant: Settings UI must be user-invokable from the app surface (`openvoice-desktop settings` and tray Settings where tray exists), render persisted config fields, and save edits without hand-editing TOML.
V15: Honest-control invariant: every visible enabled settings control/action has automated proof of current-value render, validated persistence/action, and runtime consumption; otherwise hidden or disabled with reason.
V16: Native-window invariant: tray Settings focuses one desktop application window; no external browser, localhost listener, duplicate settings process, or duplicate daemon.
V17: Live-config invariant: successful Save reports per-field applied/restart-required status; runtime-effective fields update daemon before success response; invalid update preserves prior config/runtime.
V18: Shortcut invariant: displayed toggle/push-to-talk/cancel shortcuts equal registered shortcuts; default toggle is `Ctrl+Space`; conflicts return visible error without losing prior registration.
V19: Device invariant: microphone/output selectors enumerate current devices, identify system default, persist stable selection, use selected device, and fall back visibly when device disappears.
V20: Audio-settings invariant: ASR encoding displays effective defaults (`16000` Hz, mono) and validates supported values; feedback device/volume/theme and mute/always-on controls affect real audio lifecycle or remain unavailable.
V21: Action invariant: every tray/settings action performs named operation and reports success/sanitized failure; no placeholder redirects such as `Action available in settings UI`.
V22: Completeness invariant: CI maintains explicit inventory of every config field and settings action; adding visible field/action without render, validation, persistence, runtime/action, and test fails.
V23: Desktop-shell invariant: `cargo test --features desktop-shell` passes on Linux and Windows; every Tauri configured/generated bundle asset exists.

§T
id|status|task|detail|Handy ref|cites
T1|x|Design app shell architecture|Decide Tauri/React shell vs current daemon-only UI. Keep Rust core reusable. Define IPC events: recording, processing, inserted, empty, busy, error.|H:src/App.tsx; H:src-tauri/src/lib.rs|V1,V3,I.controller
T2|x|Settings schema v1|Add schema version, migration, defaults, persistence. Map existing TOML into UI settings without token loss.|H:src-tauri/src/settings.rs|V2,V8,V11,I.config
T3|x|Settings window layout|Build Handy-like sidebar sections adapted to our app: General, ASR Endpoint, Advanced, History, Debug, About. No Models section except remote ASR model field.|H:src/components/Sidebar.tsx|V11,V13,I.handy.sidebar
T4|x|General settings|Shortcut, push-to-talk, cancel shortcut, microphone selector, mute while recording, audio feedback, output device, volume.|H:src/components/settings/general/GeneralSettings.tsx|V3,V5,V11
T5|x|ASR endpoint settings|UI for `base_url`, `model`, `language`, `prompt`, token source/status, test endpoint, timeout. Replace Handy local model card with endpoint card.|H:src/components/settings/general/ModelSettingsCard.tsx; H:src/components/model-selector/*|V1,V2,V11,I.asr
T6|x|Advanced app settings|Start hidden, autostart, tray icon visibility, overlay style/position, experimental toggle.|H:src/components/settings/advanced/AdvancedSettings.tsx; H:src/components/settings/StartHidden.tsx; H:src/components/settings/AutostartToggle.tsx; H:src/components/settings/ShowTrayIcon.tsx; H:src/components/settings/ShowOverlay.tsx|V3,V11
T7|x|Output settings|Paste method, typing tool, clipboard handling, auto-submit, paste delays, external script path.|H:src/components/settings/PasteMethod.tsx; H:src/components/settings/TypingTool.tsx; H:src/components/settings/ClipboardHandling.tsx; H:src/components/settings/AutoSubmit.tsx; H:src-tauri/src/clipboard.rs|V6,V7,V9,V11
T8|x|Transcription text settings|Custom words, fuzzy correction threshold, filler-word filtering, append trailing space, optional translate language hint. For remote ASR, apply after ASR result unless endpoint handles it.|H:src/components/settings/CustomWords.tsx; H:src/components/settings/AppendTrailingSpace.tsx; H:src-tauri/src/audio_toolkit/text.rs|V1,V10,V11
T9|x|History settings|History limit, recording retention, open recordings folder, delete/retry/save transcript. Save endpoint/model metadata. Audio retention opt-in.|H:src/components/settings/history/HistorySettings.tsx; H:src-tauri/src/managers/history.rs; H:src-tauri/src/commands/history.rs|V6,V10,V11
T10|x|Debug settings|Debug mode shortcut, log level, live log viewer, log directory open, app data directory open, paste delay controls, recording buffer.|H:src/components/settings/debug/DebugSettings.tsx; H:src/components/settings/debug/*; H:src-tauri/src/commands/mod.rs|V2,V11,V12
T11|x|About settings|Version, license, repo link, config path, app data path, logs path, acknowledgements including Handy as UX reference.|H:src/components/settings/about/AboutSettings.tsx|V11,V13
T12|x|Onboarding flow|First run asks permissions, endpoint URL/model/token, microphone, shortcut. Skip Handy model download step. Returning user only sees missing permission repair.|H:src/App.tsx; H:src/components/onboarding/*; H:src/components/AccessibilityPermissions.tsx|V1,V2,V8,V11
T13|x|Permission checks|Windows mic privacy status/open settings; Linux dependency checks for `wl-copy`, `wl-paste`, `wtype`, `xdotool`; future macOS accessibility/mic if supported.|H:src/components/AccessibilityPermissions.tsx; H:src-tauri/src/commands/audio.rs|V9,V11
T14|x|Recording overlay minimal|Show recording pill with waveform bars, elapsed timer, cancel button, top/bottom position, hide on insert/empty/error.|H:src/overlay/RecordingOverlay.tsx; H:src/overlay/RecordingOverlay.css|V3,V5,V13
T15|x|Processing overlay|Show transcribing/processing state immediately after stop; distinguish writing audio, uploading, inserting, error. No fake success message on inserted text.|H:src/overlay/RecordingOverlay.tsx; H:src-tauri/src/actions.rs|V3,V4,V6
T16|x|Live overlay optional|If remote endpoint supports streaming later, reuse Handy live overlay pattern. For now mark disabled/experimental, not required for batch ASR.|H:src/overlay/RecordingOverlay.tsx; H:src-tauri/src/managers/transcription.rs|V1,V3
T17|x|Mic level events|Emit normalized level buckets while recording so overlay waveform responds. Keep separate from audio saved for ASR.|H:src-tauri/src/audio_toolkit/audio/visualizer.rs; H:src-tauri/src/managers/audio.rs|V3,V12
T18|x|Tray icon states|Idle, recording, transcribing/processing, error optional. Theme-aware icons. Menu changes while busy.|H:src-tauri/src/tray.rs|V3,V4,V13
T19|x|Tray menu actions|Settings, copy last transcript, cancel while busy, check endpoint/test connection, open logs/config, quit. Exclude Handy model submenu/unload.|H:src-tauri/src/tray.rs|V1,V5,V6,V10
T20|x|Shortcut coordinator|Serialize shortcut, CLI, tray, and signal events through one coordinator. Support toggle and push-to-talk with debounce/release grace.|H:src-tauri/src/transcription_coordinator.rs|V3,V4,V5,V12
T21|x|Shortcut engines|Keep GNOME custom shortcut/IPC path; add UI-editable shortcuts where platform permits. Evaluate HandyKeys/Tauri approach only if dependency cost acceptable.|H:src-tauri/src/shortcut/mod.rs; H:src-tauri/src/shortcut/handy_keys.rs; H:src-tauri/src/shortcut/tauri_impl.rs|V3,V9,V11
T22|x|Cancel operation|Add cancel command from overlay, tray, CLI. Stop recorder if recording; ignore late ASR result if upload already in flight.|H:src-tauri/src/commands/mod.rs; H:src-tauri/src/actions.rs|V5,V6
T23|x|CLI parity|Add `--toggle-transcription`, `--cancel`, `--start-hidden`, `--debug` aliases while preserving existing subcommands. Optional `--no-tray` for UI shell.|H:src-tauri/src/cli.rs|V3,V5,I.cli
T24|x|Linux paste fallback|Improve Linux adapter: `wl-copy`/`wl-paste`, `wtype`, `dotool`, `ydotool`, `xdotool`, configurable direct vs clipboard paste.|H:src-tauri/src/clipboard.rs|V6,V7,V9
T25|x|Windows paste polish|Keep native clipboard/SendInput; add paste method options, delays, restore behavior, permission/errors surfaced in UI.|H:src-tauri/src/clipboard.rs; H:src-tauri/src/commands/audio.rs|V6,V7,V9
T26|x|Audio device selection|List microphones, persist selected mic, handle missing selected device, optional always-on mic mode only if latency benefit measured.|H:src-tauri/src/commands/audio.rs; H:src/components/settings/MicrophoneSelector.tsx; H:src/components/settings/AlwaysOnMicrophone.tsx|V3,V11,V12
T27|x|Output device and sounds|Audio feedback start/stop/error sounds, output device selector, volume, sound themes/custom sounds optional.|H:src-tauri/src/audio_feedback.rs; H:src/components/settings/AudioFeedback.tsx; H:src/components/settings/SoundPicker.tsx|V3,V11
T28|x|Mute while recording|Temporarily mute output while recording, restore after stop/cancel/error.|H:src/components/settings/MuteWhileRecording.tsx; H:src-tauri/src/managers/audio.rs|V5,V12
T29|x|VAD/silence trim optional|Use VAD to trim silence or detect empty clips before upload, but do not block raw remote ASR mode. Config toggle.|H:src-tauri/src/audio_toolkit/vad/*; H:src/components/settings/VoiceActivityDetection.tsx|V1,V3,V11
T30|x|Extra recording buffer|Configurable pre/post buffer to avoid clipped first/last words. Useful with push-to-talk and remote batch upload.|H:src/components/settings/debug/RecordingBuffer.tsx; H:src-tauri/src/managers/audio.rs|V3,V12
T31|x|Post-processing optional|Add LLM post-processing only behind explicit toggle. Providers can reuse OpenAI-compatible chat completion config. Never confuse with ASR endpoint token.|H:src/components/settings/post-processing/*; H:src-tauri/src/actions.rs; H:src-tauri/src/llm_client.rs|V1,V2,V11
T32|x|Endpoint test and diagnostics|Add test recording/test upload and display HTTP status/sanitized body. Redact token. Store last latency metrics.|H:src/components/settings/debug/LiveLogViewer.tsx; current src/core/asr.rs|V2,V3,V11
T33|x|Error toasts|Show user-friendly errors for mic denied, no input device, paste failed, ASR failed, endpoint config missing. Raw details in logs only when safe.|H:src/App.tsx|V2,V6,V11
T34|x|Logging|Structured logs with levels, log directory, live viewer. Include record duration, WAV write, upload/response, paste. No token/body leak.|H:src/components/settings/debug/LiveLogViewer.tsx; H:src-tauri/src/lib.rs|V2,V12
T35|x|i18n foundation|Add app language selector and translation files after UI stabilizes. Tray/menu strings included. Start with English.|H:src/i18n/*; H:src/components/settings/AppLanguageSelector.tsx; H:src-tauri/src/tray_i18n.rs|V11
T36|x|Theme selector|System/light/dark theme. Keep restrained work-focused UI. Theme applies to window, overlay, tray icons.|H:src/components/settings/ThemeSelector.tsx; H:src/styles/theme.css; H:src-tauri/src/tray.rs|V11
T37|x|Update checks and whats-new optional|Add only when release/update channel exists. Settings: update checks, show whats-new, modal release notes.|H:src/components/update-checker/*; H:src/components/whats-new/*|V11
T38|x|Portable mode optional|Support portable config/data next to executable only if installer need appears.|H:src-tauri/src/portable.rs|V8
T39|x|Installer integration|Update Linux and Windows installers for UI shell, autostart, tray, dependencies, config migration.|H:BUILD.md; H:src-tauri/tauri.conf.json; H:src-tauri/nsis/installer.nsi|V8,V9
T40|x|Remove local-model features from scope|Document exclusions: model download/delete/select, GPU accelerator, model unload, local headless transcription, local streaming internals. Replace with remote endpoint equivalents where needed.|H:src-tauri/src/managers/model.rs; H:src-tauri/src/commands/models.rs; H:src/components/settings/models/*|V1,V13

T41|x|Settings control inventory gate|Encode `I.settings.*` inventory in tests. Require each control state `working` or `unavailable`; delete contract-only tests that pass on string presence.|H:src/components/Sidebar.tsx|V15,V22
T42|x|Native settings shell|Replace localhost/browser panel with single-instance Tauri 2 settings window; tray/CLI focus existing window; preserve Rust config/core APIs.|H:src/App.tsx; H:src-tauri/src/lib.rs|V14,V16,I.shell
T43|x|Settings API and live reload|Add typed get/save commands, validation, atomic file write, daemon update channel, per-field apply/restart result, rollback on rejection. Cover every field in `I.settings.*`.|H:src-tauri/src/settings.rs; H:src-tauri/src/commands/mod.rs|V8,V15,V17,V22
T44|x|Shortcut runtime|Default `behavior.shortcut` to `Ctrl+Space`; parse/register toggle, push-to-talk press/release, cancel, and debug shortcuts; live re-register with conflict rollback.|H:src-tauri/src/shortcut/mod.rs; H:src-tauri/src/shortcut/tauri_impl.rs|V3,V5,V18
T45|x|Input device settings|Enumerate CPAL microphones, mark default, persist `audio.selected_microphone`, instantiate recorder with selection, refresh list, and visibly fall back if missing. Implement or remove `audio.always_on_microphone` based measured latency value.|H:src/components/settings/MicrophoneSelector.tsx; H:src/components/settings/AlwaysOnMicrophone.tsx|V15,V19,V20
T46|x|ASR encoding settings|Render `audio.sample_rate_hz=16000` and `audio.channels=1`; choose fixed read-only encoding or implement validated resample/channel targets. Remove editable values that recorder ignores.|H:src-tauri/src/commands/audio.rs|V15,V20
T47|x|Mute lifecycle|Implement `audio.mute_while_recording` through platform audio-session adapter; always restore on stop, cancel, recorder error, ASR error, paste error, and shutdown.|H:src/components/settings/MuteWhileRecording.tsx|V5,V15,V20
T48|x|Audio feedback|Enumerate output devices; implement start/stop/error playback, `audio.selected_output_device`, clamped `audio.audio_feedback_volume`, default/minimal/custom `audio.sound_theme`, and custom sound paths; disable dependent controls when feedback off.|H:src-tauri/src/audio_feedback.rs; H:src/components/settings/AudioFeedback.tsx; H:src/components/settings/SoundPicker.tsx|V15,V19,V20
T49|x|App lifecycle settings|Implement `app.start_hidden`, `app.autostart`, `app.show_tray_icon`, and safe recovery when tray hidden; installer and Tauri autostart state must agree.|H:src/components/settings/StartHidden.tsx; H:src/components/settings/AutostartToggle.tsx; H:src/components/settings/ShowTrayIcon.tsx|V15,V16,V17
T50|x|Overlay settings|Connect events to real overlay window; implement `app.show_overlay`, `overlay_position`, `overlay_style`, experimental availability, live mic levels, cancel; position/style changes apply live.|H:src/overlay/RecordingOverlay.tsx; H:src/overlay/RecordingOverlay.css|V3,V5,V15,V17
T51|.|Language, theme, updates|Implement supported locale catalog for `ui.language`; apply `ui.theme` system/light/dark to settings/overlay/tray assets. Hide update controls until release service exists, then implement `updates.check_for_updates`, `updates.show_whats_new`, and `updates.release_channel`.|H:src/i18n/*; H:src/components/settings/ThemeSelector.tsx; H:src/components/update-checker/*|V15,V17
T52|.|Portable mode|Implement `portable.enabled/data_dir` before config resolution with migration/secret constraints, or remove control from normal builds. Mode change is restart-required and never strands config.|H:src-tauri/src/portable.rs|V8,V15,V17
T53|.|Output completion|Keep working paste/clipboard/delays; wire `output.typing_tool`; implement auto-submit and external script execution with path/exit validation. Remove legacy `behavior.restore_clipboard/clipboard_restore_delay_ms` UI or map one canonical source.|H:src/components/settings/PasteMethod.tsx; H:src/components/settings/TypingTool.tsx; H:src/components/settings/AutoSubmit.tsx; H:src-tauri/src/clipboard.rs|V6,V7,V9,V15
T54|.|Text processing completion|Implement UI persistence/runtime for `custom_words`, fuzzy threshold, translate hint; define deterministic correction/translation semantics. Keep filler filtering/trailing space and test interactions/order.|H:src/components/settings/CustomWords.tsx; H:src-tauri/src/audio_toolkit/text.rs|V1,V10,V15
T55|.|VAD and recording buffers|Call VAD/silence trim before upload; implement empty/trim behavior. Apply recording/pre/post buffers to capture lifecycle with bounded memory; consolidate duplicate buffer fields.|H:src-tauri/src/audio_toolkit/vad/*; H:src/components/settings/VoiceActivityDetection.tsx; H:src/components/settings/debug/RecordingBuffer.tsx|V1,V3,V15
T56|.|Post-processing|Implement optional OpenAI-compatible chat path for all `post_processing.*` fields, separate secret storage/token, timeout, cancellation, error fallback, and UI status; otherwise hide entire section.|H:src/components/settings/post-processing/*; H:src-tauri/src/llm_client.rs|V1,V2,V5,V15
T57|.|History subsystem|Persist completed/failed transcripts with endpoint/model metadata; implement `history.enabled/limit/retention/recordings_dir`, retention cleanup, open folder, delete/retry/save actions, and Copy Last Transcript.|H:src-tauri/src/managers/history.rs; H:src/components/settings/history/HistorySettings.tsx|V6,V10,V15,V21
T58|.|Logging and diagnostics|Add structured rotating file logs honoring level/directory; implement live viewer, open logs/app-data, latency/status/sanitized-body metrics, and define/remove debug shortcut/paste-delay duplication. Never log tokens/transcript body by default.|H:src/components/settings/debug/*; H:src-tauri/src/commands/mod.rs|V2,V15,V21
T59|.|ASR token and endpoint test|Show actual token source/status; write secrets to Credential Manager/Secret Service, plaintext only explicit fallback. Test endpoint with configured URL/model/auth/timeout and show latency/status/sanitized response without recording.|H:src/components/settings/general/ModelSettingsCard.tsx; H:src/components/settings/debug/LiveLogViewer.tsx|V1,V2,V15,V21
T60|.|About and paths|Render version/license/repository/config/app-data/log paths and acknowledgement as read-only values/actions; validate `ipc.socket_path`, platform applicability, and restart requirement.|H:src/components/settings/about/AboutSettings.tsx|V13,V15,V17,V21
T61|.|Tray action/state completion|Connect dynamic menu model; implement Copy Last Transcript, Test Endpoint, Open Logs; enable Cancel only while active; reflect recording/processing/error icon state; remove placeholder message.|H:src-tauri/src/tray.rs|V3,V4,V15,V21
T62|.|Installer and process lifecycle|Package Tauri/WebView prerequisites, stop/upgrade running binary safely, enforce one daemon/settings instance, preserve config, create correct autostart entry, and test install-upgrade-start path.|H:BUILD.md; H:src-tauri/tauri.conf.json|V8,V16,V17
T63|.|Settings end-to-end matrix|Windows UI automation: open from tray, inspect defaults/options, modify each working control, assert file and runtime, exercise every action/error. Linux adapter tests plus documented manual GNOME pass.|H:src/App.tsx|V12,V15,V16,V17,V18,V19,V20,V21,V22
T64|.|Onboarding state|Implement first-run permission/endpoint/microphone/shortcut flow backed by `onboarding.completed` and `onboarding.show_permission_repair`, or remove both fields until onboarding ships.|H:src/App.tsx; H:src/components/onboarding/*|V8,V15,V22

§B
id|date|cause|fix
B1|2026-07-10|Settings work built config/UI contract but no user-invokable settings surface existed|V14
B2|2026-07-10|Settings tasks marked complete from schema/model/string tests without render-persist-runtime/action proof, exposing dead controls and placeholder tray actions|V15,V16,V17,V18,V19,V20,V21,V22
B3|2026-07-10|Tauri generated context required missing default `icons/icon.png`, breaking desktop-shell builds on Linux and Windows|V23
B4|2026-07-10|V23 asset test checked Linux icon only; Tauri Windows resource build required missing `icons/icon.ico`|V23
B5|2026-07-11|Shortcut rollback test fake registrar removed a parsed key from a string-keyed set|—
B6|2026-07-11|Legacy-default test retained pre-T44 shortcut expectation after default changed to `Ctrl+Space`|—
B7|2026-07-11|Desktop-shell verification host lacks `dbus-1.pc`, so `libdbus-sys` cannot compile|—
B8|2026-07-11|Legacy settings test retained restart-required overlay disposition after T50 added live overlay polling|V17
B9|2026-07-11|Static shell test matched exact two-command handler text after T50 added overlay cancel command|V21
B10|2026-07-11|Desktop-shell temporary-checkout tests compiled app code but rust-lld bus-error blocked test binary linking|V23
