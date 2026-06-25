#!/usr/bin/env bash
# Extracted from the `benchmark` job in .github/workflows/ci.yml.
# GitHub's default shell is `bash -eo pipefail`; match it so error
# behavior is identical to the former inline step.
set -eo pipefail

BASELINE_BIN="$RUNNER_TEMP/baseline-target/release/sumzle-solver"
> baseline-stream-bench-results.txt
> baseline-stream-benchmark-output.txt
if $BASELINE_BIN solve --help 2>&1 | grep -q -- '--output'; then
  for len in 6 7 8 9; do
    echo "Running baseline streaming: L=$len..."
    OUTPUT=$($BASELINE_BIN solve -i puzzle-len${len}.json -t 0 --output "baseline-stream-len${len}.jsonl" 2>&1 || true)
    echo "$OUTPUT" >> baseline-stream-benchmark-output.txt

    SOL=$(echo "$OUTPUT" | sed -n 's/^Solutions found: \([0-9]*\).*/\1/p')
    SRC=$(echo "$OUTPUT" | sed -n 's/^Expressions searched: \([0-9]*\).*/\1/p')
    TIME_MS=$(echo "$OUTPUT" | sed -n 's/^Time: \([0-9]*\)ms.*/\1/p')
    SPEED=$(echo "$OUTPUT" | sed -n 's/^Speed: \([0-9]*\) expr\/s.*/\1/p')
    FILE_BYTES=$(stat -c %s "baseline-stream-len${len}.jsonl" 2>/dev/null || echo 0)
    if [ -n "$SOL" ]; then
      echo "${len}:${SOL}:${SRC}:${TIME_MS}:${SPEED}:${FILE_BYTES}" >> baseline-stream-bench-results.txt
    fi
  done
else
  echo "Baseline binary does not support --output, skipping baseline streaming benchmarks." >> baseline-stream-benchmark-output.txt
fi
