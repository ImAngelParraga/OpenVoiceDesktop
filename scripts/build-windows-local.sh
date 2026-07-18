#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="x86_64-pc-windows-msvc"
IMAGE="openvoice-desktop-windows-cross:0.23.0"
RUN_TESTS=1
CONTAINER_INNER=0

for argument in "$@"; do
  case "$argument" in
    --skip-tests) RUN_TESTS=0 ;;
    --container-inner) CONTAINER_INNER=1 ;;
    *)
      echo "Usage: $0 [--skip-tests]" >&2
      exit 2
      ;;
  esac
done

export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"
export XWIN_CACHE_DIR="${XWIN_CACHE_DIR:-$HOME/.cache/cargo-xwin}"

native_tools_ready() {
  local tool
  for tool in cargo cargo-xwin cargo-tauri makensis llvm-rc lld-link; do
    command -v "$tool" >/dev/null 2>&1 || return 1
  done
  rustup target list --installed | grep -Fxq "$TARGET"
}

if [[ $CONTAINER_INNER -eq 0 ]] && ! native_tools_ready; then
  if ! command -v docker >/dev/null 2>&1 || ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
    echo "Cross-build tools not found. Run ./scripts/setup-windows-cross.sh first." >&2
    exit 1
  fi

  CACHE_ROOT="${OPENVOICE_CROSS_CACHE:-$HOME/.cache/openvoice-desktop/windows-cross}"
  mkdir -p "$CACHE_ROOT"/{cargo,home,xwin}
  INNER_ARGUMENTS=(--container-inner)
  if [[ $RUN_TESTS -eq 0 ]]; then
    INNER_ARGUMENTS+=(--skip-tests)
  fi
  exec docker run --rm \
    --user "$(id -u):$(id -g)" \
    --env "CARGO_HOME=/cache/cargo" \
    --env "HOME=/cache/home" \
    --env "RUSTUP_HOME=/usr/local/rustup" \
    --env "XWIN_CACHE_DIR=/cache/xwin" \
    --volume "$ROOT:/workspace" \
    --volume "$CACHE_ROOT:/cache" \
    --workdir /workspace \
    "$IMAGE" \
    ./scripts/build-windows-local.sh "${INNER_ARGUMENTS[@]}"
fi

if ! native_tools_ready; then
  echo "Container cross-build image is missing required tools." >&2
  exit 1
fi

cd "$ROOT"

if [[ $RUN_TESTS -eq 1 ]]; then
  cargo test
fi

ARTIFACT_DIR="$ROOT/target/$TARGET/release/bundle/nsis"
mkdir -p "$ARTIFACT_DIR"
find "$ARTIFACT_DIR" -maxdepth 1 -type f \
  \( -name '*-setup.exe' -o -name '*-setup.exe.sha256' \) -delete

cargo tauri build \
  --runner cargo-xwin \
  --target "$TARGET" \
  --features desktop-shell \
  --bundles nsis

mapfile -t INSTALLERS < <(find "$ARTIFACT_DIR" -maxdepth 1 -type f -name '*-setup.exe' -print)
if [[ ${#INSTALLERS[@]} -ne 1 ]]; then
  echo "Expected one NSIS setup in $ARTIFACT_DIR; found ${#INSTALLERS[@]}." >&2
  exit 1
fi

INSTALLER="${INSTALLERS[0]}"
CHECKSUM="$INSTALLER.sha256"
(
  cd "$(dirname "$INSTALLER")"
  sha256sum "$(basename "$INSTALLER")" >"$(basename "$CHECKSUM")"
)

echo
echo "Windows installer: $INSTALLER"
echo "SHA-256:          $CHECKSUM"
echo "Native Windows install/reinstall/uninstall smoke remains required before release."
