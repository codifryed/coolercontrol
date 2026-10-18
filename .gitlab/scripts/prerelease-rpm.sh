#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Guy Boldon and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# prerelease-rpm.sh - build the Fedora RPMs from the shipped source tarballs in mock.
#
# Usage: prerelease-rpm.sh <sources-dir> <ref>
#
# <sources-dir> holds what build_source_assets makes for <ref>: coolercontrol-<ref>.tar.gz and
# the coolercontrold vendor tarballs. With PRERELEASE_VERSION set (a release rehearsal), builds
# the release specs after checking that their Version is that version, from <ref>'s tarballs
# repacked under the version's names. Otherwise builds the rc1 specs against <ref>, as the manual
# Copr check did. mock builds offline in a fresh chroot, as Copr does, so %check runs the
# default test suite under Fedora's own Rust. Needs a privileged Fedora container. Shared by the
# CI prerelease jobs and `make prerelease-local`.

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <sources-dir> <ref>" >&2
    exit 2
fi
readonly SOURCES_DIR="$1"
readonly REF="$2"
# The ref reaches an rpm macro, where %(...) would run.
if [[ ! ${REF} =~ ^[A-Za-z0-9._/-]+$ ]]; then
    echo "unsupported ref: ${REF}" >&2
    exit 2
fi
readonly SAFE_REF="${REF//\//-}"
REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
readonly REPO_ROOT
readonly SPEC_DIR="${REPO_ROOT}/packaging/fedora"
readonly RESULT_DIR="${PRERELEASE_RESULT_DIR:-${PWD}/rpm-results}"
readonly STAGING_DIR="/tmp/prerelease-rpm-sources"
readonly VERSION="${PRERELEASE_VERSION-}"

dnf -y install mock rpm-build

# Only the tarballs the specs name, so mock does not copy a whole checkout into the chroot.
rm -rf "${STAGING_DIR}"
mkdir -p "${STAGING_DIR}"
defines=()
if [[ -n ${VERSION} ]]; then
    specs=("${SPEC_DIR}/coolercontrold.spec" "${SPEC_DIR}/coolercontrol.spec")
    for spec in "${specs[@]}"; do
        spec_version="$(rpmspec -q --srpm --qf '%{version}' "${spec}")"
        if [[ ${spec_version} != "${VERSION}" ]]; then
            echo "$(basename "${spec}") is version ${spec_version}, not ${VERSION}" >&2
            exit 1
        fi
    done
    # The release specs unpack coolercontrol-<version>/, which the tag's tarball will hold.
    mkdir -p "${STAGING_DIR}/repack"
    tar -xzf "${SOURCES_DIR}/coolercontrol-${SAFE_REF}.tar.gz" -C "${STAGING_DIR}/repack"
    mv "${STAGING_DIR}/repack/coolercontrol-${SAFE_REF}" "${STAGING_DIR}/repack/coolercontrol-${VERSION}"
    tar -czf "${STAGING_DIR}/coolercontrol-${VERSION}.tar.gz" -C "${STAGING_DIR}/repack" \
        "coolercontrol-${VERSION}"
    rm -rf "${STAGING_DIR}/repack"
    cp "${SOURCES_DIR}/coolercontrold-vendor.tar.gz" \
        "${STAGING_DIR}/coolercontrold-vendor-${VERSION}.tar.gz"
else
    cp "${SOURCES_DIR}/coolercontrol-${SAFE_REF}.tar.gz" "${STAGING_DIR}/"
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
