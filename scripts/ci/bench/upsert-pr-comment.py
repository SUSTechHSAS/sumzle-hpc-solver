#!/usr/bin/env python3
"""Create or update the benchmark dashboard comment on a pull request."""
from __future__ import annotations

import argparse
import json
import os
import subprocess
from pathlib import Path
from typing import Any


MARKER = "<!-- sumzle-benchmark-dashboard -->"


def run(args: list[str], input_text: str | None = None) -> str:
    return subprocess.check_output(args, input=input_text, text=True, timeout=30).strip()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--result", default="benchmark-result.json")
    parser.add_argument("--pages-url", required=True)
    args = parser.parse_args()

    result_path = Path(args.result)
    if not result_path.is_file():
        print(f"Error: result path is not a file: {result_path}")
        return
    result: Any = json.loads(result_path.read_text(encoding="utf-8"))
    if not isinstance(result, dict):
        print("Error: result is not a valid JSON object")
        return
    pr = result.get("pull_request")
    if not isinstance(pr, dict) or not pr.get("number"):
        print("Not a pull request benchmark; skipping comment")
        return

    repository = os.environ.get("GITHUB_REPOSITORY", "")
    if not repository:
        print("Error: GITHUB_REPOSITORY environment variable is not set")
        return
    pr_number = str(pr["number"])
    dashboard_url = f"{args.pages_url.rstrip('/')}/?pr={pr_number}"
    run_payload = result.get("run")
    run_dict = run_payload if isinstance(run_payload, dict) else {}
    run_url = run_dict.get("url", "")
    sha = result.get("sha", "")
    short_sha = sha[:7] if sha else "unknown"
    metrics = result.get("metrics")
    metric_count = len(metrics) if isinstance(metrics, list) else 0

    body = f"""{MARKER}
## Benchmark History

Latest benchmark for this PR: `{short_sha}` with **{metric_count}** metrics.

- Dashboard: {dashboard_url}
- Workflow run: {run_url}

PR benchmark commits are shown as a separate curve from `main`.
"""

    comments_raw = run(["gh", "api", f"repos/{repository}/issues/{pr_number}/comments", "--paginate", "--slurp"])
    pages = json.loads(comments_raw) if comments_raw else []
    comments = []
    for item in pages:
        if isinstance(item, list):
            comments.extend(item)
        elif isinstance(item, dict):
            comments.append(item)
    existing_id = None
    for comment in comments:
        if isinstance(comment, dict) and MARKER in (comment.get("body") or ""):
            existing_id = comment.get("id")
            break

    if existing_id:
        run(["gh", "api", f"repos/{repository}/issues/comments/{existing_id}", "-X", "PATCH", "-f", f"body={body}"])
        print(f"Updated benchmark comment {existing_id}")
    else:
        run(["gh", "api", f"repos/{repository}/issues/{pr_number}/comments", "-f", f"body={body}"])
        print("Created benchmark comment")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:  # noqa: BLE001 - PR comments are non-critical.
        print(f"Warning: Failed to upsert PR comment: {exc}")
