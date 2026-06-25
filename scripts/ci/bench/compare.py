#!/usr/bin/env python3
"""Compare current-branch benchmark results against the base-branch
baseline and append a Markdown report to $GITHUB_STEP_SUMMARY.

Extracted verbatim from the `benchmark` job's "Compare benchmark results
with main branch" step in .github/workflows/ci.yml. Reads PR_BRANCH /
BASE_BRANCH / GITHUB_STEP_SUMMARY from the environment (set by that step)
and the *-bench-results.txt files written by the earlier benchmark steps.
"""
import re, os

def parse_time_ns(time_str):
    """Convert a time string like 275us, 2.2ms, 367ms to nanoseconds."""
    if not time_str or time_str.strip() == "":
        return None
    time_str = time_str.strip()
    # Handle micro sign (µ) and ASCII 'us'
    m = re.match(r"^([0-9.]+)\s*(ns|µs|us|ms|s)$", time_str)
    if not m:
        return None
    value = float(m.group(1))
    unit = m.group(2)
    multipliers = {"ns": 1, "µs": 1e3, "us": 1e3, "ms": 1e6, "s": 1e9}
    return value * multipliers.get(unit, 1)

def format_time(ns):
    """Format nanoseconds to a human-readable string."""
    if ns is None:
        return "N/A"
    if ns < 1e3:
        return f"{ns:.0f}ns"
    elif ns < 1e6:
        return f"{ns/1e3:.1f}us"
    elif ns < 1e9:
        return f"{ns/1e6:.1f}ms"
    else:
        return f"{ns/1e9:.2f}s"

def format_diff_pct(current_ns, baseline_ns):
    """Format percentage difference with emoji indicator."""
    if current_ns is None or baseline_ns is None or baseline_ns == 0:
        return "N/A", "⚪"
    pct = ((current_ns - baseline_ns) / baseline_ns) * 100
    if pct > 10:
        emoji = "🔴"
    elif pct > 5:
        emoji = "🟡"
    elif pct < -10:
        emoji = "🟢"
    elif pct < -5:
        emoji = "🔵"
    else:
        emoji = "⚪"
    sign = "+" if pct >= 0 else ""
    return f"{sign}{pct:.1f}%", emoji

def load_results(filepath):
    """Load benchmark results from colon-separated file."""
    results = {}
    try:
        with open(filepath) as f:
            for line in f:
                parts = line.strip().split(":", 3)
                if len(parts) == 4:
                    length = parts[0]
                    results[length] = {
                        "solutions": int(parts[1]),
                        "searched": int(parts[2]),
                        "time_raw": parts[3],
                        "time_ns": parse_time_ns(parts[3]),
                    }
    except FileNotFoundError:
        pass
    return results

def load_topn_results(filepath):
    """Load top-N results: L:N:sol:searched:time_ms:speed."""
    results = {}
    try:
        with open(filepath) as f:
            for line in f:
                parts = line.strip().split(":")
                if len(parts) == 6:
                    key = f"{parts[0]}:{parts[1]}"
                    results[key] = {
                        "length": parts[0],
                        "n": int(parts[1]),
                        "solutions": int(parts[2]),
                        "searched": int(parts[3]),
                        "time_ms": int(parts[4]),
                        "speed": int(parts[5]),
                        "time_ns": int(parts[4]) * 1_000_000,
                    }
    except FileNotFoundError:
        pass
    return results

def load_stream_results(filepath):
    """Load streaming results: L:sol:searched:time_ms:speed:file_bytes."""
    results = {}
    try:
        with open(filepath) as f:
            for line in f:
                parts = line.strip().split(":")
                if len(parts) == 6:
                    results[parts[0]] = {
                        "solutions": int(parts[1]),
                        "searched": int(parts[2]),
                        "time_ms": int(parts[3]),
                        "speed": int(parts[4]),
                        "file_bytes": int(parts[5]),
                        "time_ns": int(parts[3]) * 1_000_000,
                    }
    except FileNotFoundError:
        pass
    return results

