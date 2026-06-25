#!/usr/bin/env bash
# Extracted from the `benchmark` job in .github/workflows/ci.yml.
# GitHub's default shell is `bash -eo pipefail`; match it so error
# behavior is identical to the former inline step.
set -eo pipefail

BASELINE_BIN="$RUNNER_TEMP/baseline-target/release/sumzle-solver"

> baseline-bench-results.txt
for len in 3 4 5 6 7 8; do
  echo "Running baseline CLI benchmark for length $len..."
  OUTPUT=$($BASELINE_BIN bench -l $len 2>&1 || true)
  echo "$OUTPUT" >> baseline-benchmark-output.txt

  SOL=$(echo "$OUTPUT" | sed -n 's/.*: \([0-9]*\) solutions.*/\1/p')
  SRC=$(echo "$OUTPUT" | sed -n 's/.* \([0-9]\+\) searched.*/\1/p')
  TIME_RAW=$(echo "$OUTPUT" | sed -n 's/.*searched,[[:space:]]*\(.*\)/\1/p')

  if [ -n "$SOL" ]; then
    echo "${len}:${SOL}:${SRC}:${TIME_RAW}" >> baseline-bench-results.txt
  fi
done

> baseline-parallel-bench-results.txt
# Check if baseline binary supports --parallel flag
if $BASELINE_BIN bench --help 2>&1 | grep -q -- '--parallel'; then
  for len in 3 4 5 6 7 8; do
    echo "Running baseline parallel CLI benchmark for length $len..."
    OUTPUT=$($BASELINE_BIN bench -l $len --parallel 2>&1 || true)
    echo "$OUTPUT" >> baseline-parallel-benchmark-output.txt

    SOL=$(echo "$OUTPUT" | sed -n 's/.*: \([0-9]*\) solutions.*/\1/p')
    SRC=$(echo "$OUTPUT" | sed -n 's/.* \([0-9]\+\) searched.*/\1/p')
    TIME_RAW=$(echo "$OUTPUT" | sed -n 's/.*searched,[[:space:]]*\(.*\)/\1/p')

    if [ -n "$SOL" ]; then
      echo "${len}:${SOL}:${SRC}:${TIME_RAW}" >> baseline-parallel-bench-results.txt
    fi
  done
else
  echo "Baseline binary does not support --parallel, skipping baseline parallel benchmarks."
fi
