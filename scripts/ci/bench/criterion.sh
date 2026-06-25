#!/usr/bin/env bash
# Extracted from the `benchmark` job in .github/workflows/ci.yml.
# GitHub's default shell is `bash -eo pipefail`; match it so error
# behavior is identical to the former inline step.
set -eo pipefail

echo "## 📈 Criterion Micro-Benchmarks" >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY

# Run all criterion benchmarks with minimal sample count for CI speed.
# Use --bench benchmark to only run the Criterion benchmark target,
# avoiding the lib unit-test harness which doesn't understand --quick.
cargo bench --bench benchmark -- --quick 2>&1 | tee criterion-output.txt || true

# Parse criterion output and build summary table
echo "| Benchmark Group | Benchmark | Mean Time |" >> $GITHUB_STEP_SUMMARY
echo "|----------------|-----------|-----------|" >> $GITHUB_STEP_SUMMARY

# Extract benchmark results from criterion output
# Criterion prints time: [lower estimate upper] (three values + units).
# We want the middle (estimate), so awk field $3 + $4, NOT $1 + $2
# (which would be the lower bound of the confidence interval).
grep -E '^\S+.*time:\s+\[' criterion-output.txt | while IFS= read -r LINE; do
  BENCH_NAME=$(echo "$LINE" | sed -E 's/\s+time:.*//')
  MEAN_TIME=$(echo "$LINE" | sed -E 's/.*time:\s+\[\s*([^]]+)\].*/\1/' | awk '{print $3, $4}')
  GROUP=$(echo "$BENCH_NAME" | cut -d'/' -f1)
  BENCH=$(echo "$BENCH_NAME" | cut -d'/' -f2-)
  echo "| $GROUP | $BENCH | $MEAN_TIME |" >> $GITHUB_STEP_SUMMARY
done

echo "" >> $GITHUB_STEP_SUMMARY

# Add raw output in collapsible section
echo "<details>" >> $GITHUB_STEP_SUMMARY
echo "<summary>📝 Raw Criterion Output</summary>" >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo '```' >> $GITHUB_STEP_SUMMARY
cat criterion-output.txt >> $GITHUB_STEP_SUMMARY
echo '```' >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo "</details>" >> $GITHUB_STEP_SUMMARY
