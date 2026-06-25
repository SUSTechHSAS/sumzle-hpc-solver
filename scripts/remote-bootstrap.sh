#!/usr/bin/env bash
#
# Bootstrap the Tauri 2 Android build toolchain on a remote build host.
#
# Heavy builds (Rust + Android NDK + Gradle) run on a remote machine because the
# local box is resource-constrained; the local box only runs waydroid + installs
# the APK. This script is idempotent — safe to re-run on a fresh container.
#
# Usage:   REMOTE=user@host ./scripts/remote-bootstrap.sh
#   or:    ./scripts/remote-bootstrap.sh user@host
#
# The remote host identifier is EPHEMERAL (e.g. a CNB cloud container destroyed
# after inactivity); pass the current one each session.
set -euo pipefail

REMOTE="${REMOTE:-${1:-}}"
if [[ -z "$REMOTE" ]]; then
  echo "error: set REMOTE=user@host (or pass it as the first arg)" >&2
  exit 1
fi

NDK_VERSION="27.2.12479018"
PLATFORM="android-34"
BUILD_TOOLS="34.0.0"

echo ">>> Bootstrapping Android/Tauri toolchain on $REMOTE ..."
ssh -o BatchMode=yes "$REMOTE" NDK_VERSION="$NDK_VERSION" PLATFORM="$PLATFORM" \
    BUILD_TOOLS="$BUILD_TOOLS" 'bash -s' <<'REMOTE_SCRIPT'
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive

# --- C toolchain (Rust build scripts + NDK need a linker) -------------------
apt-get update -qq
apt-get install -y -qq build-essential pkg-config curl unzip

# --- JDK (Android Gradle Plugin needs 17+; Debian 13 ships 21) --------------
if ! command -v javac >/dev/null 2>&1; then
  apt-get install -y -qq openjdk-17-jdk-headless 2>/dev/null \
    || apt-get install -y -qq openjdk-21-jdk-headless
fi
JAVA_HOME="$(dirname "$(dirname "$(readlink -f "$(command -v javac)")")")"

# --- Rust + Android targets -------------------------------------------------
if [ ! -x "$HOME/.cargo/bin/rustc" ]; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain stable
fi
source "$HOME/.cargo/env"
rustup target add x86_64-linux-android aarch64-linux-android \
                  armv7-linux-androideabi i686-linux-android

# --- cargo-tauri CLI (NOT the npm CLI — see README) -------------------------
command -v cargo-tauri >/dev/null 2>&1 || cargo install tauri-cli --version "^2" --locked

# --- Android SDK cmdline-tools + platform/build-tools/NDK -------------------
export ANDROID_HOME="$HOME/android-sdk"
mkdir -p "$ANDROID_HOME/cmdline-tools"
if [ ! -d "$ANDROID_HOME/cmdline-tools/latest" ]; then
  curl -fsSL -o /tmp/cmdtools.zip \
    https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip
  unzip -q /tmp/cmdtools.zip -d "$ANDROID_HOME/cmdline-tools"
  mv "$ANDROID_HOME/cmdline-tools/cmdline-tools" "$ANDROID_HOME/cmdline-tools/latest"
fi
export PATH="$PATH:$ANDROID_HOME/cmdline-tools/latest/bin"
export JAVA_HOME
yes 2>/dev/null | sdkmanager --licenses >/dev/null 2>&1 || true
sdkmanager "platform-tools" "platforms;${PLATFORM}" \
           "build-tools;${BUILD_TOOLS}" "ndk;${NDK_VERSION}" >/dev/null

# --- Persisted env sourced by every build shell ----------------------------
cat > "$HOME/android-env.sh" <<EOF
export JAVA_HOME="$JAVA_HOME"
export ANDROID_HOME="$ANDROID_HOME"
export ANDROID_SDK_ROOT="\$ANDROID_HOME"
export NDK_HOME="\$ANDROID_HOME/ndk/${NDK_VERSION}"
export ANDROID_NDK_HOME="\$NDK_HOME"
export PATH="\$PATH:\$ANDROID_HOME/cmdline-tools/latest/bin:\$ANDROID_HOME/platform-tools"
[ -f "\$HOME/.cargo/env" ] && source "\$HOME/.cargo/env"
EOF

echo "OK: $(javac -version 2>&1), $("$HOME/.cargo/bin/rustc" --version), NDK ${NDK_VERSION}"
REMOTE_SCRIPT

echo ">>> Bootstrap complete. Next: ./scripts/remote-build.sh $REMOTE"
