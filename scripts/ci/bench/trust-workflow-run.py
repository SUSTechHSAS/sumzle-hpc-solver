#!/usr/bin/env python3
"""Normalize untrusted benchmark artifacts with trusted workflow_run metadata."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import Request, urlopen


def pull_request_payload(pr: dict[str, Any], workflow_run: dict[str, Any]) -> dict[str, Any] | None:
    if not pr.get("number"):
        return None
    head = pr.get("head")
    head_dict = head if isinstance(head, dict) else {}
    base = pr.get("base")
    base_dict = base if isinstance(base, dict) else {}
    return {
        "number": pr.get("number"),
        "title": pr.get("title") or "",
        "head_sha": head_dict.get("sha") or workflow_run.get("head_sha") or "",
        "head_ref": head_dict.get("ref") or workflow_run.get("head_branch") or "",
        "base_ref": base_dict.get("ref") or "",
        "url": pr.get("html_url") or "",
    }


def first_pull_request(workflow_run: dict[str, Any]) -> dict[str, Any] | None:
    prs = workflow_run.get("pull_requests")
    if not isinstance(prs, list) or not prs:
        return None
    pr = prs[0]
    if not isinstance(pr, dict):
        return None
    return pull_request_payload(pr, workflow_run)


def github_api_json(path: str) -> Any:
    token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
    headers = {
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
    }
    if token:
        headers["Authorization"] = f"Bearer {token}"
    api_url = os.environ.get("GITHUB_API_URL", "https://api.github.com").rstrip("/")
    request = Request(f"{api_url}/{path.lstrip('/')}", headers=headers)
    try:
        with urlopen(request, timeout=20) as response:
            return json.loads(response.read().decode("utf-8"))
    except (HTTPError, URLError, TimeoutError, json.JSONDecodeError) as exc:
        print(f"Warning: GitHub API lookup failed for {path}: {exc}")
        return None


def pull_request_by_head_sha(repository: str, workflow_run: dict[str, Any]) -> dict[str, Any] | None:
    if workflow_run.get("event") != "pull_request":
        return None
    head_sha = workflow_run.get("head_sha")
    if not repository or not head_sha:
        return None
    pulls = github_api_json(f"repos/{repository}/commits/{quote(str(head_sha), safe='')}/pulls")
    if not isinstance(pulls, list):
        return None
    for pr in pulls:
        if not isinstance(pr, dict):
            continue
        head = pr.get("head")
        head_dict = head if isinstance(head, dict) else {}
        base = pr.get("base")
        base_dict = base if isinstance(base, dict) else {}
        base_repo = base_dict.get("repo")
        base_repo_dict = base_repo if isinstance(base_repo, dict) else {}
        if head_dict.get("sha") == head_sha and base_repo_dict.get("full_name") == repository:
            return pull_request_payload(pr, workflow_run)
    return None


def trusted_generated_at(workflow_run: dict[str, Any]) -> str:
    for key in ("run_started_at", "created_at", "updated_at"):
        value = workflow_run.get(key)
        if value:
            return str(value)
    return ""


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
    result["generated_at"] = trusted_generated_at(workflow_run)
    result["run"] = {
        "id": str(workflow_run.get("id") or ""),
        "attempt": str(workflow_run.get("run_attempt") or ""),
        "url": workflow_run.get("html_url") or "",
        "workflow": workflow_run.get("name") or "",
    }
    result["pull_request"] = first_pull_request(workflow_run) or pull_request_by_head_sha(repository, workflow_run)

    Path(args.output).write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"Trusted benchmark metadata for run {result['run']['id']}")


if __name__ == "__main__":
    main()
