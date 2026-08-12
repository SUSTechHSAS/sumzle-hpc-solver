#!/usr/bin/env bash
# Consistency harness for the Sumzle solver.
#
# Compares the CURRENT build against golden baseline artifacts captured from
# the unmodified upstream tree. Every optimization must keep ALL of these
# byte-identical:
#
#   * the full solution set (sorted) for each length / mode
#   * found_count and searched_count statistics
#   * single-threaded DFS emission order
#   * parallel == sequential, streaming == in-memory, top-N ranking
#
# Usage:
#   scripts/consistency-check.sh record   # capture goldens (run on baseline)
#   scripts/consistency-check.sh check    # verify current build vs goldens

set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${BIN:-$REPO/target/release/sumzle-solver}"
GOLD="${GOLD:-$REPO/.consistency}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

MODE="${1:-check}"
FAIL=0
PASS=0

# Lengths exercised for full-set comparison. 3..8 covers every structural
# feature (factorial, permutation, floor, brackets, negative RHS) at a size
# that still completes quickly.
LENGTHS_FULL="3 4 5 6 7 8"

# Constrained puzzles: fixed / present / absent tiles, which take the solver
# down the non-unconstrained code paths that the fast path must not alter.
mk_constrained() {
  cat > "$WORK/c1.json" <<'JSON'
{"length":6,"rows":[{"tiles":[
  {"char":"1","state":"correct"},
  {"char":"+","state":"present"},
  {"char":"2","state":"empty"},
  {"char":"=","state":"correct"},
  {"char":"3","state":"empty"},
  {"char":"0","state":"empty"}]}]}
JSON
  cat > "$WORK/c2.json" <<'JSON'
{"length":7,"rows":[{"tiles":[
  {"char":"5","state":"present"},
  {"char":"*","state":"present"},
  {"char":"9","state":"empty"},
  {"char":">","state":"empty"},
  {"char":"4","state":"empty"},
  {"char":"7","state":"empty"},
  {"char":"1","state":"present"}]}]}
JSON
  cat > "$WORK/c3.json" <<'JSON'
{"length":8,"rows":[{"tiles":[
  {"char":"3","state":"empty"},
  {"char":"!","state":"present"},
  {"char":"=","state":"present"},
  {"char":"6","state":"empty"},
  {"char":"(","state":"empty"},
  {"char":")","state":"empty"},
  {"char":"^","state":"empty"},
  {"char":"2","state":"empty"}]}]}
JSON
  cat > "$WORK/c4.json" <<'JSON'
{"length":7,"rows":[{"tiles":[
  {"char":"[","state":"present"},
  {"char":"7","state":"present"},
  {"char":"/","state":"present"},
  {"char":"2","state":"empty"},
  {"char":"]","state":"present"},
  {"char":"=","state":"empty"},
  {"char":"3","state":"empty"}]}]}
JSON
}

# Timing fields are wall-clock and differ run to run; they carry no
# correctness signal, so they are normalized away before comparison. Everything
# else (solution text, ordering, counts, scores) must match exactly.
normalize() {
  sed -E -e 's/^Time: [0-9]+ms$/Time: <n>ms/' \
         -e 's/^Speed: [0-9]+ expr\/s$/Speed: <n> expr\/s/' \
         -e 's/"elapsed_ms": [0-9]+/"elapsed_ms": <n>/' \
         -e 's/"speed": [0-9]+/"speed": <n>/' \
         -e 's#^Output: .*/([^/]+)$#Output: <tmp>/\1#'
}

