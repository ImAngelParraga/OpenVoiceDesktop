#!/usr/bin/env bash
set -euo pipefail

# Prepare an Ubuntu/Debian host to build the Linux desktop shell and bundle it
# into the release `.deb`. Mirrors scripts/setup-windows-cross.sh for the
# native Linux packaging path.

TAURI_CLI_VERSION="2.11.4"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "This setup script supports Ubuntu/Linux hosts only." >&2
  exit 1
fi

usage() {
  cat <<'USAGE'
Usage: ./scripts/setup-linux-build.sh [--skip-apt]

  --skip-apt   Do not touch system packages; only install the pinned Tauri CLI.
USAGE
}

SKIP_APT=0
for argument in "$@"; do
  case "$argument" in
    --skip-apt) SKIP_APT=1 ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
done

export PATH="$HOME/.cargo/bin:$PATH"

if [[ $SKIP_APT -eq 0 ]]; then
  if ! command -v apt-get >/dev/null 2>&1; then
    echo "apt-get not found; install the build dependencies listed in AGENTS.md manually." >&2
    exit 1
  fi

  if [[ $EUID -eq 0 ]]; then
    APT=(apt-get)
  elif command -v sudo >/dev/null 2>&1; then
    APT=(sudo apt-get)
  else
    echo "sudo not found; run this script as root for package installation." >&2
    exit 1
  fi

  "${APT[@]}" update
  "${APT[@]}" install -y \
    build-essential pkg-config \
    libasound2-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev \
    wl-clipboard wtype libsecret-tools libnotify-bin
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found. Install Rust first: ./scripts/install.sh --install-rust" >&2
  exit 1
fi

if command -v cargo-tauri >/dev/null 2>&1 && cargo-tauri --version | grep -Fq "$TAURI_CLI_VERSION"; then
  echo "cargo-tauri $TAURI_CLI_VERSION already installed"
else
  cargo install --locked --version "$TAURI_CLI_VERSION" tauri-cli
fi

echo
echo "Linux packaging toolchain ready."
echo "Run: ./scripts/build-linux-local.sh"
