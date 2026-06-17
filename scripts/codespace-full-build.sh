#!/usr/bin/env bash
# Full Tauri Android APK build + verification, designed to run inside a
# GitHub Codespace (4 cores, 16GB RAM, 32GB storage).
#
# What this script does:
#   1. Installs Rust + Android SDK/NDK + JDK (with javac)
#   2. Cross-compiles the Rust solver for arm64-v8a + x86_64 (skip armv7/i686 to save time)
#   3. Runs the solver-logic verification script (proves Rust code is correct)
#   4. Builds an Android debug APK via Tauri
#   5. Verifies the APK contains lib/<abi>/libsumzle_tauri_lib.so + index.html + assets/
#   6. Stores build artifacts to /tmp/tauri-build-output/ for download
#
# Usage:  bash scripts/codespace-full-build.sh
# Output: /tmp/tauri-build-output/{apk/, logs/, summary.txt}

set -euo pipefail
set -o pipefail

LOG_DIR=/tmp/tauri-build-output/logs
OUT_DIR=/tmp/tauri-build-output
mkdir -p "$LOG_DIR" "$OUT_DIR"
SUMMARY="$OUT_DIR/summary.txt"
: > "$SUMMARY"

log()  { echo "[$(date +%H:%M:%S)] $*" | tee -a "$SUMMARY"; }
fail() { log "ERROR: $*"; exit 1; }

REPO_ROOT="${REPO_ROOT:-$(pwd)}"
cd "$REPO_ROOT"
log "Repo: $REPO_ROOT"
log "Branch: $(git rev-parse --abbrev-ref HEAD)"
log "Commit: $(git rev-parse --short HEAD)"

# ---------------------------------------------------------------------------
# 0. Detect available disk and memory
# ---------------------------------------------------------------------------
log ""
log "=== Environment ==="
log "Disk: $(df -h / | tail -1 | awk '{print $4}') free"
log "Memory: $(free -h | awk '/^Mem:/ {print $2}') total"
log "CPUs: $(nproc)"

# ---------------------------------------------------------------------------
# 1. Install Rust
# ---------------------------------------------------------------------------
log ""
log "=== Step 1: Install Rust ==="
if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal
  source "$HOME/.cargo/env"
fi
log "Rust: $(rustc --version)"
log "Adding Android targets..."
rustup target add aarch64-linux-android x86_64-linux-android armv7-linux-androideabi i686-linux-android 2>&1 | tail -5

# ---------------------------------------------------------------------------
# 2. Install Android SDK + NDK + build-tools
# ---------------------------------------------------------------------------
log ""
log "=== Step 2: Install Android SDK + NDK ==="
export ANDROID_HOME="$HOME/android-sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
export NDK_HOME="$ANDROID_NDK_HOME"
mkdir -p "$ANDROID_HOME/cmdline-tools"

if [ ! -d "$ANDROID_HOME/cmdline-tools/latest" ]; then
  log "  Downloading cmdline-tools..."
  curl -sL -o /tmp/cmdline-tools.zip https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip
  unzip -q /tmp/cmdline-tools.zip -d "$ANDROID_HOME/cmdline-tools/"
  mv "$ANDROID_HOME/cmdline-tools/cmdline-tools" "$ANDROID_HOME/cmdline-tools/latest"
  rm /tmp/cmdline-tools.zip
fi

export PATH="$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH"

log "  Installing platform-tools, platform 34, build-tools 34.0.0, NDK 27..."
yes | sdkmanager --licenses > /dev/null 2>&1
sdkmanager "platform-tools" "platforms;android-34" "build-tools;34.0.0" "ndk;27.0.12077973" 2>&1 | tail -3

# ---------------------------------------------------------------------------
# 3. Install full JDK (with javac) — codespace may only have JRE
# ---------------------------------------------------------------------------
log ""
log "=== Step 3: Install JDK with javac ==="
if ! command -v javac >/dev/null 2>&1; then
  log "  Downloading Temurin JDK 21..."
  curl -sL -o /tmp/jdk21.tar.gz "https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.5%2B11/OpenJDK21U-jdk_x64_linux_hotspot_21.0.5_11.tar.gz"
  mkdir -p "$HOME/jdk21"
  tar -xzf /tmp/jdk21.tar.gz -C "$HOME/jdk21" --strip-components=1
  rm /tmp/jdk21.tar.gz
fi
export JAVA_HOME="${JAVA_HOME:-$HOME/jdk21}"
export JDK_HOME="$JAVA_HOME"
export PATH="$JAVA_HOME/bin:$PATH"
log "JDK: $(javac --version 2>&1)"

# ---------------------------------------------------------------------------
# 4. Install Tauri CLI (npm precompiled — avoids cargo install timeout)
# ---------------------------------------------------------------------------
log ""
log "=== Step 4: Install Tauri CLI + frontend deps ==="
cd "$REPO_ROOT"
log "  Installing @tauri-apps/cli..."
npm install --no-audit --no-fund 2>&1 | tail -3
log "  Tauri CLI: $(npx tauri --version)"

log "  Installing frontend deps..."
cd frontend
npm install --no-audit --no-fund 2>&1 | tail -3
cd "$REPO_ROOT"

# ---------------------------------------------------------------------------
# 5. Verify solver logic (fast — proves Rust code is correct)
# ---------------------------------------------------------------------------
log ""
log "=== Step 5: Verify solver logic (Rust side) ==="
bash scripts/verify-solver-logic.sh 2>&1 | tee "$LOG_DIR/verify-solver-logic.log" | tail -20