def load_mem_results(filepath):
    """Load memory results: L:mode:peak_rss_kb:wall_ms."""
    results = {}
    try:
        with open(filepath) as f:
            for line in f:
                parts = line.strip().split(":")
                if len(parts) == 4:
                    key = f"{parts[0]}:{parts[1]}"
                    results[key] = {
                        "length": parts[0],
                        "mode": parts[1],
                        "peak_rss_kb": int(parts[2]),
                        "wall_ms": int(parts[3]),
                    }
    except FileNotFoundError:
        pass
    return results

def load_server_results(filepath):
    """Load server results: L:req:conc:throughput:mean_ms:p50_ms:p95_ms:peak_rss_kb:found:searched:errors."""
    results = {}
    try:
        with open(filepath) as f:
            for line in f:
                parts = line.strip().split(":")
                if len(parts) == 11:
                    results[parts[0]] = {
                        "requests": int(parts[1]),
                        "concurrency": int(parts[2]),
                        "throughput": float(parts[3]),
                        "mean_ms": float(parts[4]),
                        "p50_ms": float(parts[5]),
                        "p95_ms": float(parts[6]),
                        "peak_rss_kb": int(parts[7]),
                        "found": int(parts[8]),
                        "searched": int(parts[9]),
                        "errors": int(parts[10]),
                        # Mean latency in ns, so format_diff_pct (lower = better)
                        # treats a latency regression like the other time metrics.
                        "time_ns": float(parts[4]) * 1_000_000,
                    }
    except FileNotFoundError:
        pass
    return results

def format_rss(kb):
    if kb is None:
        return "N/A"
    if kb >= 1024 * 1024:
        return f"{kb / (1024 * 1024):.2f} GB"
    elif kb >= 1024:
        return f"{kb / 1024:.1f} MB"
    else:
        return f"{kb} KB"

current = load_results("current-bench-results.txt")
current_par = load_results("current-parallel-bench-results.txt")
baseline = load_results("baseline-bench-results.txt")
baseline_par = load_results("baseline-parallel-bench-results.txt")
current_topn = load_topn_results("current-topn-bench-results.txt")
baseline_topn = load_topn_results("baseline-topn-bench-results.txt")
current_stream = load_stream_results("current-stream-bench-results.txt")
baseline_stream = load_stream_results("baseline-stream-bench-results.txt")
current_mem = load_mem_results("current-mem-bench-results.txt")
baseline_mem = load_mem_results("baseline-mem-bench-results.txt")
current_server = load_server_results("current-server-bench-results.txt")
baseline_server = load_server_results("baseline-server-bench-results.txt")

pr_branch = os.environ.get("PR_BRANCH", "PR")
base_branch = os.environ.get("BASE_BRANCH", "main")

has_mismatch = False
has_regression = False

lines = []

# =============================================================
#  Section 1: Sequential Performance Comparison
# =============================================================
lines.append("## 📊 Sequential Benchmark Comparison with Main Branch")
lines.append("")
lines.append(f"Comparing **{pr_branch}** (PR) vs **{base_branch}** (baseline)")
lines.append("")
lines.append("| Length | Metric | PR | Main | Match | Diff |")
lines.append("|--------|--------|-----|------|-------|------|")

for length in ["3", "4", "5", "6", "7", "8"]:
    cur = current.get(length)
    base = baseline.get(length)

    if not cur or not base:
        lines.append(f"| {length} | — | — | — | ⚠️ Missing | — |")
        continue

    # Solutions comparison
    sol_match = cur["solutions"] == base["solutions"]
    sol_icon = "✅" if sol_match else "❌"
    if not sol_match:
        has_mismatch = True
        sol_diff = f'{cur["solutions"] - base["solutions"]:+d}'
    else:
        sol_diff = "="
    lines.append(f'| {length} | Solutions | {cur["solutions"]} | {base["solutions"]} | {sol_icon} | {sol_diff} |')

    # Expressions Searched comparison
    src_match = cur["searched"] == base["searched"]
    src_icon = "✅" if src_match else "❌"
    if not src_match:
        has_mismatch = True
        src_diff = f'{cur["searched"] - base["searched"]:+d}'
    else:
        src_diff = "="
    lines.append(f'| {length} | Expr Searched | {cur["searched"]} | {base["searched"]} | {src_icon} | {src_diff} |')

    # Time comparison
    time_diff_pct, time_emoji = format_diff_pct(cur["time_ns"], base["time_ns"])
    if time_emoji in ("🔴", "🟡"):
        has_regression = True
    cur_time_str = format_time(cur["time_ns"]) if cur["time_ns"] is not None else cur["time_raw"]
    base_time_str = format_time(base["time_ns"]) if base["time_ns"] is not None else base["time_raw"]
    lines.append(f"| {length} | Time | {cur_time_str} | {base_time_str} | {time_emoji} | {time_diff_pct} |")

