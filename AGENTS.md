# Repository Guidelines

## Project Structure & Module Organization

OpenVoiceDesktop is a Rust desktop dictation client. Shared logic lives in `src/core/`: ASR upload, audio WAV writing, config loading, controller state, recorder traits, and platform traits. Platform adapters live in `src/platform/`, with Linux GNOME Wayland support in `linux.rs` and Windows tray/hotkey behavior in `windows.rs`. Entry points are `src/main.rs`, `src/daemon.rs`, and `src/ipc.rs`. Integration tests live in `tests/`. Runtime assets are in `assets/`, install helpers in `scripts/`, systemd unit files in `systemd/`, and user-facing docs in `docs/`.

## Build, Test, and Development Commands

- `cargo test`: run all Rust tests; CI runs this on Ubuntu and Windows.
- `cargo build --release`: build optimized binary at `target/release/openvoice-desktop`.
- `cargo run -- config-path`: print default config path for local validation.
- `cargo run -- daemon`: run resident daemon locally after creating config.
- `./scripts/install.sh --install-deps --install-rust --start`: Linux source installer flow.
- `./scripts/setup-linux-build.sh` then `./scripts/build-linux-local.sh`: build, verify, and checksum the release `.deb` under `target/release/bundle/deb/`.
- `./scripts/test-deb-install.sh <deb>`: root-only Linux install/upgrade/remove smoke.
- `./scripts/release-local.sh [--dry-run|--linux-only|--windows-only]`: build every release artifact on this host and upload it to the tagged GitHub release.
- `powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1 -Start`: Windows installer flow.

Linux builds need ALSA/pkg-config dependencies, for example `sudo apt install libasound2-dev pkg-config`. Packaging the `.deb` additionally needs the WebKitGTK/appindicator/rsvg development headers and the pinned `cargo-tauri`; `scripts/setup-linux-build.sh` installs both sets. Platform packaging notes live in `docs/linux.md` and `docs/windows.md`.

## Coding Style & Naming Conventions

Use Rust 2021 idioms and keep platform-specific code behind `cfg` gates or platform modules. Format with `cargo fmt` before submitting. Prefer clear module-level responsibilities over cross-platform conditionals in core code. Use `snake_case` for functions, modules, and test names; `PascalCase` for types; `SCREAMING_SNAKE_CASE` for constants. Keep bearer tokens and secrets out of logs.

## Testing Guidelines

Tests are standard Cargo integration tests in `tests/*_tests.rs`. They must be hermetic: anything the host owns rather than the fixture - notably `OPENVOICE_DESKTOP_TOKEN` and the OS secret store, which both outrank config in token resolution - has to be switched off with `set_ambient_token_lookup(false)` before the fixture is built, or the suite passes or fails depending on whose machine runs it. Add focused tests near the behavior changed, especially for config parsing, ASR response handling, audio output, controller transitions, installer scripts, and Windows tray feedback. Run `cargo test` before opening a PR. When touching OS-specific behavior, note which platform was manually checked if local automation cannot cover it.

## Commit & Pull Request Guidelines

History mostly uses Conventional Commit prefixes such as `feat:`, `fix:`, and `chore:`; keep subjects short and imperative. Some spec/task commits use labels like `T1:`. After implementing a `SPEC.md` task or full spec, commit the completed changes and push them to the active branch. PRs should include a concise problem/solution summary, linked issue or task when available, test results, and screenshots or short recordings for tray, popup, or installer UI changes. Call out config or credential-store changes explicitly.

Always commit and push after each change.

## No CI

This repository defines no CI workflows, and a push starts no remote build. Do not add GitHub Actions, GitLab CI, or any other remote pipeline. Every release artifact is compiled on the maintainer's Ubuntu host and uploaded from it by `scripts/release-local.sh`, so local `cargo test` and the packaging verification scripts are the only gates that exist. Windows-only regressions cannot be caught by a runner: keep tests over repository text ending-agnostic, and treat the cross-compiled Windows setup as unsmoked until someone runs `scripts/test-installer.ps1` on Windows.

Long local builds still deserve the detach discipline: confirm the build started, then report progress instead of blocking, and say which verification actually ran.

## Security & Configuration Tips

Use `config.example.toml` as template. Prefer `OPENVOICE_DESKTOP_TOKEN`, Secret Service, or Windows Credential Manager over plaintext `asr.bearer_token`. If plaintext token config is unavoidable on Unix, keep file permissions private with `chmod 600`.
