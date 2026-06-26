#!/usr/bin/env python3
"""Create or update the benchmark dashboard comment on a pull request."""
from __future__ import annotations

import argparse
import json
import os
import subprocess
from pathlib import Path


MARKER = "<!-- sumzle-benchmark-dashboard -->"


def run(args: list[str], input_text: str | None = None) -> str:
    return subprocess.check_output(args, input=input_text, text=True).strip()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--result", default="benchmark-result.json")
    parser.add_argument("--pages-url", required=True)
    args = parser.parse_args()

    result = json.loads(Path(args.result).read_text(encoding="utf-8"))
    pr = result.get("pull_request")
    if not pr or not pr.get("number"):
        print("Not a pull request benchmark; skipping comment")
        return

    repository = os.environ["GITHUB_REPOSITORY"]
    pr_number = str(pr["number"])
    dashboard_url = f"{args.pages_url.rstrip('/')}/?pr={pr_number}"
    run_url = result.get("run", {}).get("url", "")
    sha = result.get("sha", "")
    short_sha = sha[:7] if sha else "unknown"
    metric_count = len(result.get("metrics", []))

    body = f"""{MARKER}
## Benchmark History

Latest benchmark for this PR: `{short_sha}` with **{metric_count}** metrics.

- Dashboard: {dashboard_url}
- Workflow run: {run_url}

PR benchmark commits are shown as a separate curve from `main`.
"""

    comments_raw = run(["gh", "api", f"repos/{repository}/issues/{pr_number}/comments", "--paginate", "--slurp"])
    pages = json.loads(comments_raw) if comments_raw else []
    comments = [comment for page in pages for comment in page]
    existing_id = None
    for comment in comments:
        if MARKER in (comment.get("body") or ""):
            existing_id = comment.get("id")
            break

    if existing_id:
        run(["gh", "api", f"repos/{repository}/issues/comments/{existing_id}", "-X", "PATCH", "-f", f"body={body}"])
        print(f"Updated benchmark comment {existing_id}")
    else:
        run(["gh", "api", f"repos/{repository}/issues/{pr_number}/comments", "-f", f"body={body}"])
        print("Created benchmark comment")


if __name__ == "__main__":
    main()
