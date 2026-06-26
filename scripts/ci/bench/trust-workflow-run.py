#!/usr/bin/env python3
"""Normalize untrusted benchmark artifacts with trusted workflow_run metadata."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
from typing import Any


def first_pull_request(workflow_run: dict[str, Any]) -> dict[str, Any] | None:
    prs = workflow_run.get("pull_requests") or []
    if not prs:
        return None
    pr = prs[0]
    return {
        "number": pr.get("number"),
        "title": pr.get("title"),
        "head_sha": workflow_run.get("head_sha"),
        "head_ref": workflow_run.get("head_branch"),
        "base_ref": (pr.get("base") or {}).get("ref"),
        "url": pr.get("html_url"),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--result", default="incoming/benchmark-result.json")
    parser.add_argument("--event-path", default=os.environ.get("GITHUB_EVENT_PATH", ""))
    parser.add_argument("--output", default="incoming/benchmark-result.json")
    args = parser.parse_args()

    result_path = Path(args.result)
    if not args.event_path:
        raise SystemExit("Error: --event-path is required or GITHUB_EVENT_PATH must be set")
    event_path = Path(args.event_path)
    if not event_path.is_file():
        raise SystemExit(f"Error: event path is not a file: {event_path}")
    result = json.loads(result_path.read_text(encoding="utf-8"))
    event = json.loads(event_path.read_text(encoding="utf-8"))
    workflow_run = event["workflow_run"]

    repository = os.environ.get("GITHUB_REPOSITORY") or (event.get("repository") or {}).get("full_name", "")
    result["repository"] = repository
    result["event"] = workflow_run.get("event", "")
    result["branch"] = workflow_run.get("head_branch", "")
    result["sha"] = workflow_run.get("head_sha", "")
    result["run"] = {
        "id": str(workflow_run.get("id", "")),
        "attempt": str(workflow_run.get("run_attempt", "")),
        "url": workflow_run.get("html_url", ""),
        "workflow": workflow_run.get("name", ""),
    }
    result["pull_request"] = first_pull_request(workflow_run)

    Path(args.output).write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"Trusted benchmark metadata for run {result['run']['id']}")


if __name__ == "__main__":
    main()
