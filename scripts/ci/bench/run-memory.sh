#!/usr/bin/env bash
# Extracted from the `benchmark` job in .github/workflows/ci.yml.
# GitHub's default shell is `bash -eo pipefail`; match it so error
# behavior is identical to the former inline step.
set -eo pipefail

echo "## 📊 Memory Benchmarks — Peak RSS by Mode" >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo "> Peak resident set size (VmHWM) per (length, mode). Lengths 6–8 chosen so the default in-memory path stays feasible on the runner." >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY

> current-mem-bench-results.txt
> mem-benchmark-output.txt
for len in 6 7 8; do
  for mode in default streaming top; do
    echo "Running memory benchmark: L=$len mode=$mode..."
    # --quiet suppresses the solver's stdout (which can be hundreds of
    # MB in default -f text mode at L8) so the captured OUTPUT holds
    # only the final PEAK_RSS_KB=... line.
    if [ "$mode" = "default" ]; then
      OUTPUT=$(python3 scripts/ci/bench/measure_rss.py --quiet ./target/release/sumzle-solver solve -i puzzle-len${len}.json -t 0 -f text 2>&1 || true)
    elif [ "$mode" = "streaming" ]; then
      OUTPUT=$(python3 scripts/ci/bench/measure_rss.py --quiet ./target/release/sumzle-solver solve -i puzzle-len${len}.json -t 0 --output "mem-stream-len${len}.jsonl" 2>&1 || true)
    else
      OUTPUT=$(python3 scripts/ci/bench/measure_rss.py --quiet ./target/release/sumzle-solver solve -i puzzle-len${len}.json -t 0 --top 100 -f text 2>&1 || true)
    fi
    echo "$OUTPUT" >> mem-benchmark-output.txt

    PEAK=$(echo "$OUTPUT" | sed -n 's/^PEAK_RSS_KB=\([0-9]*\).*/\1/p')
    WALL=$(echo "$OUTPUT" | sed -n 's/.*WALL_MS=\([0-9]*\).*/\1/p')
    if [ -n "$PEAK" ]; then
      echo "${len}:${mode}:${PEAK}:${WALL}" >> current-mem-bench-results.txt
    fi
  done
done

echo "| Length | Mode | Peak RSS | Wall Time |" >> $GITHUB_STEP_SUMMARY
echo "|--------|------|----------|-----------|" >> $GITHUB_STEP_SUMMARY
while IFS=: read -r LEN MODE PEAK WALL; do
  if [ "$PEAK" -ge 1048576 ]; then
    RSS_HR="$(awk "BEGIN {printf \"%.1f GB\", $PEAK/1048576}")"
  elif [ "$PEAK" -ge 1024 ]; then
    RSS_HR="$(awk "BEGIN {printf \"%.1f MB\", $PEAK/1024}")"
  else
    RSS_HR="$PEAK KB"
  fi
  echo "| $LEN | $MODE | $RSS_HR | ${WALL}ms |" >> $GITHUB_STEP_SUMMARY
done < current-mem-bench-results.txt
echo "" >> $GITHUB_STEP_SUMMARY

echo "<details>" >> $GITHUB_STEP_SUMMARY
echo "<summary>📝 Raw Memory Benchmark Output</summary>" >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo '```' >> $GITHUB_STEP_SUMMARY
cat mem-benchmark-output.txt >> $GITHUB_STEP_SUMMARY
echo '```' >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo "</details>" >> $GITHUB_STEP_SUMMARY
