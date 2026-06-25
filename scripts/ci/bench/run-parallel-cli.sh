#!/usr/bin/env bash
# Extracted from the `benchmark` job in .github/workflows/ci.yml.
# GitHub's default shell is `bash -eo pipefail`; match it so error
# behavior is identical to the former inline step.
set -eo pipefail

echo "## 📊 CLI Parallel Solver Benchmarks" >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo "> Parallel solver uses auto-detected thread count (num_cpus)." >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY

> current-parallel-bench-results.txt
for len in 3 4 5 6 7 8; do
  echo "Running parallel CLI benchmark for length $len..."
  OUTPUT=$(./target/release/sumzle-solver bench -l $len --parallel 2>&1 || true)
  echo "$OUTPUT" >> cli-parallel-benchmark-output.txt

  SOL=$(echo "$OUTPUT" | sed -n 's/.*: \([0-9]*\) solutions.*/\1/p')
  SRC=$(echo "$OUTPUT" | sed -n 's/.* \([0-9]\+\) searched.*/\1/p')
  TIME_RAW=$(echo "$OUTPUT" | sed -n 's/.*searched,[[:space:]]*\(.*\)/\1/p')

  if [ -n "$SOL" ]; then
    echo "${len}:${SOL}:${SRC}:${TIME_RAW}" >> current-parallel-bench-results.txt
  fi
done

# Build summary table
echo "| Length | Solutions | Expressions Searched | Time |" >> $GITHUB_STEP_SUMMARY
echo "|--------|-----------|---------------------|------|" >> $GITHUB_STEP_SUMMARY

while IFS=: read -r LEN SOL SRC TIME_RAW; do
  echo "| $LEN | $SOL | $SRC | $TIME_RAW |" >> $GITHUB_STEP_SUMMARY
done < current-parallel-bench-results.txt

echo "" >> $GITHUB_STEP_SUMMARY

echo "<details>" >> $GITHUB_STEP_SUMMARY
echo "<summary>📝 Raw Parallel CLI Benchmark Output</summary>" >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo '```' >> $GITHUB_STEP_SUMMARY
cat cli-parallel-benchmark-output.txt >> $GITHUB_STEP_SUMMARY
echo '```' >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo "</details>" >> $GITHUB_STEP_SUMMARY