lines.append("")

# =============================================================
#  Section 2: Parallel Performance Comparison
# =============================================================
if baseline_par:
    lines.append("## 📊 Parallel Benchmark Comparison with Main Branch")
    lines.append("")
    lines.append(f"Comparing **{pr_branch}** (PR) vs **{base_branch}** (baseline)")
    lines.append("")
    lines.append("| Length | Metric | PR | Main | Match | Diff |")
    lines.append("|--------|--------|-----|------|-------|------|")

    for length in ["3", "4", "5", "6", "7", "8"]:
        cur = current_par.get(length)
        base = baseline_par.get(length)

        if not cur or not base:
            lines.append(f"| {length} | — | — | — | ⚠️ Missing | — |")
            continue

        sol_match = cur["solutions"] == base["solutions"]
        sol_icon = "✅" if sol_match else "❌"
        if not sol_match:
            has_mismatch = True
            sol_diff = f'{cur["solutions"] - base["solutions"]:+d}'
        else:
            sol_diff = "="
        lines.append(f'| {length} | Solutions | {cur["solutions"]} | {base["solutions"]} | {sol_icon} | {sol_diff} |')

        src_match = cur["searched"] == base["searched"]
        src_icon = "✅" if src_match else "❌"
        if not src_match:
            has_mismatch = True
            src_diff = f'{cur["searched"] - base["searched"]:+d}'
        else:
            src_diff = "="
        lines.append(f'| {length} | Expr Searched | {cur["searched"]} | {base["searched"]} | {src_icon} | {src_diff} |')

        time_diff_pct, time_emoji = format_diff_pct(cur["time_ns"], base["time_ns"])
        if time_emoji in ("🔴", "🟡"):
            has_regression = True
        cur_time_str = format_time(cur["time_ns"]) if cur["time_ns"] is not None else cur["time_raw"]
        base_time_str = format_time(base["time_ns"]) if base["time_ns"] is not None else base["time_raw"]
        lines.append(f"| {length} | Time | {cur_time_str} | {base_time_str} | {time_emoji} | {time_diff_pct} |")

    lines.append("")

# =============================================================
#  Section 3: Sequential vs Parallel Consistency Check
# =============================================================
if current_par:
    lines.append("## ✅ Sequential vs Parallel Consistency Check")
    lines.append("")
    lines.append("Verifying that the parallel solver produces identical results to the sequential solver (auto thread count).")
    lines.append("")
    lines.append("| Length | Metric | Sequential | Parallel | Match |")
    lines.append("|--------|--------|------------|----------|-------|")

    for length in ["3", "4", "5", "6", "7", "8"]:
        seq = current.get(length)
        par = current_par.get(length)

        if not seq or not par:
            lines.append(f"| {length} | — | — | — | ⚠️ Missing |")
            continue

        sol_match = seq["solutions"] == par["solutions"]
        sol_icon = "✅" if sol_match else "❌"
        if not sol_match:
            has_mismatch = True
        lines.append(f'| {length} | Solutions | {seq["solutions"]} | {par["solutions"]} | {sol_icon} |')

        src_match = seq["searched"] == par["searched"]
        src_icon = "✅" if src_match else "❌"
        if not src_match:
            has_mismatch = True
        lines.append(f'| {length} | Expr Searched | {seq["searched"]} | {par["searched"]} | {src_icon} |')

        # Speedup comparison
        if seq["time_ns"] and par["time_ns"] and par["time_ns"] > 0:
            speedup = seq["time_ns"] / par["time_ns"]
            speedup_str = f"{speedup:.2f}x"
        else:
            speedup_str = "N/A"
        lines.append(f'| {length} | Time | {format_time(seq["time_ns"]) if seq["time_ns"] else seq["time_raw"]} | {format_time(par["time_ns"]) if par["time_ns"] else par["time_raw"]} | ⚡ {speedup_str} |')

    lines.append("")

