#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2021 Guy Boldon and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# this is run AFTER version_bump.sh
cd coolercontrold || exit
pushd daemon
eval RELEASE_VERSION="$(cargo get package.version)"
popd
cd ..
git add CHANGELOG.md \
    coolercontrold/daemon/Cargo.toml \
    coolercontrold/cc-detect/Cargo.toml \
    coolercontrold/cc-image/Cargo.toml \
    coolercontrold/cc-stress/Cargo.toml \
    coolercontrold/Cargo.lock \
    coolercontrol-ui/package.json \
    coolercontrol-ui/package-lock.json \
    coolercontrol/constants.h \
    packaging/metadata/org.coolercontrol.CoolerControl.metainfo.xml \
    packaging/fedora/coolercontrol.spec \
    packaging/fedora/coolercontrold.spec \
    packaging/fedora/coolercontrol-rc1.spec \
    packaging/fedora/coolercontrold-rc1.spec \
    packaging/debian/changelog \
    openapi/openapi.json
# --allow-empty: after a failed rehearsal, the fixed release gets a new, empty Release commit.
git commit -S --allow-empty -m "Release ${RELEASE_VERSION}"
git tag -s "$RELEASE_VERSION" -m "$RELEASE_VERSION"
