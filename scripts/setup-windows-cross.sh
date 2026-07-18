#!/usr/bin/env bash
set -euo pipefail

TARGET="x86_64-pc-windows-msvc"
CARGO_XWIN_VERSION="0.23.0"
TAURI_CLI_VERSION="2.11.4"
IMAGE="openvoice-desktop-windows-cross:$CARGO_XWIN_VERSION"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:---docker}"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "This setup script supports Ubuntu/Linux hosts only." >&2
  exit 1
fi

if [[ "$MODE" == "--docker" ]]; then
  if ! command -v docker >/dev/null 2>&1; then
    echo "docker not found. Install Docker or run $0 --native." >&2
    exit 1
  fi
  docker info >/dev/null
  docker build \
    --build-arg "CARGO_XWIN_VERSION=$CARGO_XWIN_VERSION" \
    --build-arg "TAURI_CLI_VERSION=$TAURI_CLI_VERSION" \
    --file "$ROOT/scripts/windows-cross.Dockerfile" \
    --tag "$IMAGE" \
    "$ROOT/scripts"
  echo
  echo "Windows cross-build container ready: $IMAGE"
  echo "Run: ./scripts/build-windows-local.sh"
  exit 0
elif [[ "$MODE" != "--native" ]]; then
  echo "Usage: $0 [--docker|--native]" >&2
  exit 2
fi

if ! command -v apt-get >/dev/null 2>&1; then
  echo "apt-get not found; use Docker or install nsis, llvm, lld, and clang manually." >&2
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
"${APT[@]}" install -y nsis llvm lld clang

rustup target add "$TARGET"

install_cargo_tool() {
  local binary="$1"
  local crate="$2"
  local version="$3"

  if command -v "$binary" >/dev/null 2>&1 && "$binary" --version | grep -Fq "$version"; then
    echo "$binary $version already installed"
    return
  fi
  cargo install --locked --version "$version" "$crate"
}

install_cargo_tool cargo-xwin cargo-xwin "$CARGO_XWIN_VERSION"
install_cargo_tool cargo-tauri tauri-cli "$TAURI_CLI_VERSION"

link_versioned_llvm_tool() {
  local name="$1"
  if command -v "$name" >/dev/null 2>&1; then
    return
  fi

  local candidate
  candidate="$(find /usr/bin -maxdepth 1 -name "$name-[0-9]*" -print | sort -V | tail -n 1)"
  if [[ -z "$candidate" ]]; then
    echo "$name not found after installing LLVM." >&2
    exit 1
  fi
  mkdir -p "$HOME/.local/bin"
  ln -sfn "$candidate" "$HOME/.local/bin/$name"
}

link_versioned_llvm_tool llvm-rc
link_versioned_llvm_tool lld-link
if ! command -v clang-cl >/dev/null 2>&1; then
  ln -sfn "$(command -v clang)" "$HOME/.local/bin/clang-cl"
fi

echo
echo "Native Windows cross-build toolchain ready."
echo "Run: ./scripts/build-windows-local.sh"
