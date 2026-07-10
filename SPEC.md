# OpenVoiceDesktop Handy-Style UX Spec

§G
G1: Build OpenVoiceDesktop into Handy-like desktop dictation app: fast visible feedback, polished settings, tray, overlay, configurable behavior.
G2: Keep primary transcription path remote OpenAI-compatible ASR: record audio, send `{base_url}/v1/audio/transcriptions`, paste returned text.
G3: Use Handy as UX/reference code, not fork base. Handy ref: `https://github.com/cjpais/Handy`, commit `1c4f21acd2bab81f75a0cd7556cd3bb7399ffd9a`, release `v0.9.1`.

§C
C1: No local offline model manager in v1. Do not add `transcribe-cpp`, `transcribe-rs`, ONNX, Vulkan, Metal, model downloads, or model unload controls unless later product decision changes.
C2: Remote ASR endpoint config stays first-class: `base_url`, `model`, token source, language, prompt, timeout/retry.
C3: UI may use Tauri/React if chosen, but core ASR/record/paste stays testable as Rust library.
C4: GNOME Wayland and Windows 10/11 remain target platforms. macOS support can be designed but not required by current README.
C5: Secrets never logged. Token UI must store through platform secret store when possible; config plaintext token remains fallback only.
C6: Handy code references use `H:<path>` relative to Handy repo at pinned commit. New agent should clone with: `git clone https://github.com/cjpais/Handy /tmp/Handy-review && git -C /tmp/Handy-review checkout 1c4f21acd2bab81f75a0cd7556cd3bb7399ffd9a`.
C7: Preserve low dependency surface unless feature requires app shell. Avoid importing Handy local inference dependencies.
C8: UX must feel immediate even when remote ASR latency exists: state changes visible before network call starts.

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
T16|.|Live overlay optional|If remote endpoint supports streaming later, reuse Handy live overlay pattern. For now mark disabled/experimental, not required for batch ASR.|H:src/overlay/RecordingOverlay.tsx; H:src-tauri/src/managers/transcription.rs|V1,V3
T17|.|Mic level events|Emit normalized level buckets while recording so overlay waveform responds. Keep separate from audio saved for ASR.|H:src-tauri/src/audio_toolkit/audio/visualizer.rs; H:src-tauri/src/managers/audio.rs|V3,V12
T18|.|Tray icon states|Idle, recording, transcribing/processing, error optional. Theme-aware icons. Menu changes while busy.|H:src-tauri/src/tray.rs|V3,V4,V13
T19|.|Tray menu actions|Settings, copy last transcript, cancel while busy, check endpoint/test connection, open logs/config, quit. Exclude Handy model submenu/unload.|H:src-tauri/src/tray.rs|V1,V5,V6,V10
T20|.|Shortcut coordinator|Serialize shortcut, CLI, tray, and signal events through one coordinator. Support toggle and push-to-talk with debounce/release grace.|H:src-tauri/src/transcription_coordinator.rs|V3,V4,V5,V12
T21|.|Shortcut engines|Keep GNOME custom shortcut/IPC path; add UI-editable shortcuts where platform permits. Evaluate HandyKeys/Tauri approach only if dependency cost acceptable.|H:src-tauri/src/shortcut/mod.rs; H:src-tauri/src/shortcut/handy_keys.rs; H:src-tauri/src/shortcut/tauri_impl.rs|V3,V9,V11
T22|.|Cancel operation|Add cancel command from overlay, tray, CLI. Stop recorder if recording; ignore late ASR result if upload already in flight.|H:src-tauri/src/commands/mod.rs; H:src-tauri/src/actions.rs|V5,V6
T23|.|CLI parity|Add `--toggle-transcription`, `--cancel`, `--start-hidden`, `--debug` aliases while preserving existing subcommands. Optional `--no-tray` for UI shell.|H:src-tauri/src/cli.rs|V3,V5,I.cli
T24|.|Linux paste fallback|Improve Linux adapter: `wl-copy`/`wl-paste`, `wtype`, `dotool`, `ydotool`, `xdotool`, configurable direct vs clipboard paste.|H:src-tauri/src/clipboard.rs|V6,V7,V9
T25|.|Windows paste polish|Keep native clipboard/SendInput; add paste method options, delays, restore behavior, permission/errors surfaced in UI.|H:src-tauri/src/clipboard.rs; H:src-tauri/src/commands/audio.rs|V6,V7,V9
T26|.|Audio device selection|List microphones, persist selected mic, handle missing selected device, optional always-on mic mode only if latency benefit measured.|H:src-tauri/src/commands/audio.rs; H:src/components/settings/MicrophoneSelector.tsx; H:src/components/settings/AlwaysOnMicrophone.tsx|V3,V11,V12
T27|.|Output device and sounds|Audio feedback start/stop/error sounds, output device selector, volume, sound themes/custom sounds optional.|H:src-tauri/src/audio_feedback.rs; H:src/components/settings/AudioFeedback.tsx; H:src/components/settings/SoundPicker.tsx|V3,V11
T28|.|Mute while recording|Temporarily mute output while recording, restore after stop/cancel/error.|H:src/components/settings/MuteWhileRecording.tsx; H:src-tauri/src/managers/audio.rs|V5,V12
T29|.|VAD/silence trim optional|Use VAD to trim silence or detect empty clips before upload, but do not block raw remote ASR mode. Config toggle.|H:src-tauri/src/audio_toolkit/vad/*; H:src/components/settings/VoiceActivityDetection.tsx|V1,V3,V11
T30|.|Extra recording buffer|Configurable pre/post buffer to avoid clipped first/last words. Useful with push-to-talk and remote batch upload.|H:src/components/settings/debug/RecordingBuffer.tsx; H:src-tauri/src/managers/audio.rs|V3,V12
T31|.|Post-processing optional|Add LLM post-processing only behind explicit toggle. Providers can reuse OpenAI-compatible chat completion config. Never confuse with ASR endpoint token.|H:src/components/settings/post-processing/*; H:src-tauri/src/actions.rs; H:src-tauri/src/llm_client.rs|V1,V2,V11
T32|.|Endpoint test and diagnostics|Add test recording/test upload and display HTTP status/sanitized body. Redact token. Store last latency metrics.|H:src/components/settings/debug/LiveLogViewer.tsx; current src/core/asr.rs|V2,V3,V11
T33|.|Error toasts|Show user-friendly errors for mic denied, no input device, paste failed, ASR failed, endpoint config missing. Raw details in logs only when safe.|H:src/App.tsx|V2,V6,V11
T34|.|Logging|Structured logs with levels, log directory, live viewer. Include record duration, WAV write, upload/response, paste. No token/body leak.|H:src/components/settings/debug/LiveLogViewer.tsx; H:src-tauri/src/lib.rs|V2,V12
T35|.|i18n foundation|Add app language selector and translation files after UI stabilizes. Tray/menu strings included. Start with English.|H:src/i18n/*; H:src/components/settings/AppLanguageSelector.tsx; H:src-tauri/src/tray_i18n.rs|V11
T36|.|Theme selector|System/light/dark theme. Keep restrained work-focused UI. Theme applies to window, overlay, tray icons.|H:src/components/settings/ThemeSelector.tsx; H:src/styles/theme.css; H:src-tauri/src/tray.rs|V11
T37|.|Update checks and whats-new optional|Add only when release/update channel exists. Settings: update checks, show whats-new, modal release notes.|H:src/components/update-checker/*; H:src/components/whats-new/*|V11
T38|.|Portable mode optional|Support portable config/data next to executable only if installer need appears.|H:src-tauri/src/portable.rs|V8
T39|.|Installer integration|Update Linux and Windows installers for UI shell, autostart, tray, dependencies, config migration.|H:BUILD.md; H:src-tauri/tauri.conf.json; H:src-tauri/nsis/installer.nsi|V8,V9
T40|.|Remove local-model features from scope|Document exclusions: model download/delete/select, GPU accelerator, model unload, local headless transcription, local streaming internals. Replace with remote endpoint equivalents where needed.|H:src-tauri/src/managers/model.rs; H:src-tauri/src/commands/models.rs; H:src/components/settings/models/*|V1,V13

§B
id|date|cause|fix
