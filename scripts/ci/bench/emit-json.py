#!/usr/bin/env python3
"""Emit a structured benchmark JSON artifact from CI text result files."""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


TIME_RE = re.compile(r"^([0-9.]+)\s*(ns|µs|μs|us|ms|s)$")
TIME_MULTIPLIERS = {"ns": 1, "µs": 1_000, "μs": 1_000, "us": 1_000, "ms": 1_000_000, "s": 1_000_000_000}


def parse_time_ns(value: str) -> float | None:
    match = TIME_RE.match(value.strip())
    if not match:
        return None
    try:
        return float(match.group(1)) * TIME_MULTIPLIERS[match.group(2)]
    except ValueError:
        return None


def read_lines(path: Path) -> list[str]:
    try:
        return [line.strip() for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
    except FileNotFoundError:
        return []


def run_text(args: list[str]) -> str:
    try:
        return subprocess.check_output(args, text=True, stderr=subprocess.DEVNULL).strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        return ""


def parse_ints(*values: str) -> tuple[int, ...] | None:
    try:
        return tuple(int(value) for value in values)
    except ValueError:
        return None


def parse_floats(*values: str) -> tuple[float, ...] | None:
    try:
        return tuple(float(value) for value in values)
    except ValueError:
        return None


def metric(
    suite: str,
    name: str,
    value: int | float,
    unit: str,
    params: dict[str, Any] | None = None,
    lower_is_better: bool = True,
) -> dict[str, Any]:
    return {
        "suite": suite,
        "metric": name,
        "value": value,
        "unit": unit,
        "params": params or {},
        "lower_is_better": lower_is_better,
    }


def parse_cli(path: Path, suite: str) -> list[dict[str, Any]]:
    metrics: list[dict[str, Any]] = []
    for line in read_lines(path):
        parts = line.split(":", 3)
        if len(parts) != 4:
            continue
        length, solutions, searched, time_raw = parts
        parsed = parse_ints(length, solutions, searched)
        if parsed is None:
            continue
        len_val, sol_val, search_val = parsed
        params = {"length": len_val}
        time_ns = parse_time_ns(time_raw)
        metrics.append(metric(suite, "solutions", sol_val, "count", params, lower_is_better=False))
        metrics.append(metric(suite, "expressions_searched", search_val, "count", params, lower_is_better=False))
        if time_ns is not None:
            metrics.append(metric(suite, "time", time_ns, "ns", params))
    return metrics


def parse_topn(path: Path) -> list[dict[str, Any]]:
    metrics: list[dict[str, Any]] = []
    for line in read_lines(path):
        parts = line.split(":")
        if len(parts) != 6:
            continue
        parsed = parse_ints(*parts)
        if parsed is None:
            continue
        length, n, solutions, searched, time_ms, speed = parsed
        params = {"length": length, "n": n}
        metrics.append(metric("topn", "solutions_kept", solutions, "count", params, lower_is_better=False))
        metrics.append(metric("topn", "expressions_searched", searched, "count", params, lower_is_better=False))
        metrics.append(metric("topn", "time", time_ms, "ms", params))
        metrics.append(metric("topn", "speed", speed, "expr/s", params, lower_is_better=False))
    return metrics


def parse_streaming(path: Path) -> list[dict[str, Any]]:
    metrics: list[dict[str, Any]] = []
    for line in read_lines(path):
        parts = line.split(":")
        if len(parts) != 6:
            continue
        parsed = parse_ints(*parts)
        if parsed is None:
            continue
        length, solutions, searched, time_ms, speed, file_bytes = parsed
        params = {"length": length}
        metrics.append(metric("streaming", "solutions", solutions, "count", params, lower_is_better=False))
        metrics.append(metric("streaming", "expressions_searched", searched, "count", params, lower_is_better=False))
        metrics.append(metric("streaming", "time", time_ms, "ms", params))
        metrics.append(metric("streaming", "speed", speed, "expr/s", params, lower_is_better=False))
        metrics.append(metric("streaming", "output_size", file_bytes, "bytes", params))
    return metrics


def parse_memory(path: Path) -> list[dict[str, Any]]:
    metrics: list[dict[str, Any]] = []
    for line in read_lines(path):
        parts = line.split(":")
        if len(parts) != 4:
            continue
        length, mode, peak_rss_kb, wall_ms = parts
        parsed = parse_ints(length, peak_rss_kb, wall_ms)
        if parsed is None:
            continue
        len_val, peak_val, wall_val = parsed
        params = {"length": len_val, "mode": mode}
        metrics.append(metric("memory", "peak_rss", peak_val, "KiB", params))
        metrics.append(metric("memory", "wall_time", wall_val, "ms", params))
    return metrics


def parse_server(path: Path) -> list[dict[str, Any]]:
    metrics: list[dict[str, Any]] = []
    for line in read_lines(path):
        parts = line.split(":")
        if len(parts) != 11:
            continue
        length, requests, concurrency, throughput, mean_ms, p50_ms, p95_ms, peak_rss_kb, found, searched, errors = parts
        parsed_ints = parse_ints(length, requests, concurrency, peak_rss_kb, found, searched, errors)
        parsed_floats = parse_floats(throughput, mean_ms, p50_ms, p95_ms)
        if parsed_ints is None or parsed_floats is None:
            continue
        len_val, req_val, conc_val, peak_val, found_val, searched_val, errors_val = parsed_ints
        throughput_val, mean_val, p50_val, p95_val = parsed_floats
        params = {"length": len_val, "requests": req_val, "concurrency": conc_val}
        metrics.append(metric("server", "throughput", throughput_val, "req/s", params, lower_is_better=False))
        metrics.append(metric("server", "mean_latency", mean_val, "ms", params))
        metrics.append(metric("server", "p50_latency", p50_val, "ms", params))
        metrics.append(metric("server", "p95_latency", p95_val, "ms", params))
        metrics.append(metric("server", "peak_rss", peak_val, "KiB", params))
        metrics.append(metric("server", "solutions", found_val, "count", params, lower_is_better=False))
        metrics.append(metric("server", "expressions_searched", searched_val, "count", params, lower_is_better=False))
        metrics.append(metric("server", "errors", errors_val, "count", params))
    return metrics


def parse_criterion(path: Path) -> list[dict[str, Any]]:
    metrics: list[dict[str, Any]] = []
    pattern = re.compile(r"^(\S.*?)\s+time:\s+\[\s*([^\]]+)\]")
    for line in read_lines(path):
        match = pattern.match(line)
        if not match:
            continue
        bench_name, bracket = match.groups()
        fields = bracket.split()
        if len(fields) < 6:
            continue
        # Criterion prints lower, estimate, upper as value/unit pairs.
        estimate_raw = f"{fields[2]}{fields[3]}"
        estimate_ns = parse_time_ns(estimate_raw)
        if estimate_ns is None:
            continue
        group, _, case = bench_name.partition("/")
        metrics.append(
            metric(
                "criterion",
                "mean_time",
                estimate_ns,
                "ns",
                {"group": group, "case": case or group},
            )
        )
    return metrics


def github_context() -> dict[str, Any]:
    event_path = os.environ.get("GITHUB_EVENT_PATH")
    event: dict[str, Any] = {}
    if event_path and Path(event_path).exists():
        event = json.loads(Path(event_path).read_text(encoding="utf-8"))

    pull_request = event.get("pull_request")
    pr_payload = None
    if pull_request:
        pr_payload = {
            "number": pull_request.get("number"),
            "title": pull_request.get("title"),
            "head_sha": pull_request.get("head", {}).get("sha"),
            "head_ref": pull_request.get("head", {}).get("ref"),
            "base_ref": pull_request.get("base", {}).get("ref"),
            "url": pull_request.get("html_url"),
        }

    server_url = os.environ.get("GITHUB_SERVER_URL", "https://github.com")
    repository = os.environ.get("GITHUB_REPOSITORY", "")
    run_id = os.environ.get("GITHUB_RUN_ID", "")
    run_url = f"{server_url}/{repository}/actions/runs/{run_id}" if repository and run_id else ""

    return {
        "repository": repository,
        "event": os.environ.get("GITHUB_EVENT_NAME", "local"),
        "branch": os.environ.get("GITHUB_HEAD_REF") or os.environ.get("GITHUB_REF_NAME") or run_text(["git", "branch", "--show-current"]),
        "sha": os.environ.get("GITHUB_SHA") or run_text(["git", "rev-parse", "HEAD"]),
        "run": {
            "id": run_id,
            "attempt": os.environ.get("GITHUB_RUN_ATTEMPT", ""),
            "url": run_url,
            "workflow": os.environ.get("GITHUB_WORKFLOW", ""),
        },
        "pull_request": pr_payload,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", default="benchmark-result.json")
    args = parser.parse_args()

    cwd = Path.cwd()
    metrics: list[dict[str, Any]] = []
    metrics.extend(parse_cli(cwd / "current-bench-results.txt", "cli"))
    metrics.extend(parse_cli(cwd / "current-parallel-bench-results.txt", "parallel_cli"))
    metrics.extend(parse_topn(cwd / "current-topn-bench-results.txt"))
    metrics.extend(parse_streaming(cwd / "current-stream-bench-results.txt"))
    metrics.extend(parse_memory(cwd / "current-mem-bench-results.txt"))
    metrics.extend(parse_server(cwd / "current-server-bench-results.txt"))
    metrics.extend(parse_criterion(cwd / "criterion-output.txt"))

    context = github_context()
    payload = {
        "schema_version": 1,
        "generated_at": datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z"),
        **context,
        "environment": {
            "runner_os": os.environ.get("RUNNER_OS", ""),
            "runner_arch": os.environ.get("RUNNER_ARCH", ""),
            "cpu_count": os.cpu_count(),
            "rustc": run_text(["rustc", "--version"]),
            "cargo": run_text(["cargo", "--version"]),
        },
        "metrics": metrics,
    }

    Path(args.output).write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"Wrote {args.output} with {len(metrics)} metrics")


if __name__ == "__main__":
    main()
