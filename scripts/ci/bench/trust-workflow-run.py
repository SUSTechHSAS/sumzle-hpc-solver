#!/usr/bin/env python3
"""Normalize untrusted benchmark artifacts with trusted workflow_run metadata."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
from typing import Any
from urllib.parse import quote
from urllib.request import Request, urlopen

MAX_METRICS = 10_000
MAX_STRING_LEN = 256
MAX_PARAMS = 16


def clean_string(value: Any, max_len: int = MAX_STRING_LEN) -> str:
    return str(value or "")[:max_len]


def clean_bool(value: Any) -> bool:
    return bool(value)


def clean_metric(metric: Any) -> dict[str, Any] | None:
    if not isinstance(metric, dict):
        return None
    value = metric.get("value")
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        return None
    params_payload = metric.get("params")
    params: dict[str, Any] = {}
    if isinstance(params_payload, dict):
        for key, param_value in list(params_payload.items())[:MAX_PARAMS]:
            if isinstance(param_value, (int, float, str, bool)) or param_value is None:
                params[clean_string(key, 64)] = clean_string(param_value, 128) if isinstance(param_value, str) else param_value
    return {
        "suite": clean_string(metric.get("suite"), 80),
        "metric": clean_string(metric.get("metric"), 80),
        "value": value,
        "unit": clean_string(metric.get("unit"), 40),
        "params": params,
        "lower_is_better": clean_bool(metric.get("lower_is_better", True)),
    }


def clean_metrics(metrics: Any) -> list[dict[str, Any]]:
    if not isinstance(metrics, list):
        return []
    clean: list[dict[str, Any]] = []
    for metric in metrics[:MAX_METRICS]:
        clean_item = clean_metric(metric)
        if clean_item is not None:
            clean.append(clean_item)
    return clean


def clean_environment(environment: Any) -> dict[str, Any]:
    if not isinstance(environment, dict):
        return {}
    clean: dict[str, Any] = {}
    for key in ("runner_os", "runner_arch", "rustc", "cargo"):
        clean[key] = clean_string(environment.get(key), 160)
    cpu_count = environment.get("cpu_count")
    if isinstance(cpu_count, int) and not isinstance(cpu_count, bool):
        clean["cpu_count"] = cpu_count
    return clean


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
    except (OSError, json.JSONDecodeError) as exc:
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
    trusted_result = {
        "schema_version": 1,
        "generated_at": trusted_generated_at(workflow_run),
        "repository": clean_string(repository, 160),
        "event": clean_string(workflow_run.get("event"), 80),
        "branch": clean_string(workflow_run.get("head_branch"), 160),
        "sha": clean_string(workflow_run.get("head_sha"), 80),
        "run": {
            "id": str(workflow_run.get("id") or ""),
            "attempt": str(workflow_run.get("run_attempt") or ""),
            "url": clean_string(workflow_run.get("html_url"), 512),
            "workflow": clean_string(workflow_run.get("name"), 160),
        },
        "pull_request": first_pull_request(workflow_run) or pull_request_by_head_sha(repository, workflow_run),
        "environment": clean_environment(result.get("environment")),
        "metrics": clean_metrics(result.get("metrics")),
    }

    Path(args.output).write_text(json.dumps(trusted_result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"Trusted benchmark metadata for run {trusted_result['run']['id']}")


if __name__ == "__main__":
    main()
