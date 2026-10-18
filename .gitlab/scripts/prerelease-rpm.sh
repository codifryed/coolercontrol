#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Guy Boldon and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# prerelease-rpm.sh - build the Fedora RPMs from the shipped source tarballs in mock.
#
# Usage: prerelease-rpm.sh <sources-dir> <ref>
#
# <sources-dir> holds what build_source_assets makes for <ref>: coolercontrol-<ref>.tar.gz and
# the coolercontrold vendor tarballs. With PRERELEASE_TAG set, builds the release specs after
# checking that their Version is the tag. Otherwise builds the rc1 specs against <ref>, as the
# manual Copr check did. mock builds offline in a fresh chroot, as Copr does, so %check runs the
# default test suite under Fedora's own Rust. Needs a privileged Fedora container. Shared by the
# CI prerelease jobs and `make prerelease-local`.

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <sources-dir> <ref>" >&2
    exit 2
fi
readonly SOURCES_DIR="$1"
readonly REF="$2"
readonly SAFE_REF="${REF//\//-}"
REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
readonly REPO_ROOT
readonly SPEC_DIR="${REPO_ROOT}/packaging/fedora"
readonly RESULT_DIR="${PRERELEASE_RESULT_DIR:-${PWD}/rpm-results}"
readonly STAGING_DIR="/tmp/prerelease-rpm-sources"
readonly TAG="${PRERELEASE_TAG-}"

dnf -y install mock rpm-build

# Only the tarballs the specs name, so mock does not copy a whole checkout into the chroot.
rm -rf "${STAGING_DIR}"
mkdir -p "${STAGING_DIR}"
cp "${SOURCES_DIR}/coolercontrol-${SAFE_REF}.tar.gz" "${STAGING_DIR}/"
defines=()
if [[ -n ${TAG} ]]; then
    if [[ ${TAG} != "${SAFE_REF}" ]]; then
        echo "PRERELEASE_TAG ${TAG} does not match the ref ${REF}" >&2
        exit 1
    fi
    cp "${SOURCES_DIR}/coolercontrold-vendor-${SAFE_REF}.tar.gz" "${STAGING_DIR}/"
    specs=("${SPEC_DIR}/coolercontrold.spec" "${SPEC_DIR}/coolercontrol.spec")
    for spec in "${specs[@]}"; do
        version="$(rpmspec -q --srpm --qf '%{version}' "${spec}")"
        if [[ ${version} != "${TAG}" ]]; then
            echo "$(basename "${spec}") is version ${version}, not the tag ${TAG}" >&2
            exit 1
        fi
    done
else
    cp "${SOURCES_DIR}/coolercontrold-vendor.tar.gz" "${STAGING_DIR}/"
    specs=("${SPEC_DIR}/coolercontrold-rc1.spec" "${SPEC_DIR}/coolercontrol-rc1.spec")
    defines=(--define "branch ${SAFE_REF}")
fi

# The chroot matches the container's Fedora release and architecture.
fedora="$(rpm -E '%fedora')"
arch="$(uname -m)"
readonly ROOT="fedora-${fedora}-${arch}"
echo "mock root - ${ROOT}"
for spec in "${specs[@]}"; do
    name="$(basename "${spec}" .spec)"
    # simple isolation and no bootstrap image: a container has neither systemd-nspawn nor podman.
    mock -r "${ROOT}" --isolation=simple --no-bootstrap-image "${defines[@]}" \
        --spec "${spec}" --sources "${STAGING_DIR}" --resultdir "${RESULT_DIR}/${name}"
done
