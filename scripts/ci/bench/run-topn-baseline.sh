#!/usr/bin/env bash
# Extracted from the `benchmark` job in .github/workflows/ci.yml.
# GitHub's default shell is `bash -eo pipefail`; match it so error
# behavior is identical to the former inline step.
set -eo pipefail

BASELINE_BIN="$RUNNER_TEMP/baseline-target/release/sumzle-solver"
> baseline-topn-bench-results.txt
> baseline-topn-benchmark-output.txt
# Skip if baseline binary doesn't support --top (pre-PR9 main)
if $BASELINE_BIN solve --help 2>&1 | grep -q -- '--top'; then
  for len in 6 7 8 9; do
    for n in 100 1000; do
      echo "Running baseline top-N: L=$len N=$n..."
      OUTPUT=$($BASELINE_BIN solve -i puzzle-len${len}.json -t 0 --top $n -f text 2>&1 || true)
      echo "$OUTPUT" >> baseline-topn-benchmark-output.txt

      SOL=$(echo "$OUTPUT" | sed -n 's/^Solutions found: \([0-9]*\).*/\1/p')
      SRC=$(echo "$OUTPUT" | sed -n 's/^Expressions searched: \([0-9]*\).*/\1/p')
      TIME_MS=$(echo "$OUTPUT" | sed -n 's/^Time: \([0-9]*\)ms.*/\1/p')
      SPEED=$(echo "$OUTPUT" | sed -n 's/^Speed: \([0-9]*\) expr\/s.*/\1/p')
      if [ -n "$SOL" ]; then
        echo "${len}:${n}:${SOL}:${SRC}:${TIME_MS}:${SPEED}" >> baseline-topn-bench-results.txt
      fi
    done
  done
else
  echo "Baseline binary does not support --top, skipping baseline top-N benchmarks." >> baseline-topn-benchmark-output.txt
fi
