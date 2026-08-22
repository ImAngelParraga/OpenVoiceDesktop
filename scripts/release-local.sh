#!/usr/bin/env bash
set -euo pipefail

# Build every release artifact on this Ubuntu host and publish them to a GitHub
# release. This repository runs no GitHub Actions: the binaries a user downloads
# are exactly the ones compiled here.
#
#   Linux    native Tauri deb bundle
#   Windows  cargo-xwin MSVC cross-build into an NSIS setup
#
# Prerequisites:
#   ./scripts/setup-linux-build.sh      once, for the Linux bundle
#   ./scripts/setup-windows-cross.sh    once, for the Windows cross-build
#   gh auth login                       once, to publish

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_LINUX=1
BUILD_WINDOWS=1
RUN_TESTS=1
DRY_RUN=0

usage() {
  cat <<'USAGE'
Usage: ./scripts/release-local.sh [options]

  --linux-only    Build and publish only the Debian package.
  --windows-only  Build and publish only the Windows NSIS setup.
  --skip-tests    Do not run cargo test before building.
  --dry-run       Build and verify everything, publish nothing.
  -h, --help      Show this help.

The release tag is always v<Cargo version>. Bump Cargo.toml (and the package
stanza in Cargo.lock) and commit before releasing.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --linux-only) BUILD_WINDOWS=0 ;;
    --windows-only) BUILD_LINUX=0 ;;
    --skip-tests) RUN_TESTS=0 ;;
    --dry-run) DRY_RUN=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

cd "$ROOT"
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"

for tool in cargo jq gh; do
  command -v "$tool" >/dev/null 2>&1 || { echo "$tool not found." >&2; exit 1; }
done

VERSION="$(cargo metadata --no-deps --format-version 1 \
  | jq -r '.packages[] | select(.name == "openvoice-desktop") | .version')"
if [[ -z "$VERSION" || "$VERSION" == "null" ]]; then
  echo "Could not read the Cargo version." >&2
  exit 1
fi
TAG="v$VERSION"

# A release must be reproducible from a commit that exists on the remote.
if [[ -n "$(git status --porcelain)" ]]; then
  echo "Working tree is dirty; commit or stash before releasing $TAG." >&2
  git status --short >&2
  exit 1
fi
HEAD_SHA="$(git rev-parse HEAD)"
if ! git branch -r --contains "$HEAD_SHA" | grep -q .; then
  echo "HEAD is not on any remote branch; push before releasing $TAG." >&2
  exit 1
fi

STAGING="$ROOT/target/release-assets/$TAG"
rm -rf "$STAGING"
mkdir -p "$STAGING"

stage_asset() {
  local source="$1"
  local name="$2"
  cp "$source" "$STAGING/$name"
  (cd "$STAGING" && sha256sum "$name" >"$name.sha256")
  echo "staged $name"
}

TEST_ARGUMENTS=()
if [[ $RUN_TESTS -eq 0 ]]; then
  TEST_ARGUMENTS+=(--skip-tests)
fi

if [[ $BUILD_LINUX -eq 1 ]]; then
  echo
  echo "== Linux package =="
  ./scripts/build-linux-local.sh "${TEST_ARGUMENTS[@]}"
  mapfile -t PACKAGES < <(find target/release/bundle/deb -maxdepth 1 -type f -name '*.deb' -print)
  if [[ ${#PACKAGES[@]} -ne 1 ]]; then
    echo "Expected one Debian package; found ${#PACKAGES[@]}." >&2
    exit 1
  fi
  stage_asset "${PACKAGES[0]}" "OpenVoiceDesktop_${VERSION}_amd64.deb"
fi

if [[ $BUILD_WINDOWS -eq 1 ]]; then
  echo
  echo "== Windows installer =="
  # The Linux run already covered the shared test suite.
  ./scripts/build-windows-local.sh --skip-tests
  mapfile -t INSTALLERS < <(find target/x86_64-pc-windows-msvc/release/bundle/nsis \
    -maxdepth 1 -type f -name '*-setup.exe' -print)
  if [[ ${#INSTALLERS[@]} -ne 1 ]]; then
    echo "Expected one NSIS setup; found ${#INSTALLERS[@]}." >&2
    exit 1
  fi
  stage_asset "${INSTALLERS[0]}" "OpenVoiceDesktop_${VERSION}_x64-setup.exe"
fi

echo
echo "== Staged assets =="
ls -1 "$STAGING"
(cd "$STAGING" && sha256sum -c ./*.sha256)

if [[ $DRY_RUN -eq 1 ]]; then
  echo
  echo "Dry run: nothing published. Assets are in $STAGING"
  exit 0
fi

NOTES="Built locally on Ubuntu and uploaded from this machine; this repository runs no CI.

Windows: download \`OpenVoiceDesktop_${VERSION}_x64-setup.exe\` and double-click it. The build is unsigned, so SmartScreen may ask for confirmation. Cross-compiled from Linux and not smoke-tested on Windows.

Ubuntu/Debian: download \`OpenVoiceDesktop_${VERSION}_amd64.deb\` and run \`sudo apt install ./OpenVoiceDesktop_${VERSION}_amd64.deb\`.

Verify any asset with the adjacent \`.sha256\` file."

if ! git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
  git tag "$TAG"
fi
if [[ "$(git rev-parse "$TAG^{commit}")" != "$HEAD_SHA" ]]; then
  echo "Tag $TAG does not point at HEAD; move it deliberately before releasing." >&2
  exit 1
fi
git push origin "$TAG"

if ! gh release view "$TAG" >/dev/null 2>&1; then
  gh release create "$TAG" \
    --title "OpenVoiceDesktop $TAG" \
    --target "$HEAD_SHA" \
    --notes "$NOTES"
fi
gh release upload "$TAG" "$STAGING"/* --clobber

echo
echo "Published $TAG"
gh release view "$TAG" --json assets --jq '.assets[].name'
