#!/usr/bin/env bash
# Extracted from the `benchmark` job in .github/workflows/ci.yml.
# GitHub's default shell is `bash -eo pipefail`; match it so error
# behavior is identical to the former inline step.
set -eo pipefail

# No-constraint puzzles of each length — the solver enumerates
# every valid expression so default / streaming / top-N all run
# against identical workloads.
for len in 6 7 8 9; do
  printf '{"length":%d,"rows":[]}\n' "$len" > "puzzle-len${len}.json"
done
