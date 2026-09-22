# Windows and settings review — 2026-09-22

Scope: static review of Windows daemon, native popup, settings shell, settings UI, configuration, and relevant tests. Reviewed on Ubuntu; no Windows executable was run. “Confirmed” means the code path establishes the behavior. “Verify on Windows” means the code shows a plausible failure path whose user-visible effect needs a native check.

Progress (2026-09-22): W01–W03 implemented and published in v0.1.22; native Windows behavior remains unverified. W04 works for the user after a manual restart, so its audio path was not changed. Save now reloads audio settings without a manual restart; native verification remains needed. W05–W09 are implemented for v0.1.23, with native Windows checks still outstanding. W12's transient executable-busy error has a bounded retry and regression test because it blocked the release gate. W10–W11 remain open.

Baseline: `node --check ui/settings.js` and `node --check ui/overlay.js` passed. `cargo test --locked` reported 139 passed and one failure: `external_script_receives_transcript_on_stdin` hit `Text file busy (os error 26)`. That test passed when rerun alone. Windows smoke coverage currently checks only that Settings opens and a second launch reuses its window.

## P0 — Windows behavior

### W01. Make Windows overlay settings control the visible overlay — confirmed

Evidence: [Windows daemon](../src/daemon.rs#L951) starts the native tray/popup but never starts the Tauri overlay shell. [Native popup placement](../src/platform/windows.rs#L859) always uses primary-screen bottom coordinates and fixed `180×34` size. Its show path does not consult `app.show_overlay`, `app.overlay_position`, or `app.overlay_style`. [Tauri overlay](../src/settings_shell.rs#L164) applies those fields only while a Settings shell happens to be running. The two surfaces can therefore disagree or appear together.

Task: choose one Windows recording overlay owner; apply show/hide, top/bottom, compact/expanded, theme, and monitor/DPI placement to that owner; avoid duplicate status windows. Acceptance: each overlay field changes the visible Windows overlay immediately after Save; hidden means no recording popup; Settings open/closed yields the same behavior; check secondary monitor and scale changes.

### W02. Honor Windows shortcut fields and push-to-talk — confirmed

Evidence: [hotkey registration](../src/platform/windows.rs#L457) hardcodes `Ctrl+Space` and `Escape`; only language shortcut is read from config, once at tray startup. [reload handling](../src/daemon.rs#L827) updates controller and tray visibility but never re-registers hotkeys. [inventory](../src/core/settings_inventory.rs#L107) marks shortcut and push-to-talk controls working. `behavior.push_to_talk` is not used by the Windows message loop; `debug.debug_shortcut` has no Windows registration/action. Unmodified `Escape` is registered globally and can interfere with other applications. [Save disposition](../src/core/settings_service.rs#L460) singles out only language shortcut as restart-required, while other shortcut changes are reported applied.

Task: register all supported bindings from saved config, handle press/release for push-to-talk, update bindings safely on Save, and report registration conflicts in Settings. If a field cannot work on Windows, disable it with an accurate reason. Acceptance: change toggle/cancel/language shortcut and use it without restart; push-to-talk stops on release; debug shortcut either acts or is unavailable; ordinary Escape remains usable in other apps unless user explicitly chooses that global binding.

### W03. Keep Windows cancel, status, and Save responsive during transcription — confirmed

Evidence: [controller worker](../src/daemon.rs#L729) receives one request at a time and awaits `controller.toggle()` to completion. Stopping a recording performs WAV writing, ASR upload, and insertion inside that call. Later Cancel, Status, and Reload requests wait behind it; the [5-second Save reload timeout](../src/settings_shell.rs#L305) can then fail and roll back a valid edit. The Tauri overlay also cannot poll progress during the request.

Task: publish controller state independently of long operations, make cancellation interrupt in-flight ASR/insertion before text is emitted, and defer or queue reload with a clear completion result. Acceptance: Cancel during a slow ASR request stops insertion promptly; Status returns during processing; Save during processing either applies after idle or clearly reports pending state without a false success or unnecessary rollback.

### W04. Repair Windows output reduction lifecycle — partly confirmed; verify device routing on Windows

Evidence: [controller stop path](../src/core/controller.rs#L239) restores output at [line 350](../src/core/controller.rs#L350), after ASR and paste; volume stays reduced/muted while processing. [Windows adjustment](../src/platform/windows.rs#L188) always changes the default `eMultimedia` endpoint at [line 320](../src/platform/windows.rs#L320), regardless of `audio.selected_output_device` or per-app output routing. The selected output field currently controls [feedback sounds](../src/core/audio.rs#L286) only. [Reduction formula](../src/core/platform.rs#L54) caps output at the configured absolute scalar; it does not reduce by a percentage of the current volume. Windows endpoint success and actual audible effect were not tested here.

Task: restore output as soon as recording capture stops; define whether reduced volume means absolute cap or relative attenuation and label it accordingly; target the playback endpoint users actually hear; handle output-device changes and failed restore visibly. Acceptance: recording at a known volume reduces audible playback, stopping immediately restores prior mute/volume before upload completes, and the test works with default and separately routed/headset output. Add native Windows checks around COM read/set/restore failures.

## P1 — Save and settings UI

### W05. Make Save's runtime result truthful — confirmed

Evidence: [save command](../src/settings_shell.rs#L259) treats a daemon reload response as proof of complete application, though Windows reload does not update popup theme or hotkeys. If daemon is absent, [startup](../src/settings_shell.rs#L319) discards `spawn_resident_daemon()` failure and returns `Deferred` without waiting for daemon readiness. [field results](../src/core/settings_service.rs#L450) classify most working fields as `Applied` even when unchanged or ignored by the current Windows runtime.

Task: track changed fields and actual runtime application per subsystem; wait for daemon readiness after startup; surface startup/reload failures. Acceptance: Save reports applied only for changed fields confirmed active, restart/pending only where required, and an absent daemon cannot produce a misleading applied result.

### W06. Replace permanent, oversized Save feedback — confirmed

Evidence: [Save handler](../ui/settings.js#L459) counts every working inventory field, including unchanged fields, then concatenates reasons for all unavailable fields. [success text](../ui/settings.js#L464) remains set indefinitely. [layout](../ui/styles.css#L50) puts unbounded bold green text in the same flex row as Save; status is also duplicated above the form.

Task: show a short, accurate changed-field summary; move detailed exceptions to a bounded expandable area; clear transient success feedback after a short interval or next edit while keeping failures accessible. Acceptance: no-change Save says “No changes”; one-field Save reports one field; message does not stretch the button row and disappears after a defined interval.

### W07. Wire theme and language selectors; finish UI translation — confirmed

Evidence: [select handler](../ui/settings.js#L228) only rerenders on three unrelated fields. `applyTheme()` and `applyLocale()` calls at [line 269](../ui/settings.js#L269) are in the generic input handler, which theme/language selects never reach. `applyLocale()` also sets page title to `OpenVoiceDesktop About` at [line 55](../ui/settings.js#L55). Translation covers navigation and Save, but control labels/descriptions and action/status text remain English.

Task: apply selected theme and locale on change and after Save/reload; keep Settings title accurate; either translate all user-facing text or label language choice as partial. Acceptance: changing each selector updates visible UI immediately, survives Save/reopen, and Settings title remains correct.

### W08. Fix field rendering, validation, and misleading labels — confirmed

Evidence: `debug.log_level` is declared a Select in [layout](../src/core/settings_ui.rs#L684) but absent from [select options](../ui/settings.js#L135), so it renders as free text. `audio.recording_output_volume` is declared a Slider but rendered as an unconstrained number; all numeric fields initially get `step="any"` at [line 243](../ui/settings.js#L243), including integer-only config values. Clearing an already populated optional `asr.language` yields `""`, not `null`, because null conversion at [line 265](../ui/settings.js#L265) only handles values that were already null; [ASR request builder](../src/core/asr.rs#L119) then includes that empty language. Microphone fallback note is appended to the output-device row at [line 200](../ui/settings.js#L200). `Start hidden` claims to suppress Settings at startup, but [default launch](../src/core/lifecycle.rs#L29) opens Settings for incomplete onboarding and otherwise starts background daemon regardless of this field; its only Windows use is hiding an already zero-sized tray host window. `Live log viewer` displays one snapshot, not a live stream.

Task: render controls from declared kinds with enum options and field-specific limits; normalize optional blanks; attach notes to matching field; rename/remove controls whose Windows behavior is absent or implement it. Acceptance: invalid numeric/enum values are blocked beside their fields before Save, clearing language omits ASR language, fallback notes appear once, and labels match observable behavior.

### W09. Refresh Windows audio-device inventory safely — confirmed

Evidence: [device enumeration cache](../src/core/settings_service.rs#L238) uses a process-lifetime `OnceLock`. This avoids an earlier WASAPI crash on repeated enumeration, but plug/unplug or default-device changes remain invisible until Settings restarts. A cached selected device can be missing from the dropdown while the model retains its name.

Task: add explicit safe refresh or a supervised enumeration path with bounded failure handling; preserve missing saved devices as visible options with fallback warnings. Acceptance: newly connected devices appear without restarting Settings and repeated refresh does not crash Windows audio enumeration.

## P2 — Stability and verification

### W10. Release COM objects on every Windows audio error — confirmed

Evidence: [manual COM setup](../src/platform/windows.rs#L305) uses `?` between `CoCreateInstance`, endpoint activation, and volume operations, but releases objects only on the success path at [line 367](../src/platform/windows.rs#L367). Failed calls can leak enumerator/device/endpoint references across attempts.

Task: use scoped COM ownership/RAII and verify every HRESULT path. Acceptance: injected failures release every acquired interface; repeated failed recording starts do not grow COM resources.

### W11. Expand Windows runtime smoke coverage — confirmed gap

Evidence: [Windows smoke script](../scripts/test-settings-ui.ps1#L1) checks window appearance and single-instance reuse only. [Windows output test](../tests/windows_feedback_tests.rs#L30) checks source-text markers, not audible endpoint behavior. [settings matrix](../tests/settings_e2e_tests.rs#L14) checks serialized fields and symbol presence, not browser interaction or native hotkeys.

Task: add a native Windows manual/automated matrix for Save reload, hotkey changes/conflicts, cancel during slow ASR, overlay controls, device changes, output reduction/restore, theme/locale, and error feedback. Keep Linux text tests ending-agnostic. Acceptance: each W01–W09 behavior has a Windows reproduction and pass/fail observation; published Windows setup remains explicitly unsmoked until that matrix runs.

### W12. Remove intermittent Linux external-script test failure — observed once

Evidence: full `cargo test --locked` failed at [output test](../tests/output_tests.rs#L39) with `starting external script ...: Text file busy (os error 26)` immediately after the fixture wrote and chmodded `capture.sh`; the isolated rerun passed. This is a flaky gate, not evidence of the Windows complaints.

Task: determine whether executable-file timing, filesystem behavior, or another test causes the race; make fixture execution deterministic and keep the transcript-on-stdin assertion. Acceptance: repeated full-suite runs pass on supported Linux build host.

## Suggested order

W03, W01, W02, W04, W05, W06, W07, W08, W09, W10, W11, W12. W03 and W05 share reload semantics; W01 and W02 explain most Windows controls that appear saved but do nothing.
