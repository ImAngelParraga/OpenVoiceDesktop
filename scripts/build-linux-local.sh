#!/usr/bin/env bash
set -euo pipefail

# Build the Linux desktop shell and bundle it into the release `.deb`, then
# verify the package payload without requiring root. Mirrors
# scripts/build-windows-local.sh for the native Linux packaging path.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_TESTS=1

for argument in "$@"; do
  case "$argument" in
    --skip-tests) RUN_TESTS=0 ;;
    *)
      echo "Usage: $0 [--skip-tests]" >&2
      exit 2
      ;;
  esac
done

export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"

for tool in cargo cargo-tauri dpkg-deb; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "$tool not found. Run ./scripts/setup-linux-build.sh first." >&2
    exit 1
  fi
done

cd "$ROOT"

if [[ $RUN_TESTS -eq 1 ]]; then
  cargo test --features desktop-shell
fi

ARTIFACT_DIR="$ROOT/target/release/bundle/deb"
mkdir -p "$ARTIFACT_DIR"
find "$ARTIFACT_DIR" -maxdepth 1 -type f \
  \( -name '*.deb' -o -name '*.deb.sha256' \) -delete

cargo tauri build \
  --features desktop-shell \
  --bundles deb

mapfile -t PACKAGES < <(find "$ARTIFACT_DIR" -maxdepth 1 -type f -name '*.deb' -print)
if [[ ${#PACKAGES[@]} -ne 1 ]]; then
  echo "Expected one Debian package in $ARTIFACT_DIR; found ${#PACKAGES[@]}." >&2
  exit 1
fi

PACKAGE="${PACKAGES[0]}"
CHECKSUM="$PACKAGE.sha256"

"$ROOT/scripts/verify-deb.sh" "$PACKAGE"

(
  cd "$(dirname "$PACKAGE")"
  sha256sum "$(basename "$PACKAGE")" >"$(basename "$CHECKSUM")"
)

echo
echo "Linux package: $PACKAGE"
echo "SHA-256:       $CHECKSUM"
echo "Payload verified without root; run ./scripts/test-deb-install.sh for the install/upgrade/remove smoke."
