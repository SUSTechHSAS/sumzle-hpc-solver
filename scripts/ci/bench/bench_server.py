#!/usr/bin/env python3
"""Benchmark a running sumzle-solver server with concurrent /api/solve requests.

Fires --warmup untimed requests, then --requests timed POST /api/solve requests
at --concurrency (each request uses ?threads=N), all for one puzzle length.
Reports throughput, mean/p50/p95 latency, the server's peak resident set (VmHWM
from /proc/<pid>/status), and a correctness check that every response returns
identical found_count/searched_count (the search is deterministic, so they must).

Machine-parseable line on stdout (consumed by the workflow):
  L:requests:concurrency:throughput:mean_ms:p50_ms:p95_ms:peak_rss_kb:found:searched:errors
Human-readable detail goes to stderr.
"""
import argparse
import json
import math
import sys
import time
import urllib.request
from concurrent.futures import ThreadPoolExecutor


def solve_once(url, length, threads, timeout):
    """One POST /api/solve. Returns (latency_s, found, searched, ok)."""
    body = json.dumps({"length": length, "rows": []}).encode()
    req = urllib.request.Request(
        f"{url}/api/solve?threads={threads}",
        data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    start = time.monotonic()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            payload = json.loads(resp.read())
        latency = time.monotonic() - start
        stats = payload.get("stats", {})
        return (
            latency,
            int(stats.get("found_count", -1)),
            int(stats.get("searched_count", -1)),
            True,
        )
    except Exception as e:  # noqa: BLE001 - any failure counts as a failed request
        latency = time.monotonic() - start
        print(f"  request error: {e}", file=sys.stderr)
        return (latency, -1, -1, False)


def read_vmhwm_kb(pid):
    """Peak resident set (VmHWM, KB) of the server process, or 0 if unavailable."""
    if not pid:
        return 0
    try:
        with open(f"/proc/{pid}/status") as f:
            for line in f:
                if line.startswith("VmHWM:"):
                    return int(line.split()[1])
    except (FileNotFoundError, ProcessLookupError, ValueError, IndexError):
        pass
    return 0


def percentile(sorted_vals, pct):
    """Nearest-rank percentile of an already-sorted list."""
    if not sorted_vals:
        return 0.0
    rank = math.ceil(pct / 100.0 * len(sorted_vals))
    k = min(max(rank, 1), len(sorted_vals)) - 1
    return sorted_vals[k]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--url", default="http://127.0.0.1:3030")
    ap.add_argument("--pid", type=int, default=0)
    ap.add_argument("--length", type=int, required=True)
    ap.add_argument("--requests", type=int, default=40)
    ap.add_argument("--concurrency", type=int, default=4)
    ap.add_argument("--threads", type=int, default=1)
    ap.add_argument("--warmup", type=int, default=5)
    ap.add_argument("--timeout", type=float, default=120.0)
    args = ap.parse_args()

    def fire(_i):
        return solve_once(args.url, args.length, args.threads, args.timeout)

    # Warmup (untimed): let mimalloc/Rayon reach steady state before measuring.
    if args.warmup > 0:
        with ThreadPoolExecutor(max_workers=args.concurrency) as ex:
            list(ex.map(fire, range(args.warmup)))

    # Timed run.
    wall_start = time.monotonic()
    with ThreadPoolExecutor(max_workers=args.concurrency) as ex:
        results = list(ex.map(fire, range(args.requests)))
    wall = time.monotonic() - wall_start

    latencies = sorted(r[0] for r in results)
    founds = {r[1] for r in results if r[3]}
    searcheds = {r[2] for r in results if r[3]}
    errors = sum(1 for r in results if not r[3])

    # Correctness: deterministic search => every OK response must agree.
    if len(founds) > 1 or len(searcheds) > 1:
        errors += 1
        print(f"  MISMATCH: found={founds} searched={searcheds}", file=sys.stderr)

    found = min(founds) if founds else -1
    searched = min(searcheds) if searcheds else -1
    throughput = (args.requests / wall) if wall > 0 else 0.0
    mean_ms = (sum(latencies) / len(latencies) * 1000) if latencies else 0.0
    p50_ms = percentile(latencies, 50) * 1000
    p95_ms = percentile(latencies, 95) * 1000
    peak = read_vmhwm_kb(args.pid)

    print(
        f"L={args.length} requests={args.requests} concurrency={args.concurrency} "
        f"threads={args.threads}: throughput={throughput:.1f} req/s "
        f"mean={mean_ms:.1f}ms p50={p50_ms:.1f}ms p95={p95_ms:.1f}ms "
        f"peak_rss={peak} KB found={found} searched={searched} errors={errors}",
        file=sys.stderr,
    )

    # Machine-parseable single line on stdout.
    print(
        f"{args.length}:{args.requests}:{args.concurrency}:{throughput:.1f}:"
        f"{mean_ms:.1f}:{p50_ms:.1f}:{p95_ms:.1f}:{peak}:{found}:{searched}:{errors}"
    )


if __name__ == "__main__":
    main()
