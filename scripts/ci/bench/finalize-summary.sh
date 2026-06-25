#!/usr/bin/env bash
# Extracted from the `benchmark` job in .github/workflows/ci.yml.
# GitHub's default shell is `bash -eo pipefail`; match it so error
# behavior is identical to the former inline step.
set -eo pipefail

echo "" >> $GITHUB_STEP_SUMMARY
echo "> ℹ️ Benchmarks run on GitHub Actions \`ubuntu-latest\` runner. Results may vary due to shared hardware." >> $GITHUB_STEP_SUMMARY
