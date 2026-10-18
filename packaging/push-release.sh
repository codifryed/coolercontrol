#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Guy Boldon and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Usage: push-release.sh (through `make push-release`, after release.sh)
#
# Pushes the Release commit, waits for its rehearsal to pass, then pushes the tag (RELEASING.md).

set -euo pipefail

readonly PROJECT_PATH="coolercontrol/coolercontrol"
readonly PROJECT_ID="30707566"
readonly DEFAULT_BRANCH="main"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
readonly REPO_ROOT

if [[ -z ${GITLAB_TOKEN-} ]]; then
    echo "Set GITLAB_TOKEN to a read_api token: the rehearsal is checked through the API" >&2
    exit 1
fi

if ! tag="$(git describe --exact-match --tags HEAD 2>/dev/null)"; then
    echo "HEAD has no tag: run make release first" >&2
    exit 1
fi
title="$(git log -1 --format=%s HEAD)"
if [[ ${title} != "Release ${tag}" ]]; then
    echo "HEAD is \"${title}\", not the Release ${tag} commit" >&2
    exit 1
fi
branch="$(git symbolic-ref --short -q HEAD || true)"
if [[ ${branch} != "${DEFAULT_BRANCH}" ]]; then
    echo "Release from ${DEFAULT_BRANCH}: only its Release commits are rehearsed" >&2
    exit 1
fi
sha="$(git rev-parse HEAD)"

git push --no-follow-tags origin "HEAD:refs/heads/${DEFAULT_BRANCH}"
echo "Rehearsing ${tag} (${sha}):"
echo "  https://gitlab.com/${PROJECT_PATH}/-/pipelines?ref=${DEFAULT_BRANCH}"
echo "Waiting for it to pass; the tag stays local if it fails."
CI_API_V4_URL="https://gitlab.com/api/v4" CI_PROJECT_ID="${PROJECT_ID}" \
    CI_DEFAULT_BRANCH="${DEFAULT_BRANCH}" \
    python3 "${REPO_ROOT}/.gitlab/scripts/release_rehearsal.py" wait "${sha}"
echo "Rehearsal passed; pushing ${tag}"
git push origin "refs/tags/${tag}"
