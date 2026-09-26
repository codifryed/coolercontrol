#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Guy Boldon and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# cargo-audit.sh - fail on RustSec findings in the daemon lockfile.
#
# In a scheduled pipeline it also bumps each flagged crate within its semver
# range and opens, or refreshes, a single MR with the new Cargo.lock. Fixes
# that need a Cargo.toml change stay red for a human to make.
#
# Exits non-zero while any finding remains on the checked-out ref.

set -euo pipefail

readonly BRANCH="security/cargo-audit"
readonly TARGET="main"
# Repo path, for the API. cargo audit reads .cargo/audit.toml from the
# working directory, so the script runs inside the workspace.
readonly LOCK="coolercontrold/Cargo.lock"
readonly TITLE="fix(deps): update vulnerable crates"
# unmaintained stays a warning: a bump cannot fix it.
readonly AUDIT=(cargo audit --deny unsound --deny yanked)

cd coolercontrold
if "${AUDIT[@]}"; then
    exit 0
fi
if [[ ${CI_PIPELINE_SOURCE-} != "schedule" ]]; then
    exit 1
fi

report=$("${AUDIT[@]}" --json 2>/dev/null || true)
readonly FINDINGS='[.vulnerabilities.list[], (.warnings.unsound // [])[], (.warnings.yanked // [])[]]'
specs=$(jq -r "${FINDINGS} | map(\"\(.package.name)@\(.package.version)\") | unique | .[]" <<<"${report}")
if [[ -z ${specs} ]]; then
    echo "No finding can be cleared by a lockfile bump."
    exit 1
fi

changes=""
mapfile -t flagged <<<"${specs}"
for spec in "${flagged[@]}"; do
    echo "cargo update -p ${spec}"
    if out=$(cargo update -p "${spec}" 2>&1); then
        changes+=$(grep -E '^ *(Updating|Adding|Removing) [^ ]+ v[0-9]' <<<"${out}" | sed -E 's/^ +/- /' || true)$'\n'
    else
        echo "${out}"
    fi
done

if git diff --quiet -- Cargo.lock; then
    echo "No flagged crate could be bumped within its semver range. Fix by hand."
    exit 1
fi

remaining=0
"${AUDIT[@]}" || remaining=1

readonly API="${CI_API_V4_URL:?}/projects/${CI_PROJECT_ID:?}"
readonly AUTH="PRIVATE-TOKEN: ${SECURITY_BOT_GITLAB_TOKEN:?}"

api() {
    curl --fail-with-body --silent --show-error --header "${AUTH}" "$@"
}

# Rebuild the branch from the target only when the computed lockfile differs,
# so an unchanged finding does not rerun the MR pipeline every day.
lock_path=$(jq -rn --arg p "${LOCK}" '$p | @uri')
current=$(curl --fail --silent --header "${AUTH}" \
    "${API}/repository/files/${lock_path}/raw?ref=${BRANCH//\//%2F}") || current=""
computed=$(<Cargo.lock)
if [[ ${current} != "${computed}" ]]; then
    jq -n --arg branch "${BRANCH}" --arg start "${TARGET}" --arg msg "${TITLE}" \
        --arg path "${LOCK}" --rawfile content Cargo.lock \
        '{branch: $branch, start_branch: $start, force: true, commit_message: $msg,
		  actions: [{action: "update", file_path: $path, content: $content}]}' |
        api --request POST --header "Content-Type: application/json" --data @- \
            "${API}/repository/commits" >/dev/null
    echo "Pushed ${BRANCH}."
fi

description=$(
    jq -r "\"Advisories:\n\" + (${FINDINGS} | map(\"- \(.advisory.id // \"yanked\") \`\(.package.name)\` \(.package.version): \(.advisory.title // \"yanked from crates.io\")\") | unique | join(\"\n\"))" <<<"${report}"
    printf '\nLockfile changes:\n%s' "${changes}"
    if ((remaining)); then
        printf '\nSome findings remain after this bump and need a Cargo.toml change. See the job log.\n'
    fi
    printf '\nOpened by the scheduled cargo_audit job.\n'
)

mr=$(api "${API}/merge_requests?state=opened&source_branch=${BRANCH//\//%2F}&target_branch=${TARGET}" | jq -r '.[0].iid // empty')
if [[ -z ${mr} ]]; then
    jq -n --arg src "${BRANCH}" --arg tgt "${TARGET}" --arg title "${TITLE}" --arg desc "${description}" \
        --argjson assignee "${GITLAB_USER_ID:?}" \
        '{source_branch: $src, target_branch: $tgt, title: $title, description: $desc,
		  assignee_id: $assignee, labels: "security", squash: true, remove_source_branch: true}' |
        api --request POST --header "Content-Type: application/json" --data @- \
            "${API}/merge_requests" | jq -r '"Opened \(.web_url)"'
else
    jq -n --arg desc "${description}" '{description: $desc}' |
        api --request PUT --header "Content-Type: application/json" --data @- \
            "${API}/merge_requests/${mr}" | jq -r '"Updated \(.web_url)"'
fi

exit 1
