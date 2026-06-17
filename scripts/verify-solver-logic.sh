#!/usr/bin/env bash
# CI / local verification script for the Tauri mobile build.
#
# Verifies the solver still produces correct results on the Rust side (which
# is exactly the code that gets cross-compiled into the Android APK's
# libsumzle_tauri_lib.so). Does NOT require Tauri runtime or Android SDK —
# so it can run on any Linux CI runner in seconds.
#
# Usage:
#   ./scripts/verify-solver-logic.sh
#
# Exits 0 on success, non-zero on failure.

set -euo pipefail

REPO_ROOT="${REPO_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "$REPO_ROOT"

# Build the CLI binary (release for speed).
echo "[verify] Building sumzle-solver binary (release)..."
cargo build --release --bin sumzle-solver

BIN="${CARGO_TARGET_DIR:-target}/release/sumzle-solver"
if [ ! -x "$BIN" ]; then
  BIN="target/release/sumzle-solver"
fi
if [ ! -x "$BIN" ]; then
  echo "[verify] ERROR: sumzle-solver binary not found"
  exit 1
fi

echo "[verify] Binary: $BIN"
echo ""

# ---------------------------------------------------------------------------
# Test 1: Solve a length-5 puzzle with constraints
# Expected: 28 solutions including 1-1=0, 1*5=5, 1/1=1, 1-0=1
# ---------------------------------------------------------------------------
echo "============================================================"
echo " Test 1: Solve length=5 with pos0=1 (correct), pos3== (correct)"
echo "============================================================"

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT
cat > "$TMPDIR/p1.json" <<'EOF'
{
  "length": 5,
  "rows": [
    {
      "tiles": [
        {"char": "1", "state": "correct"},
        {"char": "+", "state": "empty"},
        {"char": "2", "state": "empty"},
        {"char": "=", "state": "correct"},
        {"char": "3", "state": "empty"}
      ]
    }
  ]
}
EOF

SOLVE_JSON=$("$BIN" solve -i "$TMPDIR/p1.json" -t 1 -f json)
SOLVE_COUNT=$(echo "$SOLVE_JSON" | python3 -c "import json,sys; print(json.load(sys.stdin)['stats']['found_count'])")
echo "  Solutions found: $SOLVE_COUNT"

# Validate expected solutions are present
for expected in "1-1=0" "1*5=5" "1/1=1" "1-0=1"; do
  if echo "$SOLVE_JSON" | python3 -c "import json,sys; sys.exit(0 if '$expected' in json.load(sys.stdin)['solutions'] else 1)"; then
    echo "  ✓ contains: $expected"
  else
    echo "  ✗ MISSING: $expected"
    exit 1
  fi
done

# ---------------------------------------------------------------------------
# Test 2: Validate equations
# ---------------------------------------------------------------------------
echo ""
echo "============================================================"
echo " Test 2: Equation validation"
echo "============================================================"

if [ "$("$BIN" validate "1+2=3")" = "true" ]; then
  echo "  ✓ validate('1+2=3') = true"
else
  echo "  ✗ validate('1+2=3') should be true"
  exit 1
fi

if [ "$("$BIN" validate "6=2*3")" = "false" ]; then
  echo "  ✓ validate('6=2*3') = false (RHS must be a number)"
else
  echo "  ✗ validate('6=2*3') should be false"
  exit 1
fi

# ---------------------------------------------------------------------------
# Test 3: Evaluate expressions
# ---------------------------------------------------------------------------
echo ""
echo "============================================================"
echo " Test 3: Expression evaluation"
echo "============================================================"

check_eval() {
  local expr="$1" expected="$2"
  local actual
  actual=$("$BIN" eval "$expr")
  if [ "$actual" = "$expected" ]; then
    echo "  ✓ eval('$expr') = $expected"
  else
    echo "  ✗ eval('$expr') = '$actual', expected '$expected'"
    exit 1
  fi
}

check_eval "5!" "120"
check_eval "5A3" "60"
check_eval "[7/2]" "3"
check_eval "2^10" "1024"
check_eval "2+3*4" "14"

# ---------------------------------------------------------------------------
# Test 4: Top-N mode (length 7, top 10) — verify ordering and count
# ---------------------------------------------------------------------------
echo ""
echo "============================================================"
echo " Test 4: Top-N solve (length=7, top=10)"
echo "============================================================"

cat > "$TMPDIR/p2.json" <<'EOF'
{ "length": 7, "rows": [] }
EOF

TOP_JSON=$("$BIN" solve -i "$TMPDIR/p2.json" -t 0 -f json --top 10 2>&1 || true)
echo "  Top-N solve output (first 200 chars): ${TOP_JSON:0:200}..."

TOP_COUNT=$(echo "$TOP_JSON" | python3 -c "import json,sys; print(len(json.load(sys.stdin)['solutions']))" 2>/dev/null || echo "0")
if [ "$TOP_COUNT" = "10" ]; then
  echo "  ✓ Top-N returned 10 solutions"
else
  echo "  ✗ Top-N returned $TOP_COUNT solutions, expected 10"
  echo "    Full output: $TOP_JSON"
  exit 1
fi

echo ""
echo "============================================================"
echo " ✓ ALL SOLVER LOGIC TESTS PASSED"
echo "============================================================"
echo "The Rust solver code that gets cross-compiled into the Android"
echo "APK's libsumzle_tauri_lib.so produces correct results."