# ---------------------------------------------------------------------------
# 6. Initialize Tauri Android project (if not yet)
# ---------------------------------------------------------------------------
log ""
log "=== Step 6: Initialize Tauri Android project ==="
if [ ! -d src-tauri/gen/android ]; then
  npx tauri android init 2>&1 | tee "$LOG_DIR/tauri-init.log" | tail -10
fi
log "  Android project ready: src-tauri/gen/android"

# ---------------------------------------------------------------------------
# 7. Build APK — start with aarch64 (fastest single ABI)
#    Then add x86_64 for emulator testing if time permits.
# ---------------------------------------------------------------------------
log ""
log "=== Step 7: Build Android debug APK (aarch64 + x86_64) ==="
# We'll do aarch64 first since it's the most common phone ABI
log "  Building aarch64 (arm64-v8a)..."
npx tauri android build --debug --apk --target aarch64 2>&1 | tee "$LOG_DIR/build-aarch64.log" | tail -10

# Copy aarch64 .so before next ABI overwrites symlink
AArch64_SO=$(find /tmp/cargo-target /home/*/.cargo /root/.cargo -name "libsumzle_tauri_lib.so" -path "*aarch64*" 2>/dev/null | head -1)
log "  aarch64 .so: $AArch64_SO"

log "  Building x86_64 (for emulator)..."
npx tauri android build --debug --apk --target x86_64 2>&1 | tee "$LOG_DIR/build-x86_64.log" | tail -10 || log "  WARN: x86_64 build failed, continuing with aarch64 only"

# Find the produced APK
APK=$(find src-tauri/gen/android -name "*.apk" -path "*debug*" 2>/dev/null | head -1)
[ -z "$APK" ] && fail "No APK produced"
log "  APK: $APK"
log "  APK size: $(du -h "$APK" | awk '{print $1}')"

# ---------------------------------------------------------------------------
# 8. Fix frontend assets in APK (Tauri 2 sometimes doesn't auto-copy)
# ---------------------------------------------------------------------------
log ""
log "=== Step 8: Ensure frontend dist is in APK ==="
# Re-copy frontend dist to assets dir and re-zipalign+sign
ASSETS_DIR="src-tauri/gen/android/app/src/main/assets"
if [ -d frontend/dist ] && [ -f frontend/dist/index.html ]; then
  log "  Copying frontend/dist/* -> $ASSETS_DIR/"
  cp -r frontend/dist/* "$ASSETS_DIR/"

  # Re-sign APK with frontend assets
  APKSIGNED="$OUT_DIR/sumzle-tauri-debug.apk"
  ALIGNED="/tmp/apk-aligned.apk"
  log "  zipalign..."
  "$ANDROID_HOME/build-tools/34.0.0/zipalign" -f 4 "$APK" "$ALIGNED"

  # Create debug keystore if missing
  KS="$HOME/.android/debug.keystore"
  mkdir -p "$(dirname "$KS")"
  if [ ! -f "$KS" ]; then
    keytool -genkey -v -keystore "$KS" -storepass android -alias androiddebugkey \
      -keypass android -keyalg RSA -keysize 2048 -validity 10000 \
      -dname "CN=Android Debug,O=Android,C=US" 2>/dev/null
  fi

  log "  Adding frontend assets to APK..."
  cd "$ASSETS_DIR"
  zip -r "$ALIGNED" index.html assets/ 2>/dev/null
  cd "$REPO_ROOT"

  # Re-align after zip
  "$ANDROID_HOME/build-tools/34.0.0/zipalign" -f 4 "$ALIGNED" "$APKSIGNED"
  log "  apksigner sign..."
  "$ANDROID_HOME/build-tools/34.0.0/apksigner" sign \
    --ks "$KS" --ks-pass pass:android --key-pass pass:android \
    --out "$APKSIGNED" "$APKSIGNED" 2>&1 | tail -3
else
  log "  WARN: frontend/dist not found, skipping asset injection"
  cp "$APK" "$OUT_DIR/sumzle-tauri-debug.apk"
fi

# ---------------------------------------------------------------------------
# 9. Verify final APK contents
# ---------------------------------------------------------------------------
log ""
log "=== Step 9: Verify APK contents ==="
APK_FINAL="$OUT_DIR/sumzle-tauri-debug.apk"
log "  Final APK: $APK_FINAL"
log "  Size: $(du -h "$APK_FINAL" | awk '{print $1}')"
log ""
log "  Native libraries (.so):"
unzip -l "$APK_FINAL" | grep -E "lib/.*\.so" | tee -a "$SUMMARY"
log ""
log "  Frontend assets:"
unzip -l "$APK_FINAL" | grep -E "(index\.html|assets/.*\.(js|css|svg))" | head -10 | tee -a "$SUMMARY"
log ""
log "  Manifest:"
unzip -l "$APK_FINAL" | grep "AndroidManifest.xml" | tee -a "$SUMMARY"
log ""
log "  Signature verification:"
"$ANDROID_HOME/build-tools/34.0.0/apksigner" verify --verbose "$APK_FINAL" 2>&1 | head -5 | tee -a "$SUMMARY"

log ""
log "============================================================"
log "  ✓ BUILD COMPLETE"
log "============================================================"
log "  Artifacts: $OUT_DIR/"
log "  - sumzle-tauri-debug.apk  (signed, installable on Android arm64)"
log "  - summary.txt              (this log)"
log "  - logs/                    (detailed build logs)"
log "============================================================"
