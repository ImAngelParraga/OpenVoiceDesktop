# Development and releases

## Build Manually

```bash
cargo test
cargo build --release --features desktop-shell
```

The binary is `target/release/openvoice-desktop`.

Build the Linux release `.deb` locally:

```bash
./scripts/setup-linux-build.sh  # one-time apt deps + pinned cargo-tauri
./scripts/build-linux-local.sh
```

The package and SHA-256 file are written under `target/release/bundle/deb/`.

Build the Windows NSIS installer from Ubuntu by cross-compiling:

```bash
./scripts/setup-windows-cross.sh  # one-time Docker image setup
./scripts/build-windows-local.sh
```

The installer and SHA-256 file are written under
`target/x86_64-pc-windows-msvc/release/bundle/nsis/`. Docker is the default, so
host package installation and administrator access are not required.

## Release

This repository runs **no CI**. Every published binary is compiled on the
maintainer's Ubuntu host and uploaded from it, so bump `version` in `Cargo.toml`
(and the package stanza in `Cargo.lock`), commit, push, then:

```bash
./scripts/release-local.sh            # both platforms
./scripts/release-local.sh --dry-run  # build and verify, publish nothing
```

The script refuses to publish from a dirty tree or a commit that is not on the
remote, runs the test suite, bundles the Debian package and the Windows setup,
checksums both, tags `v<version>`, and uploads the four assets to the GitHub
release. `--linux-only` and `--windows-only` release one platform at a time.

## Development Notes

The core modules are independent of GNOME and Windows:

- `core::config`: TOML config, defaults, token resolution, plaintext permission checks
- `core::recorder`: recorder trait plus CPAL microphone recorder
- `core::audio`: 16 kHz mono PCM WAV writing
- `core::asr`: OpenAI-compatible multipart ASR client and response parsing
- `core::controller`: idle to recording to transcribing to insertion state machine
- `core::platform`: clipboard/paste/notification trait and restore helper
- `platform::linux`: GNOME Wayland command-backed adapter
- `platform::windows`: Windows tray, hotkey, clipboard paste, and status popup adapter

Timing logs include record duration, WAV write time, upload/response time, and paste time. Bearer tokens are never printed.

V1 intentionally excludes local model download/delete/select, GPU accelerator,
model unload, local headless transcription, and local streaming internals.
Configure remote ASR endpoint settings instead.

## Public data check

Review tracked files, screenshots, and release assets for personal paths, private endpoints, credentials, and local usage data. The reusable [Portable ASR scanner](https://github.com/ImAngelParraga/portable-asr/blob/main/scripts/secret-scan.sh) can be run with this checkout as its working directory. It reports file/line locations without printing matches. It is a heuristic over current tracked files; Git history and binary formats require separate review.