# =============================================================
#  Section 4: Top-N Benchmark Comparison
# =============================================================
if current_topn:
    lines.append("## 📊 Top-N Benchmark Comparison with Main Branch")
    lines.append("")
    lines.append(f"Comparing **{pr_branch}** (PR) vs **{base_branch}** (baseline)")
    lines.append("")
    lines.append("| Length | N | Metric | PR | Main | Match | Diff |")
    lines.append("|--------|------|--------|-----|------|-------|------|")
    for length in ["6", "7", "8", "9"]:
        for n in [100, 1000]:
            key = f"{length}:{n}"
            cur = current_topn.get(key)
            base = baseline_topn.get(key)
            if not cur:
                lines.append(f"| {length} | {n} | — | — | — | ⚠️ Missing | — |")
                continue
            # Solutions kept should equal N (sanity)
            sol_match = cur["solutions"] == n
            sol_icon = "✅" if sol_match else "❌"
            sol_diff = "=" if sol_match else f'{cur["solutions"] - n:+d}'
            # searched should match baseline
            if base:
                src_match = cur["searched"] == base["searched"]
                src_icon = "✅" if src_match else "❌"
                if not src_match:
                    has_mismatch = True
                    src_diff = f'{cur["searched"] - base["searched"]:+d}'
                else:
                    src_diff = "="
                time_diff_pct, time_emoji = format_diff_pct(cur["time_ns"], base["time_ns"])
                if time_emoji in ("🔴", "🟡"):
                    has_regression = True
                cur_time_str = format_time(cur["time_ns"])
                base_time_str = format_time(base["time_ns"])
                base_src_str = str(base["searched"])
                base_sol_str = str(base["solutions"])
            else:
                src_icon = "—"; src_diff = "—"
                time_emoji = "—"; time_diff_pct = "—"
                cur_time_str = format_time(cur["time_ns"])
                base_time_str = "N/A"
                base_src_str = "—"
                base_sol_str = "—"
            lines.append(f'| {length} | {n} | Solutions (kept) | {cur["solutions"]} | {base_sol_str} | {sol_icon} | {sol_diff} |')
            lines.append(f'| {length} | {n} | Expr Searched | {cur["searched"]} | {base_src_str} | {src_icon} | {src_diff} |')
            lines.append(f"| {length} | {n} | Time | {cur_time_str} | {base_time_str} | {time_emoji} | {time_diff_pct} |")
    lines.append("")

# =============================================================
#  Section 5: Streaming Benchmark Comparison
# =============================================================
if current_stream:
    lines.append("## 📊 Streaming Benchmark Comparison with Main Branch")
    lines.append("")
    lines.append(f"Comparing **{pr_branch}** (PR) vs **{base_branch}** (baseline)")
    lines.append("")
    lines.append("| Length | Metric | PR | Main | Match | Diff |")
    lines.append("|--------|--------|-----|------|-------|------|")
    for length in ["6", "7", "8", "9"]:
        cur = current_stream.get(length)
        base = baseline_stream.get(length)
        if not cur:
            lines.append(f"| {length} | — | — | — | ⚠️ Missing | — |")
            continue
        if base:
            sol_match = cur["solutions"] == base["solutions"]
            sol_icon = "✅" if sol_match else "❌"
            if not sol_match:
                has_mismatch = True
                sol_diff = f'{cur["solutions"] - base["solutions"]:+d}'
            else:
                sol_diff = "="
            src_match = cur["searched"] == base["searched"]
            src_icon = "✅" if src_match else "❌"
            if not src_match:
                has_mismatch = True
                src_diff = f'{cur["searched"] - base["searched"]:+d}'
            else:
                src_diff = "="
            time_diff_pct, time_emoji = format_diff_pct(cur["time_ns"], base["time_ns"])
            if time_emoji in ("🔴", "🟡"):
                has_regression = True
            cur_time_str = format_time(cur["time_ns"])
            base_time_str = format_time(base["time_ns"])
            base_sol_str = str(base["solutions"])
            base_src_str = str(base["searched"])
        else:
            sol_icon = "—"; sol_diff = "—"
            src_icon = "—"; src_diff = "—"
            time_emoji = "—"; time_diff_pct = "—"
            cur_time_str = format_time(cur["time_ns"])
            base_time_str = "N/A"
            base_sol_str = "—"
            base_src_str = "—"
        lines.append(f'| {length} | Solutions | {cur["solutions"]} | {base_sol_str} | {sol_icon} | {sol_diff} |')
        lines.append(f'| {length} | Expr Searched | {cur["searched"]} | {base_src_str} | {src_icon} | {src_diff} |')
        lines.append(f"| {length} | Time | {cur_time_str} | {base_time_str} | {time_emoji} | {time_diff_pct} |")
    lines.append("")

