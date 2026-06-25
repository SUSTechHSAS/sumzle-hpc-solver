#!/usr/bin/env bash
# Extracted from the `benchmark` job in .github/workflows/ci.yml.
# GitHub's default shell is `bash -eo pipefail`; match it so error
# behavior is identical to the former inline step.
set -eo pipefail

echo "## 📊 Top-N Solver Benchmarks" >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo "> Bounded min-heap of the N highest-probability solutions. Memory is O(threads·N) regardless of total solution count." >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY

> current-topn-bench-results.txt
> topn-benchmark-output.txt
for len in 6 7 8 9; do
  for n in 100 1000; do
    echo "Running top-N benchmark: L=$len N=$n..."
    OUTPUT=$(./target/release/sumzle-solver solve -i puzzle-len${len}.json -t 0 --top $n -f text 2>&1 || true)
    echo "$OUTPUT" >> topn-benchmark-output.txt

    SOL=$(echo "$OUTPUT" | sed -n 's/^Solutions found: \([0-9]*\).*/\1/p')
    SRC=$(echo "$OUTPUT" | sed -n 's/^Expressions searched: \([0-9]*\).*/\1/p')
    TIME_MS=$(echo "$OUTPUT" | sed -n 's/^Time: \([0-9]*\)ms.*/\1/p')
    SPEED=$(echo "$OUTPUT" | sed -n 's/^Speed: \([0-9]*\) expr\/s.*/\1/p')

    if [ -n "$SOL" ]; then
      echo "${len}:${n}:${SOL}:${SRC}:${TIME_MS}:${SPEED}" >> current-topn-bench-results.txt
    fi
  done
done

echo "| Length | N | Solutions (kept) | Expr Searched | Time (ms) | Speed (expr/s) |" >> $GITHUB_STEP_SUMMARY
echo "|--------|------|------------------|---------------|-----------|----------------|" >> $GITHUB_STEP_SUMMARY
while IFS=: read -r LEN N SOL SRC TIME_MS SPEED; do
  echo "| $LEN | $N | $SOL | $SRC | $TIME_MS | $SPEED |" >> $GITHUB_STEP_SUMMARY
done < current-topn-bench-results.txt
echo "" >> $GITHUB_STEP_SUMMARY

echo "<details>" >> $GITHUB_STEP_SUMMARY
echo "<summary>📝 Raw Top-N Benchmark Output</summary>" >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo '```' >> $GITHUB_STEP_SUMMARY
cat topn-benchmark-output.txt >> $GITHUB_STEP_SUMMARY
echo '```' >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo "</details>" >> $GITHUB_STEP_SUMMARY
