#!/usr/bin/env bash
# Extracted from the `benchmark` job in .github/workflows/ci.yml.
# GitHub's default shell is `bash -eo pipefail`; match it so error
# behavior is identical to the former inline step.
set -eo pipefail

BASELINE_BIN="$RUNNER_TEMP/baseline-target/release/sumzle-solver"
> baseline-mem-bench-results.txt
> baseline-mem-benchmark-output.txt
if $BASELINE_BIN solve --help 2>&1 | grep -q -- '--top' && \
   $BASELINE_BIN solve --help 2>&1 | grep -q -- '--output'; then
  for len in 6 7 8; do
    for mode in default streaming top; do
      echo "Running baseline memory: L=$len mode=$mode..."
      if [ "$mode" = "default" ]; then
        OUTPUT=$(python3 scripts/ci/bench/measure_rss.py --quiet $BASELINE_BIN solve -i puzzle-len${len}.json -t 0 -f text 2>&1 || true)
      elif [ "$mode" = "streaming" ]; then
        OUTPUT=$(python3 scripts/ci/bench/measure_rss.py --quiet $BASELINE_BIN solve -i puzzle-len${len}.json -t 0 --output "baseline-mem-stream-len${len}.jsonl" 2>&1 || true)
      else
        OUTPUT=$(python3 scripts/ci/bench/measure_rss.py --quiet $BASELINE_BIN solve -i puzzle-len${len}.json -t 0 --top 100 -f text 2>&1 || true)
      fi
      echo "$OUTPUT" >> baseline-mem-benchmark-output.txt

      PEAK=$(echo "$OUTPUT" | sed -n 's/^PEAK_RSS_KB=\([0-9]*\).*/\1/p')
      WALL=$(echo "$OUTPUT" | sed -n 's/.*WALL_MS=\([0-9]*\).*/\1/p')
      if [ -n "$PEAK" ]; then
        echo "${len}:${mode}:${PEAK}:${WALL}" >> baseline-mem-bench-results.txt
      fi
    done
  done
else
  echo "Baseline binary does not support --top/--output, skipping baseline memory benchmarks." >> baseline-mem-benchmark-output.txt
fi