# =============================================================
#  Section 6: Memory Benchmark Comparison
# =============================================================
if current_mem:
    lines.append("## 📊 Memory Benchmark Comparison with Main Branch")
    lines.append("")
    lines.append("Peak resident set size (VmHWM) per (length, mode). Larger RSS than main is a memory regression.")
    lines.append("")
    lines.append("| Length | Mode | PR RSS | Main RSS | Match | Diff |")
    lines.append("|--------|------|--------|----------|-------|------|")
    for length in ["6", "7", "8"]:
        for mode in ["default", "streaming", "top"]:
            key = f"{length}:{mode}"
            cur = current_mem.get(key)
            base = baseline_mem.get(key)
            if not cur:
                lines.append(f"| {length} | {mode} | — | — | ⚠️ Missing | — |")
                continue
            if base and base["peak_rss_kb"] > 0:
                rss_diff_pct = ((cur["peak_rss_kb"] - base["peak_rss_kb"]) / base["peak_rss_kb"]) * 100
                if rss_diff_pct > 10:
                    rss_icon = "🔴"
                    has_regression = True
                elif rss_diff_pct > 5:
                    rss_icon = "🟡"
                    has_regression = True
                elif rss_diff_pct < -10:
                    rss_icon = "🟢"
                elif rss_diff_pct < -5:
                    rss_icon = "🔵"
                else:
                    rss_icon = "⚪"
                sign = "+" if rss_diff_pct >= 0 else ""
                rss_diff_str = f"{sign}{rss_diff_pct:.1f}%"
                base_rss_hr = format_rss(base["peak_rss_kb"])
            else:
                rss_icon = "—"
                rss_diff_str = "—"
                base_rss_hr = "N/A"
            cur_rss_hr = format_rss(cur["peak_rss_kb"])
            lines.append(f'| {length} | {mode} | {cur_rss_hr} | {base_rss_hr} | {rss_icon} | {rss_diff_str} |')
    lines.append("")

