#!/usr/bin/env python3
"""Normalize untrusted benchmark artifacts with trusted workflow_run metadata."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
from typing import Any


def first_pull_request(workflow_run: dict[str, Any]) -> dict[str, Any] | None:
    prs = workflow_run.get("pull_requests")
    if not isinstance(prs, list) or not prs:
        return None
    pr = prs[0]
    if not isinstance(pr, dict):
        return None
    base = pr.get("base")
    base_dict = base if isinstance(base, dict) else {}
    return {
        "number": pr.get("number"),
        "title": pr.get("title") or "",
        "head_sha": workflow_run.get("head_sha") or "",
        "head_ref": workflow_run.get("head_branch") or "",
        "base_ref": base_dict.get("ref") or "",
        "url": pr.get("html_url") or "",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--result", default="incoming/benchmark-result.json")
    parser.add_argument("--event-path", default=os.environ.get("GITHUB_EVENT_PATH", ""))
    parser.add_argument("--output", default="incoming/benchmark-result.json")
    args = parser.parse_args()

    result_path = Path(args.result)
    if not result_path.is_file():
        raise SystemExit(f"Error: result path is not a file: {result_path}")
    if not args.event_path:
        raise SystemExit("Error: --event-path is required or GITHUB_EVENT_PATH must be set")
    event_path = Path(args.event_path)
    if not event_path.is_file():
        raise SystemExit(f"Error: event path is not a file: {event_path}")
    result = json.loads(result_path.read_text(encoding="utf-8"))
    if not isinstance(result, dict):
        raise SystemExit(f"Error: result JSON is not a valid dictionary: {result_path}")
    event = json.loads(event_path.read_text(encoding="utf-8"))
    if not isinstance(event, dict):
        raise SystemExit(f"Error: event JSON is not a valid dictionary: {event_path}")
    if "workflow_run" not in event:
        raise SystemExit("Error: GITHUB_EVENT_PATH does not contain 'workflow_run' metadata")
    workflow_run = event["workflow_run"]
    if not isinstance(workflow_run, dict):
        raise SystemExit("Error: GITHUB_EVENT_PATH workflow_run metadata is not a dictionary")

    repository_payload = event.get("repository")
    repository_dict = repository_payload if isinstance(repository_payload, dict) else {}
    repository = os.environ.get("GITHUB_REPOSITORY") or repository_dict.get("full_name", "")
    result["repository"] = repository
    result["event"] = workflow_run.get("event") or ""
    result["branch"] = workflow_run.get("head_branch") or ""
    result["sha"] = workflow_run.get("head_sha") or ""
    result["run"] = {
        "id": str(workflow_run.get("id") or ""),
        "attempt": str(workflow_run.get("run_attempt") or ""),
        "url": workflow_run.get("html_url") or "",
        "workflow": workflow_run.get("name") or "",
    }
    result["pull_request"] = first_pull_request(workflow_run)

    Path(args.output).write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"Trusted benchmark metadata for run {result['run']['id']}")


if __name__ == "__main__":
    main()
