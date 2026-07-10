# App Shell Architecture

OpenVoiceDesktop will use a Tauri/React shell around the existing Rust core.
The shell owns windows, tray, overlay, settings UI, and user-facing event
delivery. The Rust core owns recording, remote ASR, state transitions, paste,
and platform adapter boundaries.

Handy references:

- `H:src/App.tsx`: React app listens to backend events and displays settings,
  onboarding, and errors.
- `H:src-tauri/src/lib.rs`: Tauri app initializes managers, tray, overlay, and
  frontend event emission.

## Boundaries

- `src/core/*` stays platform-independent and testable without Tauri.
- `src/core/events.rs` defines the app-shell event contract.
- `src/core/controller.rs` emits events before slow work starts, so UI/tray can
  react before audio write, upload, or paste.
- Future Tauri commands and tray/overlay handlers should subscribe through an
  `AppEventSink` implementation instead of duplicating controller state.
- Current daemon and CLI keep working through `NoopEventSink`.

## Event Contract

The controller emits these events:

- `Recording`: accepted toggle entered recording before recorder startup.
- `Processing { WritingAudio }`: stop accepted before recorder stop and WAV
  write.
- `Processing { Uploading }`: WAV ready, upload/transcription about to start.
- `Processing { InsertingText }`: transcript ready, paste about to start.
- `Inserted`: transcript inserted successfully.
- `Empty`: ASR returned no usable transcript.
- `Busy { state }`: second toggle arrived while writing, uploading, or
  inserting.
- `Error { message }`: operation failed; message is for logs/toasts, not secrets.
- `Idle`: controller returned to idle and overlay can hide.

## Remote ASR

App-shell work must not add local model inference. The only transcription path
remains `ReqwestAsrClient`, which builds requests for
`{base_url.trim_end_matches('/')}/v1/audio/transcriptions`.

## V1 Exclusions

OpenVoiceDesktop V1 excludes Handy local-model features:

- model download/delete/select
- GPU accelerator selection
- model unload controls
- local headless transcription
- local streaming internals

Use remote endpoint equivalents instead: ASR `base_url`, ASR `model`, endpoint
test/diagnostics, and disabled experimental live overlay until an endpoint
streaming contract exists.