# =============================================================
#  Section 6b: Server Multi-Solve Benchmark Comparison
# =============================================================
if current_server:
    lines.append("## 📊 Server Multi-Solve Benchmark Comparison with Main Branch")
    lines.append("")
    lines.append(f"Comparing **{pr_branch}** (PR) vs **{base_branch}** (baseline). Concurrent `POST /api/solve`; lower mean latency / higher throughput is better, and server peak RSS larger than main is a memory regression (issue #20).")
    lines.append("")
    lines.append("| Length | Metric | PR | Main | Match | Diff |")
    lines.append("|--------|--------|-----|------|-------|------|")
    for length in ["4", "5", "6"]:
        cur = current_server.get(length)
        base = baseline_server.get(length)
        if not cur:
            lines.append(f"| {length} | — | — | — | ⚠️ Missing | — |")
            continue

        # Failed requests / intra-run found|searched disagreement.
        err_icon = "✅" if cur["errors"] == 0 else "❌"
        if cur["errors"] != 0:
            has_mismatch = True
        base_err = str(base["errors"]) if base else "—"
        lines.append(f'| {length} | Errors | {cur["errors"]} | {base_err} | {err_icon} | — |')

        if base:
            # Correctness: found/searched must match the baseline server.
            sol_match = cur["found"] == base["found"]
            sol_icon = "✅" if sol_match else "❌"
            if not sol_match:
                has_mismatch = True
            sol_diff = "=" if sol_match else f'{cur["found"] - base["found"]:+d}'
            lines.append(f'| {length} | Solutions | {cur["found"]} | {base["found"]} | {sol_icon} | {sol_diff} |')

            src_match = cur["searched"] == base["searched"]
            src_icon = "✅" if src_match else "❌"
            if not src_match:
                has_mismatch = True
            src_diff = "=" if src_match else f'{cur["searched"] - base["searched"]:+d}'
            lines.append(f'| {length} | Expr Searched | {cur["searched"]} | {base["searched"]} | {src_icon} | {src_diff} |')

            # Throughput (higher is better) — informational ratio.
            if base["throughput"] > 0:
                tp_ratio = f'{cur["throughput"] / base["throughput"]:.2f}x'
            else:
                tp_ratio = "N/A"
            lines.append(f'| {length} | Throughput | {cur["throughput"]:.1f} req/s | {base["throughput"]:.1f} req/s | ⚡ | {tp_ratio} |')

            # Mean latency (lower is better) drives the regression gate.
            lat_diff_pct, lat_emoji = format_diff_pct(cur["time_ns"], base["time_ns"])
            if lat_emoji in ("🔴", "🟡"):
                has_regression = True
            lines.append(f'| {length} | Mean Latency | {cur["mean_ms"]:.1f}ms | {base["mean_ms"]:.1f}ms | {lat_emoji} | {lat_diff_pct} |')

            # Server peak RSS (lower is better) — issue #20 guard.
            if base["peak_rss_kb"] > 0:
                rss_diff_pct = ((cur["peak_rss_kb"] - base["peak_rss_kb"]) / base["peak_rss_kb"]) * 100
                if rss_diff_pct > 10:
                    rss_icon = "🔴"; has_regression = True
                elif rss_diff_pct > 5:
                    rss_icon = "🟡"; has_regression = True
                elif rss_diff_pct < -10:
                    rss_icon = "🟢"
                elif rss_diff_pct < -5:
                    rss_icon = "🔵"
                else:
                    rss_icon = "⚪"
                sign = "+" if rss_diff_pct >= 0 else ""
                rss_diff_str = f"{sign}{rss_diff_pct:.1f}%"
                base_rss_hr = format_rss(base["peak_rss_kb"])
            else:
                rss_icon = "—"; rss_diff_str = "—"; base_rss_hr = "N/A"
            lines.append(f'| {length} | Server Peak RSS | {format_rss(cur["peak_rss_kb"])} | {base_rss_hr} | {rss_icon} | {rss_diff_str} |')
        else:
            lines.append(f'| {length} | Throughput | {cur["throughput"]:.1f} req/s | N/A | — | — |')
            lines.append(f'| {length} | Mean Latency | {cur["mean_ms"]:.1f}ms | N/A | — | — |')
            lines.append(f'| {length} | Server Peak RSS | {format_rss(cur["peak_rss_kb"])} | N/A | — | — |')
    lines.append("")

# =============================================================
#  Section 7: Cross-Mode searched_count Consistency Check
# =============================================================
if current_stream or current_topn:
    lines.append("## ✅ Cross-Mode searched_count Consistency Check")
    lines.append("")
    lines.append("Verifies that default, streaming, and top-N modes all enumerate the same expression space (searched_count must match across modes).")
    lines.append("")
    lines.append("| Length | Default (parallel) | Streaming | Top-N (N=100) | Top-N (N=1000) | Match |")
    lines.append("|--------|---------------------|-----------|---------------|-----------------|-------|")
    for length in ["6", "7", "8", "9"]:
        # Default searched_count comes from parallel bench (covers L3-L8; no L9).
        default_src = current_par.get(length, {}).get("searched") if length != "9" else None
        stream_src = current_stream.get(length, {}).get("searched")
        topn100_src = current_topn.get(f"{length}:100", {}).get("searched")
        topn1000_src = current_topn.get(f"{length}:1000", {}).get("searched")
        vals = [v for v in [default_src, stream_src, topn100_src, topn1000_src] if v is not None]
        all_match = len(set(vals)) <= 1
        match_icon = "✅" if all_match else "❌"
        if not all_match:
            has_mismatch = True
        def fmt_src(v):
            return str(v) if v is not None else "—"
        lines.append(f"| {length} | {fmt_src(default_src)} | {fmt_src(stream_src)} | {fmt_src(topn100_src)} | {fmt_src(topn1000_src)} | {match_icon} |")
    lines.append("")

