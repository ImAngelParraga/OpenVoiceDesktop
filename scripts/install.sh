#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Install OpenVoiceDesktop from this source checkout.

Usage:
  ./scripts/install.sh [options]

Options:
  --install-deps   Install Linux packages needed to build/run, using apt, dnf, pacman, or zypper.
  --install-rust   Install Rust with rustup if cargo is missing.
  --start          Enable and start the user systemd daemon after install.
  --no-build       Skip cargo build and install an already-built target/release/openvoice-desktop.
  -h, --help       Show this help.

Recommended first install:
  ./scripts/install.sh --install-deps --install-rust --start
EOF
}

INSTALL_DEPS=0
INSTALL_RUST=0
START_SERVICE=0
BUILD=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --install-deps)
      INSTALL_DEPS=1
      ;;
    --install-rust)
      INSTALL_RUST=1
      ;;
    --start)
      START_SERVICE=1
      ;;
    --no-build)
      BUILD=0
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="${HOME}/.local/bin"
CONFIG_HOME="${XDG_CONFIG_HOME:-${HOME}/.config}"
CONFIG_DIR="${CONFIG_HOME}/openvoice-desktop"
SYSTEMD_USER_DIR="${CONFIG_HOME}/systemd/user"
SERVICE_PATH="${SYSTEMD_USER_DIR}/openvoice-desktop.service"

cd "${ROOT_DIR}"

need_cmd() {
  command -v "$1" >/dev/null 2>&1
}

install_deps() {
  if need_cmd apt-get; then
    sudo apt-get update
    sudo apt-get install -y build-essential pkg-config libasound2-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev wl-clipboard wtype libsecret-tools libnotify-bin
  elif need_cmd dnf; then
    sudo dnf install -y gcc gcc-c++ make pkgconf-pkg-config alsa-lib-devel webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel wl-clipboard wtype libsecret libnotify
  elif need_cmd pacman; then
    sudo pacman -Syu --needed base-devel pkgconf alsa-lib webkit2gtk-4.1 libappindicator-gtk3 librsvg wl-clipboard wtype libsecret libnotify
  elif need_cmd zypper; then
    sudo zypper install -y gcc gcc-c++ make pkgconf-pkg-config alsa-devel webkit2gtk3-devel libappindicator3-1 librsvg-devel wl-clipboard wtype libsecret-tools libnotify-tools
  else
    cat >&2 <<'EOF'
Could not detect apt, dnf, pacman, or zypper.
Install these manually:
  - Rust build tools: gcc/make/pkg-config
  - ALSA development headers
  - WebKitGTK 4.1, appindicator, and librsvg development headers
  - wl-clipboard
  - wtype
  - optional fallback typing tools: dotool, ydotool, or xdotool
  - libsecret tools / secret-tool
  - notify-send
EOF
    exit 1
  fi
}

install_rust() {
  if need_cmd cargo; then
    return
  fi
  if ! need_cmd curl; then
    echo "curl is required to install rustup. Install curl, then rerun this script." >&2
    exit 1
  fi
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  source "${HOME}/.cargo/env"
}

if [[ "${INSTALL_DEPS}" -eq 1 ]]; then
  install_deps
fi

if [[ "${INSTALL_RUST}" -eq 1 ]]; then
  install_rust
fi

if ! need_cmd cargo && [[ "${BUILD}" -eq 1 ]]; then
  cat >&2 <<'EOF'
cargo is not installed.

Install Rust, then rerun:
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ./scripts/install.sh --start

Or let this installer do it:
  ./scripts/install.sh --install-rust --start
EOF
  exit 1
fi

if [[ "${BUILD}" -eq 1 ]]; then
  cargo build --release --features desktop-shell
fi

if [[ ! -x target/release/openvoice-desktop ]]; then
  echo "Missing target/release/openvoice-desktop. Build failed or --no-build was used before building." >&2
  exit 1
fi

mkdir -p "${BIN_DIR}" "${CONFIG_DIR}" "${SYSTEMD_USER_DIR}"
install -m 0755 target/release/openvoice-desktop "${BIN_DIR}/openvoice-desktop"

if [[ ! -f "${CONFIG_DIR}/config.toml" ]]; then
  install -m 0600 config.example.toml "${CONFIG_DIR}/config.toml"
  echo "Created ${CONFIG_DIR}/config.toml"
else
  echo "Keeping existing ${CONFIG_DIR}/config.toml"
fi

cat >"${SERVICE_PATH}" <<EOF
[Unit]
Description=OpenVoiceDesktop dictation daemon
After=graphical-session.target

[Service]
Type=simple
ExecStart=${BIN_DIR}/openvoice-desktop daemon
Restart=on-failure
RestartSec=2

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload

if [[ "${START_SERVICE}" -eq 1 ]]; then
  systemctl --user enable --now openvoice-desktop.service
fi

cat <<EOF

OpenVoiceDesktop installed.

Binary:
  ${BIN_DIR}/openvoice-desktop

Config:
  ${CONFIG_DIR}/config.toml

User service:
  ${SERVICE_PATH}

Next steps:
  1. Edit ${CONFIG_DIR}/config.toml and set your ASR base_url/model.
  2. Store your token:
     secret-tool store --label="OpenVoiceDesktop ASR token" service openvoice-desktop account asr-token
  3. Start the daemon if you did not pass --start:
     systemctl --user enable --now openvoice-desktop.service
  4. In GNOME Settings, bind Ctrl+Space to:
     ${BIN_DIR}/openvoice-desktop toggle
  5. If wtype cannot paste in your session, install dotool, ydotool, or xdotool for fallback paste.

Check logs:
  journalctl --user -u openvoice-desktop.service -f
EOF
