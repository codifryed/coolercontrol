#!/usr/bin/env bash

# This script creates a source taball that contains a number of files that are not directly in
# version control:
#   - node_modules submodule
#   - pre-compiled HTML and JS code
#
# (We are not packaging node modules)
#
# This enables good reproducability and one can find the origin of everything in a source release.
# One can also run this locally and the checksum should always be the same.

set -eux

# The repository URL
URL="$1"
# The git referrence, i.e. HEAD, tag or branch name
REF="$2"

REPO_DIR="$(mktemp -dt 'coolercontrol-XXXX')"
trap 'rm -rf "${REPO_DIR}"' EXIT
TARBALL_FILE=coolercontrol-"${REF}".tar.gz

git clone \
    --depth=1 \
    --recurse-submodules=coolercontrol-ui/node_modules \
    -b "${REF}" \
    "${URL}" \
    "${REPO_DIR}"

pushd "${REPO_DIR}"
SOURCE_DATE_EPOCH=$(git log -1 --pretty=%ct)

pushd coolercontrol-ui
npm ci --prefer-offline
npm exec vite build -- --outDir ../coolercontrold/resources/app --emptyOutDir
popd
# This is needed to produce reproducable checksums for the tarball
# See: https://reproducible-builds.org/docs/archives/
tar --sort=name \
    --mtime="@${SOURCE_DATE_EPOCH}" \
    --owner=0 --group=0 --numeric-owner \
    --pax-option=exthdr.name=%d/PaxHeaders/%f,delete=atime,delete=ctime \
    --exclude-vcs \
    --transform "s,^\.,coolercontrol-${REF}," \
    -czf ~1/"${TARBALL_FILE}" .
popd
sha256sum "${TARBALL_FILE}"
