#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Guy Boldon and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Find a commit's release rehearsal and use what it built.

Usage: release_rehearsal.py verify | fetch <job> | wait <sha>

Needs RELEASE_READ_TOKEN (CI) or GITLAB_TOKEN, since CI_JOB_TOKEN can list neither pipelines nor
jobs. See RELEASING.md.
"""

import http.client
import io
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile

API = os.environ.get("CI_API_V4_URL", "https://gitlab.com/api/v4")
PROJECT_ID = os.environ.get("CI_PROJECT_ID", "30707566")
DEFAULT_BRANCH = os.environ.get("CI_DEFAULT_BRANCH", "main")
MARKER_JOB = "release_rehearsed"
REHEARSAL_SOURCES = {"push", "web"}
FINISHED_UNPASSED = {"failed", "canceled", "skipped"}
WAIT_TIMEOUT_SECS = 3 * 60 * 60
# What a missing, expired or underprivileged token gets; any other error is retried.
TOKEN_REFUSED_CODES = {401, 403, 404}
WAIT_POLL_SECS = 60
REHEARSAL_ENV = "rehearsal.env"


def token() -> str:
    value = os.environ.get("RELEASE_READ_TOKEN") or os.environ.get("GITLAB_TOKEN")
    if not value:
        sys.exit(
            "Set RELEASE_READ_TOKEN (CI) or GITLAB_TOKEN (locally) to a read_api token."
        )
    return value


# HTTP(S) only: a default opener would also read file:, ftp: and data: URLs.
OPENER = urllib.request.OpenerDirector()
for handler in (
    urllib.request.ProxyHandler(),
    urllib.request.HTTPHandler(),
    urllib.request.HTTPSHandler(),
    urllib.request.HTTPRedirectHandler(),
    urllib.request.HTTPDefaultErrorHandler(),
    urllib.request.HTTPErrorProcessor(),
    # Refuses every other scheme with an error, instead of returning None.
    urllib.request.UnknownHandler(),
):
    OPENER.add_handler(handler)


def get(path: str, **params: str) -> bytes:
    url = f"{API}/projects/{PROJECT_ID}{path}"
    if params:
        url += "?" + urllib.parse.urlencode(params)
    request = urllib.request.Request(url)
    # Unredirected: artifacts redirect to object storage, which must never see the token.
    request.add_unredirected_header("PRIVATE-TOKEN", token())
    with OPENER.open(request, timeout=120) as response:
        return response.read()


def rehearsal_pipelines(sha: str) -> list[dict]:
    """The default branch's push and web pipelines for `sha`, newest first."""
    pipelines = json.loads(
        get("/pipelines", sha=sha, ref=DEFAULT_BRANCH, scope="branches", per_page="100")
    )
    return [
        p for p in pipelines if p["sha"] == sha and p["source"] in REHEARSAL_SOURCES
    ]


def passed_jobs(pipeline: dict) -> dict[str, dict]:
    jobs = json.loads(
        get(
            f"/pipelines/{int(pipeline['id'])}/jobs",
            **{"scope[]": "success"},
            per_page="100",
        )
    )
    return {job["name"]: job for job in jobs}


def passed_rehearsal(sha: str) -> tuple[dict, dict[str, dict]] | None:
    return passed_in(rehearsal_pipelines(sha))


def verified_rehearsal(sha: str) -> tuple[dict, dict[str, dict]] | None:
    """The rehearsal prerelease_verify chose, if it still binds to `sha`."""
    pipeline_id = os.environ.get("REHEARSAL_PIPELINE_ID", "")
    if not pipeline_id.isdigit():
        sys.exit(
            "REHEARSAL_PIPELINE_ID is not set: this job must need prerelease_verify."
        )
    pipeline = json.loads(get(f"/pipelines/{int(pipeline_id)}"))
    if (
        pipeline["sha"] != sha
        or pipeline["ref"] != DEFAULT_BRANCH
        or pipeline.get("tag")
        or pipeline["source"] not in REHEARSAL_SOURCES
    ):
        return None
    return passed_in([pipeline])


