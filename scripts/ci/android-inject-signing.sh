#!/usr/bin/env bash
#
# Inject a release `signingConfig` into the Tauri-generated Android Gradle
# script.
#
# `cargo tauri android init` regenerates `src-tauri/gen/android/` on every CI
# run, and the Tauri template wires NO signing into the `release` build type —
# so a release APK comes out unsigned (not installable). This patches the
# generated `app/build.gradle.kts` to add a `release` signing config that reads
# `keystore.properties` (written by the workflow from repo secrets). If that
# file is absent at build time the block is a no-op and the release stays
# unsigned, so the script is safe to run unconditionally.
#
# Idempotent: skips if a `signingConfigs` block is already present.
#
# Usage: scripts/ci/android-inject-signing.sh [path-to-build.gradle.kts]
set -euo pipefail

GRADLE="${1:-src-tauri/gen/android/app/build.gradle.kts}"
if [ ! -f "$GRADLE" ]; then
  echo "error: $GRADLE not found (run 'cargo tauri android init' first)" >&2
  exit 1
fi

if grep -q 'signingConfigs {' "$GRADLE"; then
  echo ">>> signing config already present in $GRADLE; nothing to do"
  exit 0
fi

# `import java.util.Properties` is already at the top of the generated file.
awk '
  # Add a release signing config as a sibling of `buildTypes`, reading the
  # keystore details from keystore.properties in the Gradle root project dir.
  /^[[:space:]]*buildTypes[[:space:]]*\{/ && !sc {
    print "    signingConfigs {"
    print "        create(\"release\") {"
    print "            val ksFile = rootProject.file(\"keystore.properties\")"
    print "            if (ksFile.exists()) {"
    print "                val ksProps = Properties()"
    print "                ksFile.inputStream().use { ksProps.load(it) }"
    print "                storeFile = file(ksProps.getProperty(\"storeFile\"))"
    print "                storePassword = ksProps.getProperty(\"storePassword\")"
    print "                keyAlias = ksProps.getProperty(\"keyAlias\")"
    print "                keyPassword = ksProps.getProperty(\"keyPassword\")"
    print "            }"
    print "        }"
    print "    }"
    sc = 1
  }
  { print }
  # Wire that signing config into the release build type.
  /getByName\("release"\)[[:space:]]*\{/ && !rel {
    print "            if (rootProject.file(\"keystore.properties\").exists()) {"
    print "                signingConfig = signingConfigs.getByName(\"release\")"
    print "            }"
    rel = 1
  }
' "$GRADLE" > "$GRADLE.tmp"
mv "$GRADLE.tmp" "$GRADLE"
echo ">>> injected release signingConfig into $GRADLE"
