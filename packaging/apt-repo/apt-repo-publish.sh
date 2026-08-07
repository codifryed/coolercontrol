#!/usr/bin/env bash
# Publish debs into the CoolerControl apt repository.
#
#   apt-repo-publish --repo debian|ubuntu|both [--keep N] <deb>...
#
# The repository lives in a Cloudflare R2 bucket as two independent trees under /debian and
# /ubuntu. Two trees, not two suites: bookworm-built and jammy-built debs share file names but
# not contents, so one pool cannot hold both.
#
# The published pool is the only state. Each run pulls it, rebuilds the index from it plus the
# new packages, and pushes the result. There is no separate database to keep in sync, lose, or
# recover, and the recovery path is the normal path.
#
# Set APT_REPO_DEST to a local directory to publish against a stand-in bucket (testing and
# disaster recovery). Otherwise R2 credentials are required.
set -euo pipefail

readonly SCRIPT_NAME=${0##*/}
readonly SHARE_DIR=${APT_REPO_SHARE:-/usr/local/share/apt-repo}
readonly SIGN_KEY=${APT_REPO_SIGN_KEY:-668189E5007F5A8D}
readonly ORIGIN=CoolerControl
readonly LABEL=CoolerControl
readonly DISTRIBUTION=stable
readonly COMPONENT=main
readonly ARCHITECTURES=amd64,arm64
# How many versions of each package stay installable. Users need a rollback target, especially
# across a major release.
readonly DEFAULT_KEEP=3
readonly LOCK_OBJECT=locks/publish.lock
readonly LOCK_STALE_SECONDS=1800
readonly LOCK_WAIT_SECONDS=600
readonly LOCK_POLL_SECONDS=15
# Pool files are immutable by name, dists changes every publish. no-transform stops Cloudflare
# decompressing Packages.gz, which would break apt's hash check.
readonly POOL_CACHE_HEADER='Cache-Control: public, max-age=2592000, no-transform'
readonly META_CACHE_HEADER='Cache-Control: no-cache, no-transform'

work=
lock_held=0
gnupg_home=

log() { printf '==> %s\n' "$*"; }
die() {
    printf '%s: %s\n' "${SCRIPT_NAME}" "$*" >&2
    exit 1
}

usage() {
    cat <<EOF
Usage: ${SCRIPT_NAME} --repo debian|ubuntu|both [--keep N] <deb>...

Options:
  --repo    which tree to publish into, or both for architecture independent packages
  --keep    versions of each package to keep installable (default ${DEFAULT_KEEP})

Environment:
  GPG_KEY               path to the armored signing key (GitLab file variable)
  GPG_PHRASE            passphrase for that key
  R2_ACCOUNT_ID         Cloudflare account id
  R2_BUCKET             R2 bucket name
  R2_ACCESS_KEY_ID      R2 token access key id
  R2_SECRET_ACCESS_KEY  R2 token secret
  APT_REPO_DEST         optional rclone destination overriding the R2 remote
  APT_REPO_SHARE        optional path to the baked keys and setup.sh (default ${SHARE_DIR})
  APT_REPO_SIGN_KEY     optional signing key override (default ${SIGN_KEY})
EOF
}

cleanup() {
    local status=$?
    if [[ ${lock_held} -eq 1 ]]; then
        rclone deletefile "${dest}/${LOCK_OBJECT}" 2>/dev/null || true
    fi
    if [[ -n ${gnupg_home} ]]; then
        gpgconf --kill gpg-agent 2>/dev/null || true
        rm -rf "${gnupg_home}"
    fi
    [[ -n ${work} ]] && rm -rf "${work}"
    exit "${status}"
}

require_env() {
    local var
    for var in "$@"; do
        [[ -n ${!var-} ]] || die "missing required environment variable: ${var}"
    done
}

# aptly shells out to gpg. Import into a throwaway home and warm the agent so no pinentry prompt
# is ever needed. Same pattern as build_appimages in .gitlab-ci.yml.
setup_gpg() {
    gnupg_home=$(mktemp -d)
    chmod 700 "${gnupg_home}"
    export GNUPGHOME=${gnupg_home}
    printf 'allow-loopback-pinentry\ndefault-cache-ttl 7200\nmax-cache-ttl 7200\n' \
        >"${gnupg_home}/gpg-agent.conf"
    gpg --batch --quiet --import "${GPG_KEY}"
    printf '%s' "${GPG_PHRASE}" >"${work}/passphrase"
    chmod 600 "${work}/passphrase"
    local warmup
    warmup=$(mktemp)
    gpg --batch --yes --always-trust --passphrase-file "${work}/passphrase" \
        --pinentry-mode loopback --sign --output /dev/null "${warmup}"
    log "signing key ${SIGN_KEY} ready"
}

# Best effort mutual exclusion between the tag pipeline and the weekly sibling schedules. Not
# bulletproof: two publishers starting in the same second can both take it. Acceptable because
# the schedules are staggered and any tree can be rebuilt from its own pool.
lock_acquire() {
    local waited=0 body epoch now
    while :; do
        body=$(rclone cat "${dest}/${LOCK_OBJECT}" 2>/dev/null || true)
        epoch=$(printf '%s' "${body}" | sed -n 's/^epoch=//p')
        if [[ -n ${epoch} ]]; then
            now=$(date +%s)
            if [[ "$((now - epoch))" -lt ${LOCK_STALE_SECONDS} ]]; then
                [[ ${waited} -lt ${LOCK_WAIT_SECONDS} ]] ||
                    die "publish lock held by another job for over ${LOCK_WAIT_SECONDS}s"
                log "publish lock held, waiting ${LOCK_POLL_SECONDS}s"
                sleep "${LOCK_POLL_SECONDS}"
                waited=$((waited + LOCK_POLL_SECONDS))
                continue
            fi
            log "publish lock is stale, taking it over"
        fi
        break
    done
    now=$(date +%s)
    printf 'epoch=%s\njob=%s\n' "${now}" "${CI_JOB_URL:-local}" >"${work}/publish.lock"
    rclone copyto "${work}/publish.lock" "${dest}/${LOCK_OBJECT}"
    lock_held=1
}

# Newest first, using dpkg version ordering rather than string or -V ordering, which get epochs
# and ~ suffixes wrong.
sort_versions_desc() {
    local versions=() v i j tmp
    while read -r v; do
        [[ -n ${v} ]] && versions+=("${v}")
    done
    for ((i = 0; i < ${#versions[@]}; i++)); do
        for ((j = i + 1; j < ${#versions[@]}; j++)); do
            if dpkg --compare-versions "${versions[j]}" gt "${versions[i]}"; then
                tmp=${versions[i]}
                versions[i]=${versions[j]}
                versions[j]=${tmp}
            fi
        done
    done
    [[ ${#versions[@]} -eq 0 ]] || printf '%s\n' "${versions[@]}"
}

# aptly refs look like name_version_arch. Keep the newest $keep versions of each package name and
# drop the rest, so the pool stays bounded while rollback targets remain installable.
apply_retention() {
    local conf=$1 repo=$2 name versions v kept dropped=0
    for name in $(aptly -config="${conf}" repo search "${repo}" 2>/dev/null |
        sed 's/_[^_]*_[^_]*$//' | sort -u); do
        versions=$(aptly -config="${conf}" repo search "${repo}" "Name (= ${name})" 2>/dev/null |
            sed "s/^${name}_//; s/_[^_]*$//" | sort -u | sort_versions_desc)
        kept=0
        while read -r v; do
            [[ -n ${v} ]] || continue
            kept=$((kept + 1))
            [[ ${kept} -le ${keep} ]] && continue
            log "retention: dropping ${name} ${v}"
            # Note the = operator: Debian treats < as <=, so a range query would take the
            # version we mean to keep with it.
            aptly -config="${conf}" repo remove "${repo}" "${name} (= ${v})" >/dev/null
            dropped=$((dropped + 1))
        done <<<"${versions}"
    done
    [[ ${dropped} -eq 0 ]] || log "retention: removed ${dropped} superseded version(s)"
}

publish_tree() {
    local tree=$1
    shift
    local base="${work}/${tree}"
    local repo="cc-${tree}"
    local pool="${base}/served-pool"
    local public="${base}/public/${tree}"

    mkdir -p "${base}" "${pool}"
    cat >"${base}/aptly.conf" <<EOF
{"rootDir": "${base}", "gpgProvider": "gpg"}
EOF
    local aptly_cmd=(aptly -config="${base}/aptly.conf")

    if rclone lsf "${dest}/${tree}/pool" >/dev/null 2>&1; then
        log "[${tree}] pulling published pool"
        rclone copy "${dest}/${tree}/pool" "${pool}"
    else
        log "[${tree}] nothing published yet, starting a new tree"
    fi

    "${aptly_cmd[@]}" repo create -distribution="${DISTRIBUTION}" -component="${COMPONENT}" \
        "${repo}" >/dev/null
    # Existing packages first, then the new ones. Re-adding an identical file is a no-op, which
    # is what makes a retried job safe.
    if find "${pool}" -name '*.deb' -print -quit | grep -q .; then
        "${aptly_cmd[@]}" repo add -force-replace "${repo}" "${pool}" >/dev/null
    fi
    log "[${tree}] adding $# new package(s)"
    "${aptly_cmd[@]}" repo add -force-replace "${repo}" "$@" >/dev/null

    apply_retention "${base}/aptly.conf" "${repo}"

    "${aptly_cmd[@]}" publish repo -architectures="${ARCHITECTURES}" -distribution="${DISTRIBUTION}" \
        -component="${COMPONENT}" -origin="${ORIGIN}" -label="${LABEL}" -skip-contents \
        -gpg-key="${SIGN_KEY}" -passphrase-file="${work}/passphrase" -batch \
        "${repo}" "${tree}" >/dev/null

    [[ -f "${public}/dists/${DISTRIBUTION}/InRelease" ]] || die "[${tree}] publish produced no InRelease"
    local count
    count=$(find "${public}/pool" -name '*.deb' | wc -l)
    # The pool sync below deletes anything not present locally, so refuse to run it against an
    # obviously broken publish.
    [[ ${count} -ge $# ]] || die "[${tree}] published pool holds ${count} debs, expected at least $#"
    log "[${tree}] publishing ${count} package file(s)"

    # Three steps, no inconsistent window: new files exist before the metadata names them, and
    # superseded files disappear only after the metadata stops naming them.
    rclone copy "${public}/pool" "${dest}/${tree}/pool" "${pool_flags[@]}"
    rclone sync "${public}/dists" "${dest}/${tree}/dists" "${meta_flags[@]}"
    rclone sync "${public}/pool" "${dest}/${tree}/pool" "${pool_flags[@]}"
}

refresh_root_objects() {
    log "refreshing keyring and installer"
    rclone copyto "${SHARE_DIR}/setup.sh" "${dest}/setup.sh" "${meta_flags[@]}"
    rclone copyto "${SHARE_DIR}/keys/coolercontrol-archive-keyring.gpg" \
        "${dest}/coolercontrol-archive-keyring.gpg" "${meta_flags[@]}"
    rclone copyto "${SHARE_DIR}/keys/coolercontrol.asc" "${dest}/coolercontrol.asc" \
        "${meta_flags[@]}"
}

repo=
keep=${DEFAULT_KEEP}
debs=()
while [[ $# -gt 0 ]]; do
    case $1 in
    --repo)
        [[ $# -ge 2 ]] || die "--repo needs a value"
        repo=$2
        shift 2
        ;;
    --keep)
        [[ $# -ge 2 ]] || die "--keep needs a value"
        keep=$2
        shift 2
        ;;
    -h | --help)
        usage
        exit 0
        ;;
    --)
        shift
        debs+=("$@")
        break
        ;;
    -*) die "unknown option: $1" ;;
    *)
        debs+=("$1")
        shift
        ;;
    esac
done

case ${repo} in
debian | ubuntu) trees=("${repo}") ;;
both) trees=(debian ubuntu) ;;
"") die "--repo is required" ;;
*) die "--repo must be debian, ubuntu or both" ;;
esac

case ${keep} in
'' | *[!0-9]*) die "--keep must be a positive integer" ;;
0) die "--keep must be at least 1" ;;
*) ;;
esac

[[ ${#debs[@]} -gt 0 ]] || die "no packages given"
for deb in "${debs[@]}"; do
    [[ -f ${deb} ]] || die "no such file: ${deb}"
    case ${deb} in
    *.deb) ;;
    *) die "not a deb: ${deb}" ;;
    esac
done

require_env GPG_KEY GPG_PHRASE
[[ -f "${SHARE_DIR}/keys/coolercontrol-archive-keyring.gpg" ]] || die "no keyring at ${SHARE_DIR}/keys"

# A local destination stands in for the bucket, which keeps the whole script testable without
# credentials. Upload headers only mean something on the S3 backend.
pool_flags=()
meta_flags=()
dest=${APT_REPO_DEST-}
if [[ -z ${dest} ]]; then
    pool_flags=(--header-upload "${POOL_CACHE_HEADER}")
    meta_flags=(--header-upload "${META_CACHE_HEADER}")
    require_env R2_ACCOUNT_ID R2_BUCKET R2_ACCESS_KEY_ID R2_SECRET_ACCESS_KEY
    export RCLONE_CONFIG_R2_TYPE=s3
    export RCLONE_CONFIG_R2_PROVIDER=Cloudflare
    export RCLONE_CONFIG_R2_REGION=auto
    export RCLONE_CONFIG_R2_ENDPOINT="https://${R2_ACCOUNT_ID}.r2.cloudflarestorage.com"
    export RCLONE_CONFIG_R2_ACCESS_KEY_ID=${R2_ACCESS_KEY_ID}
    export RCLONE_CONFIG_R2_SECRET_ACCESS_KEY=${R2_SECRET_ACCESS_KEY}
    export RCLONE_CONFIG_R2_ACL=private
    # A bucket scoped token cannot HeadBucket.
    export RCLONE_S3_NO_CHECK_BUCKET=true
    dest="R2:${R2_BUCKET}"
fi
readonly dest

trap cleanup EXIT INT TERM
work=$(mktemp -d)
setup_gpg
lock_acquire

for tree in "${trees[@]}"; do
    publish_tree "${tree}" "${debs[@]}"
done
refresh_root_objects

log "published to ${dest}"