def passed_in(pipelines: list[dict]) -> tuple[dict, dict[str, dict]] | None:
    for pipeline in pipelines:
        if pipeline["status"] != "success":
            continue
        jobs = passed_jobs(pipeline)
        if MARKER_JOB in jobs:
            return pipeline, jobs
    return None


def commit_sha() -> str:
    sha = os.environ.get("CI_COMMIT_SHA")
    if not sha:
        sys.exit("CI_COMMIT_SHA is not set")
    return sha


def verify() -> None:
    sha = commit_sha()
    found = passed_rehearsal(sha)
    if found is None:
        sys.exit(
            f"No passed release rehearsal for {sha} on {DEFAULT_BRANCH}. Push the release "
            "commit and let its pipeline pass before pushing the tag."
        )
    # A dotenv report: the jobs that need this one fetch from this pipeline only.
    with open(REHEARSAL_ENV, "w", encoding="utf-8") as env:
        env.write(f"REHEARSAL_PIPELINE_ID={int(found[0]['id'])}\n")
    print(f"Rehearsed in {found[0]['web_url']}")


def fetch(job_name: str) -> None:
    sha = commit_sha()
    found = verified_rehearsal(sha)
    if found is None:
        sys.exit(
            f"REHEARSAL_PIPELINE_ID is no passed rehearsal of {sha}: no {job_name}."
        )
    pipeline, jobs = found
    if job_name not in jobs:
        sys.exit(f"The rehearsal {pipeline['web_url']} has no passed {job_name} job.")
    try:
        artifacts = get(f"/jobs/{int(jobs[job_name]['id'])}/artifacts")
    except urllib.error.HTTPError as error:
        if error.code != 404:
            raise
        sys.exit(
            f"The artifacts of {job_name} in {pipeline['web_url']} have expired or are "
            "missing. Rehearse the release again."
        )
    archive = zipfile.ZipFile(io.BytesIO(artifacts))
    for info in archive.infolist():
        # extract() keeps every path inside the working directory.
        path = archive.extract(info)
        mode = (info.external_attr >> 16) & 0o777
        if mode and not info.is_dir():
            os.chmod(path, mode)
        print(f"  {info.filename}")
    print(f"Fetched {job_name} from the rehearsal {pipeline['web_url']}")


def wait(sha: str) -> None:
    deadline = time.monotonic() + WAIT_TIMEOUT_SECS
    while time.monotonic() < deadline:
        try:
            pipelines = rehearsal_pipelines(sha)
            if passed_in(pipelines) is not None:
                return
        except urllib.error.HTTPError as error:
            if error.code in TOKEN_REFUSED_CODES:
                sys.exit(f"The API refused the request ({error}); check the token.")
            print(f"API error, retrying: {error}", file=sys.stderr)
            pipelines = []
        except OSError as error:
            print(f"API not reachable, retrying: {error}", file=sys.stderr)
            pipelines = []
        except (ValueError, http.client.HTTPException) as error:
            print(f"Unreadable API response, retrying: {error!r}", file=sys.stderr)
            pipelines = []
        if pipelines and pipelines[0]["status"] in FINISHED_UNPASSED:
            sys.exit(
                f"The rehearsal {pipelines[0]['status']}: {pipelines[0]['web_url']}"
            )
        if pipelines and pipelines[0]["status"] == "success":
            sys.exit(
                f"The pipeline passed but was not a rehearsal: {pipelines[0]['web_url']}"
            )
        time.sleep(WAIT_POLL_SECS)
    sys.exit(f"No passed rehearsal for {sha} after {WAIT_TIMEOUT_SECS // 3600} h.")


def main() -> None:
    match sys.argv[1:]:
        case ["verify"]:
            verify()
        case ["fetch", job_name]:
            fetch(job_name)
        case ["wait", sha]:
            wait(sha)
        case _:
            sys.exit(__doc__.split("\n\n")[1])


if __name__ == "__main__":
    main()
