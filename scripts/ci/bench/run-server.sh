#!/usr/bin/env bash
# Extracted from the `benchmark` job in .github/workflows/ci.yml.
# GitHub's default shell is `bash -eo pipefail`; match it so error
# behavior is identical to the former inline step.
set -eo pipefail

echo "## 📊 Server Multi-Solve Benchmarks" >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo "> Concurrent \`POST /api/solve\` requests (each \`threads=1\`, so concurrency drives load) against the long-running \`serve\` process. Exercises the HTTP path and cross-request memory stability (issue #20 / mimalloc). Server peak RSS is VmHWM sampled after the run." >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY

PORT=3030
./target/release/sumzle-solver serve --host 127.0.0.1 --port $PORT >server.log 2>&1 &
SERVER_PID=$!

# Wait until the server accepts requests. There is no health endpoint,
# so probe /api/solve with a trivial length-3 puzzle until it 200s.
READY=0
for _ in $(seq 1 60); do
  CODE=$(curl -s -o /dev/null -w '%{http_code}' \
    -X POST "http://127.0.0.1:${PORT}/api/solve?threads=1" \
    -H 'Content-Type: application/json' \
    -d '{"length":3,"rows":[]}' || true)
  if [ "$CODE" = "200" ]; then READY=1; break; fi
  sleep 0.5
done

> current-server-bench-results.txt
> server-benchmark-output.txt
if [ "$READY" = "1" ]; then
  for len in 4 5 6; do
    echo "Running server multi-solve benchmark: L=$len..."
    python3 scripts/ci/bench/bench_server.py \
      --url "http://127.0.0.1:${PORT}" --pid "$SERVER_PID" \
      --length "$len" --requests 40 --concurrency 4 --threads 1 --warmup 5 \
      >> current-server-bench-results.txt 2>> server-benchmark-output.txt || true
  done
else
  echo "Server did not become ready on port ${PORT}; skipping server benchmarks." | tee -a server-benchmark-output.txt
fi

kill "$SERVER_PID" 2>/dev/null || true
wait "$SERVER_PID" 2>/dev/null || true

# Summary table.
echo "| Length | Requests | Concurrency | Throughput (req/s) | Mean (ms) | p95 (ms) | Server Peak RSS | Errors |" >> $GITHUB_STEP_SUMMARY
echo "|--------|----------|-------------|--------------------|-----------|----------|-----------------|--------|" >> $GITHUB_STEP_SUMMARY
while IFS=: read -r LEN REQ CONC TP MEAN P50 P95 PEAK FOUND SRCH ERRS; do
  if [ "$PEAK" -ge 1048576 ]; then
    RSS_HR="$(awk "BEGIN {printf \"%.2f GB\", $PEAK/1048576}")"
  elif [ "$PEAK" -ge 1024 ]; then
    RSS_HR="$(awk "BEGIN {printf \"%.1f MB\", $PEAK/1024}")"
  else
    RSS_HR="$PEAK KB"
  fi
  echo "| $LEN | $REQ | $CONC | $TP | $MEAN | $P95 | $RSS_HR | $ERRS |" >> $GITHUB_STEP_SUMMARY
done < current-server-bench-results.txt
echo "" >> $GITHUB_STEP_SUMMARY

echo "<details>" >> $GITHUB_STEP_SUMMARY
echo "<summary>📝 Raw Server Benchmark Output</summary>" >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo '```' >> $GITHUB_STEP_SUMMARY
cat server-benchmark-output.txt >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo "--- server.log (tail) ---" >> $GITHUB_STEP_SUMMARY
tail -n 20 server.log >> $GITHUB_STEP_SUMMARY 2>/dev/null || true
echo '```' >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo "</details>" >> $GITHUB_STEP_SUMMARY
