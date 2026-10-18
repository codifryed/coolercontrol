#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Guy Boldon and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# prerelease-test.sh - run the daemon test suite from the shipped source tarballs.
#
# Usage: prerelease-test.sh <source-tarball> <vendor-tarball>
#
# Tests what distros build: the source tarball (UI already built in) with the vendored crates,
# offline. First the default suite in release mode, exactly as `make test` runs downstream, then
# the gated tests, which only our own CI runs. Shared by the CI prerelease jobs and
# `make prerelease-local`.
#
# Installs a toolchain on a Debian-family image that has none, pinned to the build images'
# RUST_VERSION. CARGO_TARGET_DIR is honoured, so a local run can keep its build between runs.

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <source-tarball> <vendor-tarball>" >&2
    exit 2
fi
readonly SOURCE_TARBALL="$1"
readonly VENDOR_TARBALL="$2"
REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
readonly REPO_ROOT
# A fixed path, so a cached target directory stays valid between runs.
readonly SOURCE_DIR="${PRERELEASE_SOURCE_DIR:-/tmp/prerelease-src}"

install_toolchain() {
    local rust_version
    rust_version="${RUST_VERSION-}"
    if [[ -z ${rust_version} ]]; then
        rust_version="$(sed -n 's/.*RUST_VERSION=\([0-9][0-9.]*\).*/\1/p;T;q' \
            "${REPO_ROOT}/.gitlab/images/ubuntu/Dockerfile")"
    fi
    if [[ -z ${rust_version} ]]; then
        echo "no RUST_VERSION in .gitlab/images/ubuntu/Dockerfile" >&2
        exit 1
    fi
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        build-essential ca-certificates curl libdrm-dev pkg-config
    export RUSTUP_HOME="${RUSTUP_HOME:-/usr/local/rustup}"
    export CARGO_HOME="${CARGO_HOME:-/usr/local/cargo}"
    curl -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal \
        --default-toolchain "${rust_version}"
    export PATH="${CARGO_HOME}/bin:${PATH}"
}

if ! command -v cargo >/dev/null; then
    install_toolchain
fi

# shellcheck source=/dev/null
. /etc/os-release
arch="$(uname -m)"
glibc="$(ldd --version | sed -n 1p)"
rustc_version="$(rustc --version)"
echo "Distro - ${PRETTY_NAME}"
echo "Arch - ${arch}"
echo "glibc - ${glibc}"
echo "rustc - ${rustc_version}"

rm -rf "${SOURCE_DIR}"
mkdir -p "${SOURCE_DIR}"
tar -xzf "${SOURCE_TARBALL}" -C "${SOURCE_DIR}" --strip-components=1
tar -xzf "${VENDOR_TARBALL}" -C "${SOURCE_DIR}/coolercontrold"

# Distro builds do not deny warnings, and a newer compiler's warnings are not test failures.
export RUSTFLAGS=""
# The default suite, as distros run it.
make -C "${SOURCE_DIR}/coolercontrold" test
# The gated tests, which depend on the build environment and run only in our CI.
make -C "${SOURCE_DIR}/coolercontrold" test FEATURES="--features gated-tests"
