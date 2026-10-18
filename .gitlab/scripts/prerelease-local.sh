#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Guy Boldon and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Usage: prerelease-local.sh (through `make prerelease-local`)
#
# Runs the prerelease CI jobs in docker on the committed HEAD, never the working tree;
# PRERELEASE_ARCHES (default "amd64 arm64") and PRERELEASE_RPM_ARCHES (default "amd64") pick the
# architectures, arm64 under QEMU.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
readonly REPO_ROOT
readonly OUT="${PRERELEASE_OUT:-${REPO_ROOT}/.prerelease}"
readonly IMAGE_TAG="${PRERELEASE_IMAGE_TAG:?set PRERELEASE_IMAGE_TAG to the CI image tag}"
readonly REGISTRY="registry.gitlab.com/coolercontrol/coolercontrol"
readonly ARCHES="${PRERELEASE_ARCHES:-amd64 arm64}"
readonly RPM_ARCHES="${PRERELEASE_RPM_ARCHES:-amd64}"
uid="$(id -u)"
gid="$(id -g)"
readonly UID_GID="${uid}:${gid}"

if ! REF="$(git -C "${REPO_ROOT}" symbolic-ref --short -q HEAD)"; then
    echo "check out a branch first: the source tarball is built from a branch" >&2
    exit 1
fi
readonly REF
# The ref reaches shells and rpm macros, so allow only plain branch-name characters.
if [[ ! ${REF} =~ ^[A-Za-z0-9._/-]+$ ]]; then
    echo "unsupported branch name: ${REF}" >&2
    exit 1
fi
readonly SAFE_REF="${REF//\//-}"
if ! git -C "${REPO_ROOT}" diff --quiet HEAD; then
    echo "Note: uncommitted changes are not tested. This checks the committed ${REF}."
fi

rm -rf "${OUT}/export" "${OUT}/sources" "${OUT}/logs" "${OUT}/rpm"
mkdir -p "${OUT}/export" "${OUT}/sources" "${OUT}/logs" "${OUT}/rpm"
git -C "${REPO_ROOT}" archive HEAD | tar -x -C "${OUT}/export"

echo "Building the source and vendor tarballs for ${REF}"
# shellcheck disable=SC2016 # expanded in the container, from its environment
docker run --rm -v "${REPO_ROOT}":/git:ro -v "${OUT}/export":/repo:ro -v "${OUT}/sources":/out \
    -e REF="${REF}" -e SAFE_REF="${SAFE_REF}" -e UID_GID="${UID_GID}" \
    "${REGISTRY}/pipeline:${IMAGE_TAG}" bash -c '
        set -e
        git config --global --add safe.directory "*"
        cd /out
        /repo/packaging/release-source.sh file:///git "${REF}"
        mkdir /tmp/vendor
        tar -xzf "coolercontrol-${SAFE_REF}.tar.gz" -C /tmp/vendor --strip-components=1
        make -C /tmp/vendor vendor
        cp /tmp/vendor/coolercontrold-vendor.tar.gz /out/
        chown -R "${UID_GID}" /out' >"${OUT}/logs/sources.log" 2>&1 || {
    tail -n 30 "${OUT}/logs/sources.log"
    exit 1
}

# Extra arguments after the image are docker options, such as -e.
run_test() {
    local arch="$1" distro="$2" image="$3"
    shift 3
    docker run --rm --platform "linux/${arch}" \
        -v "${OUT}/export":/repo:ro -v "${OUT}/sources":/sources:ro \
        -v "cc-prerelease-${arch}-${distro}":/cache -e CARGO_TARGET_DIR=/cache/target "$@" \
        "${image}" bash /repo/.gitlab/scripts/prerelease-test.sh \
        "/sources/coolercontrol-${SAFE_REF}.tar.gz" /sources/coolercontrold-vendor.tar.gz
}

run_rpm() {
    local arch="$1"
    # shellcheck disable=SC2016 # expanded in the container, from its environment
    docker run --rm --privileged --platform "linux/${arch}" \
        -v "${OUT}/export":/repo:ro -v "${OUT}/sources":/sources:ro -v "${OUT}/rpm/${arch}":/results \
        -e PRERELEASE_RESULT_DIR=/results -e REF="${REF}" -e UID_GID="${UID_GID}" \
        fedora:latest bash -c '
            bash /repo/.gitlab/scripts/prerelease-rpm.sh /sources "${REF}"
            status=$?
            chown -R "${UID_GID}" /results
            exit "${status}"'
}

declare -A pids=()
for arch in ${ARCHES}; do
    for distro in ubuntu-22.04 ubuntu-rolling; do
        env_args=()
        case "${distro}" in
        ubuntu-22.04) image="${REGISTRY}/ubuntu:${IMAGE_TAG}" ;;
        ubuntu-rolling)
            image="ubuntu:rolling"
            # As the CI rolling jobs: the newest stable Rust, warnings denied.
            env_args=(-e RUST_VERSION=stable -e PRERELEASE_DENY_WARNINGS=1)
            ;;
        *)
            echo "no image for ${distro}" >&2
            exit 1
            ;;
        esac
        name="test-${arch}-${distro}"
        run_test "${arch}" "${distro}" "${image}" "${env_args[@]}" >"${OUT}/logs/${name}.log" 2>&1 &
        pids["${name}"]=$!
    done
done
for arch in ${RPM_ARCHES}; do
    mkdir -p "${OUT}/rpm/${arch}"
    name="rpm-${arch}"
    run_rpm "${arch}" >"${OUT}/logs/${name}.log" 2>&1 &
    pids["${name}"]=$!
done

echo "Running ${!pids[*]}; logs in ${OUT}/logs"
failed=0
for name in $(printf '%s\n' "${!pids[@]}" | sort); do
    if wait "${pids[${name}]}"; then
        echo "PASS ${name}"
    else
        echo "FAIL ${name} (${OUT}/logs/${name}.log)"
        failed=1
    fi
done
exit "${failed}"