# Add legend
lines.append("<details>")
lines.append("<summary>📖 Comparison Legend</summary>")
lines.append("")
lines.append("| Icon | Meaning |")
lines.append("|------|---------|")
lines.append("| ✅ | Values match (correctness OK) |")
lines.append("| ❌ | Values differ (correctness concern) |")
lines.append("| 🟢 | Time improved > 10% |")
lines.append("| 🔵 | Time improved 5-10% |")
lines.append("| ⚪ | Time within ±5% (noise range) |")
lines.append("| 🟡 | Time regressed 5-10% |")
lines.append("| 🔴 | Time regressed > 10% |")
lines.append("| ⚡ | Parallel speedup vs sequential |")
lines.append("")
lines.append("</details>")
lines.append("")

# Add warnings / all-clear
if has_mismatch:
    lines.append("> ⚠️ **Solutions or Expressions Searched count mismatch detected!** This may indicate a correctness issue.")
    lines.append("")
if has_regression:
    lines.append("> 🔴 **Performance regression detected!** One or more benchmarks show > 5% slowdown compared to main.")
    lines.append("")
if not has_mismatch and not has_regression:
    lines.append("> ✅ **No correctness or significant performance issues detected.**")
    lines.append("")

# Add raw baseline output in collapsible section
lines.append("<details>")
lines.append("<summary>📝 Raw Baseline (Main Branch) Benchmark Output</summary>")
lines.append("")
lines.append("```")
try:
    with open("baseline-benchmark-output.txt") as f:
        lines.append(f.read())
except FileNotFoundError:
    lines.append("(no baseline output)")
lines.append("```")
lines.append("")
lines.append("</details>")

lines.append("<details>")
lines.append("<summary>📝 Raw Baseline Parallel Benchmark Output</summary>")
lines.append("")
lines.append("```")
try:
    with open("baseline-parallel-benchmark-output.txt") as f:
        lines.append(f.read())
except FileNotFoundError:
    lines.append("(no baseline parallel output)")
lines.append("```")
lines.append("")
lines.append("</details>")

lines.append("<details>")
lines.append("<summary>📝 Raw Baseline Top-N Benchmark Output</summary>")
lines.append("")
lines.append("```")
try:
    with open("baseline-topn-benchmark-output.txt") as f:
        lines.append(f.read())
except FileNotFoundError:
    lines.append("(no baseline top-N output)")
lines.append("```")
lines.append("")
lines.append("</details>")

lines.append("<details>")
lines.append("<summary>📝 Raw Baseline Streaming Benchmark Output</summary>")
lines.append("")
lines.append("```")
try:
    with open("baseline-stream-benchmark-output.txt") as f:
        lines.append(f.read())
except FileNotFoundError:
    lines.append("(no baseline streaming output)")
lines.append("```")
lines.append("")
lines.append("</details>")

lines.append("<details>")
lines.append("<summary>📝 Raw Baseline Memory Benchmark Output</summary>")
lines.append("")
lines.append("```")
try:
    with open("baseline-mem-benchmark-output.txt") as f:
        lines.append(f.read())
except FileNotFoundError:
    lines.append("(no baseline memory output)")
lines.append("```")
lines.append("")
lines.append("</details>")

lines.append("<details>")
lines.append("<summary>📝 Raw Baseline Server Benchmark Output</summary>")
lines.append("")
lines.append("```")
try:
    with open("baseline-server-benchmark-output.txt") as f:
        lines.append(f.read())
except FileNotFoundError:
    lines.append("(no baseline server output)")
lines.append("```")
lines.append("")
lines.append("</details>")

# Write to step summary
summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
if summary_path:
    with open(summary_path, "a") as f:
        f.write("\n".join(lines) + "\n")
else:
    # When run outside GitHub Actions (local debugging), print to stdout
    # instead of crashing with KeyError.
    print("\n".join(lines))

# Print summary for the log
if has_mismatch:
    print("WARNING: Solutions or Expressions Searched mismatch detected!")
if has_regression:
    print("WARNING: Performance regression detected!")
if not has_mismatch and not has_regression:
    print("All benchmark comparisons passed.")
