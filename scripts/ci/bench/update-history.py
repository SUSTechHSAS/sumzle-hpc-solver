#!/usr/bin/env python3
"""Merge a benchmark result into dashboard history with stable de-duplication."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def load_json(path: Path, default: Any) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (FileNotFoundError, json.JSONDecodeError):
        return default


def run_key(run: dict[str, Any]) -> tuple[str, str, str]:
    run_meta = run.get("run") or {}
    return (str(run_meta.get("id", "")), str(run_meta.get("attempt", "")), str(run.get("sha", "")))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--history", default="_site/data/history.json")
    parser.add_argument("--result", default="benchmark-result.json")
    parser.add_argument("--output", default="_site/data/history.json")
    parser.add_argument("--max-runs", type=int, default=500)
    args = parser.parse_args()

    history_path = Path(args.history)
    result_path = Path(args.result)
    output_path = Path(args.output)

    history = load_json(history_path, {"schema_version": 1, "runs": []})
    result = load_json(result_path, None)
    if result is None:
        raise SystemExit(f"could not read result JSON: {result_path}")

    runs = [run for run in history.get("runs", []) if run_key(run) != run_key(result)]
    runs.append(result)
    runs.sort(key=lambda run: (run.get("generated_at", ""), run.get("sha", "")))
    if args.max_runs > 0:
        runs = runs[-args.max_runs :]

    merged = {
        "schema_version": 1,
        "runs": runs,
    }
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(merged, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"Wrote {output_path} with {len(runs)} runs")


if __name__ == "__main__":
    main()
