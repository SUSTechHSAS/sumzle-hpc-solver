#!/usr/bin/env bash
#
# Install + launch the Sumzle Solver APK in local waydroid (debug loop).
#
# waydroid's PC image is x86_64, so install the x86_64 APK. Assumes the
# `waydroid-container` service is already running (start once with
# `sudo systemctl start waydroid-container`).
#
# Usage: ./scripts/waydroid-run.sh [path-to-apk]
set -euo pipefail

REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
APK="${1:-$REPO_DIR/build/sumzle-solver-x86_64-debug.apk}"
PKG="com.sumzle.solver"

[[ -f "$APK" ]] || { echo "error: APK not found: $APK (run scripts/remote-build.sh first)" >&2; exit 1; }

# Ensure a session is running (Android booted).
if ! waydroid status 2>/dev/null | grep -q 'Session:[[:space:]]*RUNNING'; then
  echo ">>> starting waydroid session ..."
  nohup waydroid session start >/tmp/waydroid-session.log 2>&1 &
  for _ in $(seq 1 30); do
    waydroid status 2>/dev/null | grep -q 'Session:[[:space:]]*RUNNING' && break
    sleep 2
  done
fi

# Fail loudly if the session never reached RUNNING, rather than proceeding to a
# confusing `waydroid app install` failure.
if ! waydroid status 2>/dev/null | grep -q 'Session:[[:space:]]*RUNNING'; then
  echo "error: waydroid session did not reach RUNNING within ~60s" >&2
  echo "       check: sudo systemctl status waydroid-container; cat /tmp/waydroid-session.log" >&2
  exit 1
fi
waydroid status 2>/dev/null | head -5

echo ">>> installing $APK"
# `waydroid app install` is fire-and-forget (prints nothing on success).
waydroid app install "$APK"

echo ">>> launching $PKG"
waydroid app launch "$PKG"
echo ">>> launched. Logs: adb logcat | grep -iE 'sumzle|RustStdoutStderr'"
echo ">>> WebView DevTools: adb forward tcp:9222 localabstract:webview_devtools_remote_\$(adb shell pidof $PKG); open http://localhost:9222"
