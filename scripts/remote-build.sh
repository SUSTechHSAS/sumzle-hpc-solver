#!/usr/bin/env bash
#
# Build the Sumzle Solver Android APK on the remote host and fetch it locally.
#
# Syncs the working tree to the remote (rsync, excluding build/dep dirs), runs
# `cargo tauri android build`, and copies the APK back to ./build/.
#
# Usage:   REMOTE=user@host ./scripts/remote-build.sh [abi]
#   or:    ./scripts/remote-build.sh user@host [abi]
#
#   abi: x86_64 (default — for waydroid on a PC) | aarch64 (real phones) |
#        armv7 | i686.  Pass RELEASE=1 for a release build (needs signing).
set -euo pipefail

# Allow either `REMOTE=... script abi` or `script user@host abi`.
if [[ "${1:-}" == *@* ]]; then REMOTE="$1"; shift; fi
REMOTE="${REMOTE:-}"
ABI="${1:-x86_64}"
if [[ -z "$REMOTE" ]]; then
  echo "error: set REMOTE=user@host (or pass it as the first arg)" >&2
  exit 1
fi

REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
REMOTE_DIR="sumzle-hpc-solver"
PROFILE_FLAG="--debug"; [[ "${RELEASE:-0}" == "1" ]] && PROFILE_FLAG=""
BUILD_KIND="debug"; [[ "${RELEASE:-0}" == "1" ]] && BUILD_KIND="release"

echo ">>> rsync working tree -> $REMOTE:~/$REMOTE_DIR"
rsync -az --delete \
  --exclude target --exclude node_modules --exclude frontend/dist \
  --exclude .git --exclude 'src-tauri/gen' --exclude 'src-tauri/target' \
  --exclude build \
  "$REPO_DIR"/ "$REMOTE":~/"$REMOTE_DIR"/

echo ">>> install frontend deps (npm ci) + build APK ($ABI, $BUILD_KIND)"
ssh -o BatchMode=yes "$REMOTE" REMOTE_DIR="$REMOTE_DIR" ABI="$ABI" \
    PROFILE_FLAG="$PROFILE_FLAG" 'bash -s' <<'REMOTE_SCRIPT'
set -euo pipefail
source "$HOME/android-env.sh"
cd "$HOME/$REMOTE_DIR"
[ -d frontend/node_modules ] || (cd frontend && PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm ci)
# First-time only: scaffold the Gradle project (idempotent-ish; skip if present).
[ -d src-tauri/gen/android ] || cargo tauri android init
cargo tauri android build $PROFILE_FLAG --apk --target "$ABI"
REMOTE_SCRIPT

echo ">>> fetch APK"
mkdir -p "$REPO_DIR/build"
APK_GLOB="src-tauri/gen/android/app/build/outputs/apk/universal/${BUILD_KIND}/app-universal-${BUILD_KIND}.apk"
scp "$REMOTE":~/"$REMOTE_DIR"/"$APK_GLOB" \
    "$REPO_DIR/build/sumzle-solver-${ABI}-${BUILD_KIND}.apk"
echo ">>> done: build/sumzle-solver-${ABI}-${BUILD_KIND}.apk"
ls -la "$REPO_DIR/build/sumzle-solver-${ABI}-${BUILD_KIND}.apk"
