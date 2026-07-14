# Repository Guidelines

## Project Structure & Module Organization

OpenVoiceDesktop is a Rust desktop dictation client. Shared logic lives in `src/core/`: ASR upload, audio WAV writing, config loading, controller state, recorder traits, and platform traits. Platform adapters live in `src/platform/`, with Linux GNOME Wayland support in `linux.rs` and Windows tray/hotkey behavior in `windows.rs`. Entry points are `src/main.rs`, `src/daemon.rs`, and `src/ipc.rs`. Integration tests live in `tests/`. Runtime assets are in `assets/`, install helpers in `scripts/`, systemd unit files in `systemd/`, and user-facing docs in `docs/`.

## Build, Test, and Development Commands

- `cargo test`: run all Rust tests; CI runs this on Ubuntu and Windows.
- `cargo build --release`: build optimized binary at `target/release/openvoice-desktop`.
- `cargo run -- config-path`: print default config path for local validation.
- `cargo run -- daemon`: run resident daemon locally after creating config.
- `./scripts/install.sh --install-deps --install-rust --start`: Linux installer flow.
- `powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1 -Start`: Windows installer flow.

Linux builds need ALSA/pkg-config dependencies, for example `sudo apt install libasound2-dev pkg-config`.

## Coding Style & Naming Conventions

Use Rust 2021 idioms and keep platform-specific code behind `cfg` gates or platform modules. Format with `cargo fmt` before submitting. Prefer clear module-level responsibilities over cross-platform conditionals in core code. Use `snake_case` for functions, modules, and test names; `PascalCase` for types; `SCREAMING_SNAKE_CASE` for constants. Keep bearer tokens and secrets out of logs.

## Testing Guidelines

Tests are standard Cargo integration tests in `tests/*_tests.rs`. Add focused tests near the behavior changed, especially for config parsing, ASR response handling, audio output, controller transitions, installer scripts, and Windows tray feedback. Run `cargo test` before opening a PR. When touching OS-specific behavior, note which platform was manually checked if local automation cannot cover it.

## Commit & Pull Request Guidelines

History mostly uses Conventional Commit prefixes such as `feat:`, `fix:`, and `chore:`; keep subjects short and imperative. Some spec/task commits use labels like `T1:`. After implementing a `SPEC.md` task or full spec, commit the completed changes and push them to the active branch. PRs should include a concise problem/solution summary, linked issue or task when available, test results, and screenshots or short recordings for tray, popup, or installer UI changes. Call out config or credential-store changes explicitly.

Always commit and push after each change.

## Remote CI Waiting

After triggering a long-running remote CI job or build, confirm it started, then detach instead of polling until completion. Provide the run link, exact commands or UI steps for checking status, expected success/failure signals, and instructions for waking the agent after completion. Resume inspection or fixes when the user reports completion or explicitly asks for another status check. Only keep waiting in the same turn when the user explicitly asks to babysit or wait for the remote job.

## Security & Configuration Tips

Use `config.example.toml` as template. Prefer `OPENVOICE_DESKTOP_TOKEN`, Secret Service, or Windows Credential Manager over plaintext `asr.bearer_token`. If plaintext token config is unavoidable on Unix, keep file permissions private with `chmod 600`.
