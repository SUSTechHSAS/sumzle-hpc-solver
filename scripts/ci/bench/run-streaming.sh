#!/usr/bin/env bash
# Extracted from the `benchmark` job in .github/workflows/ci.yml.
# GitHub's default shell is `bash -eo pipefail`; match it so error
# behavior is identical to the former inline step.
set -eo pipefail

echo "## 📊 Streaming Solver Benchmarks" >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo "> All solutions streamed to a JSONL file. Memory is bounded (~constant RSS) regardless of total solution count." >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY

> current-stream-bench-results.txt
> stream-benchmark-output.txt
for len in 6 7 8 9; do
  echo "Running streaming benchmark: L=$len..."
  OUTPUT=$(./target/release/sumzle-solver solve -i puzzle-len${len}.json -t 0 --output "stream-len${len}.jsonl" 2>&1 || true)
  echo "$OUTPUT" >> stream-benchmark-output.txt

  SOL=$(echo "$OUTPUT" | sed -n 's/^Solutions found: \([0-9]*\).*/\1/p')
  SRC=$(echo "$OUTPUT" | sed -n 's/^Expressions searched: \([0-9]*\).*/\1/p')
  TIME_MS=$(echo "$OUTPUT" | sed -n 's/^Time: \([0-9]*\)ms.*/\1/p')
  SPEED=$(echo "$OUTPUT" | sed -n 's/^Speed: \([0-9]*\) expr\/s.*/\1/p')
  FILE_BYTES=$(stat -c %s "stream-len${len}.jsonl" 2>/dev/null || echo 0)

  if [ -n "$SOL" ]; then
    echo "${len}:${SOL}:${SRC}:${TIME_MS}:${SPEED}:${FILE_BYTES}" >> current-stream-bench-results.txt
  fi
done

echo "| Length | Solutions | Expr Searched | Time (ms) | Speed (expr/s) | Output Size |" >> $GITHUB_STEP_SUMMARY
echo "|--------|-----------|---------------|-----------|----------------|-------------|" >> $GITHUB_STEP_SUMMARY
while IFS=: read -r LEN SOL SRC TIME_MS SPEED FILE_BYTES; do
  if [ "$FILE_BYTES" -ge 1048576 ]; then
    SIZE_HR="$(awk "BEGIN {printf \"%.1f MB\", $FILE_BYTES/1048576}")"
  elif [ "$FILE_BYTES" -ge 1024 ]; then
    SIZE_HR="$(awk "BEGIN {printf \"%.1f KB\", $FILE_BYTES/1024}")"
  else
    SIZE_HR="$FILE_BYTES B"
  fi
  echo "| $LEN | $SOL | $SRC | $TIME_MS | $SPEED | $SIZE_HR |" >> $GITHUB_STEP_SUMMARY
done < current-stream-bench-results.txt
echo "" >> $GITHUB_STEP_SUMMARY

echo "<details>" >> $GITHUB_STEP_SUMMARY
echo "<summary>📝 Raw Streaming Benchmark Output</summary>" >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo '```' >> $GITHUB_STEP_SUMMARY
cat stream-benchmark-output.txt >> $GITHUB_STEP_SUMMARY
echo '```' >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo "</details>" >> $GITHUB_STEP_SUMMARY
