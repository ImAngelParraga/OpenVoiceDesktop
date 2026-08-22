#!/usr/bin/env bash
set -euo pipefail

# Install/upgrade/remove smoke for the release Debian package. Mirrors
# scripts/test-installer.ps1. Needs root because dpkg writes under /usr.

usage() {
  echo "Usage: $0 <path-to-openvoice-desktop.deb>" >&2
}

if [[ $# -ne 1 ]]; then
  usage
  exit 2
fi

PACKAGE="$1"
if [[ ! -f "$PACKAGE" ]]; then
  echo "package not found: $PACKAGE" >&2
  exit 2
fi

if [[ $EUID -eq 0 ]]; then
  APT=(apt-get)
elif command -v sudo >/dev/null 2>&1; then
  APT=(sudo apt-get)
else
  echo "sudo not found; run this smoke test as root." >&2
  exit 1
fi

BINARY="/usr/bin/openvoice-desktop"
UNIT="/usr/lib/systemd/user/openvoice-desktop.service"
DESKTOP_ENTRY="/usr/share/applications/OpenVoiceDesktop.desktop"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/openvoice-desktop"
CONFIG_PATH="$CONFIG_DIR/config.toml"

install_package() {
  DEBIAN_FRONTEND=noninteractive "${APT[@]}" install -y "$@" "$(readlink -f "$PACKAGE")"
}

install_package
[[ -x "$BINARY" ]] || { echo "installed executable not found at $BINARY" >&2; exit 1; }
[[ -f "$UNIT" ]] || { echo "systemd user unit was not installed" >&2; exit 1; }
[[ -f "$DESKTOP_ENTRY" ]] || { echo "desktop entry was not created" >&2; exit 1; }
grep -Fq 'ExecStart=/usr/bin/openvoice-desktop daemon' "$UNIT" \
  || { echo "installed unit does not launch the installed binary" >&2; exit 1; }

# The installed binary must resolve its own config path without a daemon.
"$BINARY" config-path >/dev/null

mkdir -p "$CONFIG_DIR"
printf 'schema_version = 1\n' >"$CONFIG_PATH"
CONFIG_HASH="$(sha256sum "$CONFIG_PATH" | cut -d' ' -f1)"

# `--reinstall` forces the unpack apt would otherwise skip for an equal version,
# which is the same dpkg path a version upgrade takes.
install_package --reinstall
if [[ "$(sha256sum "$CONFIG_PATH" | cut -d' ' -f1)" != "$CONFIG_HASH" ]]; then
  echo "reinstall changed existing config" >&2
  exit 1
fi

DEBIAN_FRONTEND=noninteractive "${APT[@]}" remove -y open-voice-desktop
[[ ! -e "$BINARY" ]] || { echo "uninstall left installed executable" >&2; exit 1; }
[[ ! -e "$UNIT" ]] || { echo "uninstall left systemd user unit" >&2; exit 1; }
[[ ! -e "$DESKTOP_ENTRY" ]] || { echo "uninstall left desktop entry" >&2; exit 1; }
[[ -f "$CONFIG_PATH" ]] || { echo "uninstall removed user config" >&2; exit 1; }

echo "Linux installer smoke: PASS"
exit 0