emit() { # name, content-producing command
  local name="$1"; shift
  local raw="$WORK/$name.raw"
  local out="$WORK/$name.out"
  if ! "$@" > "$raw" 2>"$WORK/$name.err"; then
    echo "  RUN-FAIL $name"; sed -n '1,5p' "$WORK/$name.err"; FAIL=$((FAIL+1)); return 1
  fi
  normalize < "$raw" > "$out"
  if [ "$MODE" = "record" ]; then
    mkdir -p "$GOLD"; cp "$out" "$GOLD/$name.gold"
    echo "  recorded $name ($(wc -l < "$out") lines)"
  else
    if [ ! -f "$GOLD/$name.gold" ]; then
      echo "  MISSING-GOLD $name"; FAIL=$((FAIL+1)); return 1
    fi
    if cmp -s "$out" "$GOLD/$name.gold"; then
      PASS=$((PASS+1))
    else
      echo "  MISMATCH $name"
      diff <(head -20 "$GOLD/$name.gold") <(head -20 "$out") | head -20
      echo "    gold=$(wc -l < "$GOLD/$name.gold") lines, new=$(wc -l < "$out") lines"
      FAIL=$((FAIL+1))
    fi
  fi
}

mk_constrained
for L in $LENGTHS_FULL; do echo "{\"length\":$L,\"rows\":[]}" > "$WORK/p$L.json"; done

echo "[1] single-threaded full solution set + DFS order + stats"
for L in $LENGTHS_FULL; do
  emit "st-L$L" "$BIN" solve -i "$WORK/p$L.json" -t 1 -f text
done

echo "[2] parallel (32t) full solution set + stats"
for L in $LENGTHS_FULL; do
  emit "par-L$L" "$BIN" solve -i "$WORK/p$L.json" -t 32 -f text
done

echo "[3] parallel with odd thread counts (work-splitting invariance)"
for T in 2 3 7 13; do
  emit "par7-t$T" "$BIN" solve -i "$WORK/p7.json" -t "$T" -f text
done

echo "[4] constrained puzzles (single + parallel)"
for C in c1 c2 c3 c4; do
  emit "cons-$C-t1" "$BIN" solve -i "$WORK/$C.json" -t 1 -f text
  emit "cons-$C-t32" "$BIN" solve -i "$WORK/$C.json" -t 32 -f text
done

echo "[5] streaming NDJSON (sorted; order is unspecified by contract)"
for L in 6 7 8; do
  "$BIN" solve -i "$WORK/p$L.json" -t 32 -o "$WORK/s$L.jsonl" > "$WORK/s$L.stats" 2>/dev/null
  emit "stream-L$L-sorted" sort "$WORK/s$L.jsonl"
  emit "stream-L$L-stats" cat "$WORK/s$L.stats"
done

echo "[6] top-N ranking (scores, order, full-set stats)"
for N in 1 5 25 100; do
  emit "top$N-L7" "$BIN" solve -i "$WORK/p7.json" -t 32 --top "$N" -f json
done
emit "top10-L8" "$BIN" solve -i "$WORK/p8.json" -t 32 --top 10 -f json
emit "top10-c2" "$BIN" solve -i "$WORK/c2.json" -t 32 --top 10 -f json

echo "[7] evaluator / validator surface"
{
  for e in "1+2" "5!" "5A3" "[7/2]" "2^3" "7%3" "1/0" "01" "13!" "2A5" "-5" "3--2" \
           "[10/3]" "(2+3)*(4+1)" "10A2" "12!" "0!" "[5]" "2^10" "9^9" "3^9" "[1/0]" \
           "1--1" "((1+2))" "30*30" "0" "100" "5%0" "2^-1" "[0/1]"; do
    printf "%s => " "$e"; "$BIN" eval "$e"
  done
  for q in "1+2=3" "6=2*3" "3>2" "5>=5" "5>=3" "3>=5" "5!=120" "5A3=60" "[7/2]=3" \
           "3-5=-2" "3-5=-2+1" "1+2" "=3" "5>5" "3!*2=12" "[7/2]*2=6" "2>1!" "3!>0"; do
    printf "%s => " "$q"; "$BIN" validate "$q"
  done
} > "$WORK/evalsurface.txt" 2>&1
emit "eval-surface" cat "$WORK/evalsurface.txt"

echo
if [ "$MODE" = "record" ]; then
  echo "Goldens recorded in $GOLD"
else
  echo "PASS=$PASS FAIL=$FAIL"
  [ "$FAIL" -eq 0 ] || exit 1
  echo "ALL CONSISTENCY CHECKS PASSED"
fi
