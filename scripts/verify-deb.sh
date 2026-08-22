#!/usr/bin/env bash
set -euo pipefail

# Verify the release Debian package payload without installing it, so the
# packaging contract can be checked on any host and inside CI.
#
# The apt package name is `open-voice-desktop`: Tauri derives it by kebab-casing
# the `productName`, while the binary keeps its Cargo name.

PACKAGE_NAME="open-voice-desktop"

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 <path-to-openvoice-desktop.deb>" >&2
  exit 2
fi

PACKAGE="$1"
if [[ ! -f "$PACKAGE" ]]; then
  echo "package not found: $PACKAGE" >&2
  exit 2
fi

CONTROL_NAME="$(dpkg-deb -f "$PACKAGE" Package)"
if [[ "$CONTROL_NAME" != "$PACKAGE_NAME" ]]; then
  echo "Expected package name $PACKAGE_NAME; found $CONTROL_NAME." >&2
  exit 1
fi

MEMBERS="$(dpkg-deb --fsys-tarfile "$PACKAGE" | tar -t)"
for entry in \
  'usr/bin/openvoice-desktop' \
  'usr/lib/systemd/user/openvoice-desktop.service' \
  'usr/share/applications/OpenVoiceDesktop.desktop'
do
  if ! grep -Fxq "$entry" <<<"$MEMBERS"; then
    echo "Package is missing $entry" >&2
    echo "$MEMBERS" >&2
    exit 1
  fi
done

member_contents() {
  dpkg-deb --fsys-tarfile "$PACKAGE" | tar -xO "$1"
}

UNIT="$(member_contents usr/lib/systemd/user/openvoice-desktop.service)"
if ! grep -Fq 'ExecStart=/usr/bin/openvoice-desktop daemon' <<<"$UNIT"; then
  echo "Packaged systemd user unit must launch the installed binary." >&2
  exit 1
fi

DESKTOP="$(member_contents usr/share/applications/OpenVoiceDesktop.desktop)"
if ! grep -Eq '^Exec=openvoice-desktop settings$' <<<"$DESKTOP"; then
  echo "Desktop entry must open the settings window; Linux builds have no tray." >&2
  echo "$DESKTOP" >&2
  exit 1
fi

DEPENDS="$(dpkg-deb -f "$PACKAGE" Depends)"
for dependency in 'libwebkit2gtk-4.1-0' 'libasound2' 'wl-clipboard'; do
  if ! grep -Fq "$dependency" <<<"$DEPENDS"; then
    echo "Package must depend on $dependency; Depends is: $DEPENDS" >&2
    exit 1
  fi
done

echo "Linux package payload: PASS"
