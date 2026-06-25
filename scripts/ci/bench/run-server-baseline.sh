#!/usr/bin/env bash
# Extracted from the `benchmark` job in .github/workflows/ci.yml.
# GitHub's default shell is `bash -eo pipefail`; match it so error
# behavior is identical to the former inline step.
set -eo pipefail

BASELINE_BIN="$RUNNER_TEMP/baseline-target/release/sumzle-solver"
PORT=3030
"$BASELINE_BIN" serve --host 127.0.0.1 --port $PORT >baseline-server.log 2>&1 &
SERVER_PID=$!

READY=0
for _ in $(seq 1 60); do
  CODE=$(curl -s -o /dev/null -w '%{http_code}' \
    -X POST "http://127.0.0.1:${PORT}/api/solve?threads=1" \
    -H 'Content-Type: application/json' \
    -d '{"length":3,"rows":[]}' || true)
  if [ "$CODE" = "200" ]; then READY=1; break; fi
  sleep 0.5
done

> baseline-server-bench-results.txt
> baseline-server-benchmark-output.txt
if [ "$READY" = "1" ]; then
  for len in 4 5 6; do
    echo "Running baseline server multi-solve benchmark: L=$len..."
    python3 scripts/ci/bench/bench_server.py \
      --url "http://127.0.0.1:${PORT}" --pid "$SERVER_PID" \
      --length "$len" --requests 40 --concurrency 4 --threads 1 --warmup 5 \
      >> baseline-server-bench-results.txt 2>> baseline-server-benchmark-output.txt || true
  done
else
  echo "Baseline server did not become ready; skipping baseline server benchmarks." | tee -a baseline-server-benchmark-output.txt
fi

kill "$SERVER_PID" 2>/dev/null || true
wait "$SERVER_PID" 2>/dev/null || true
