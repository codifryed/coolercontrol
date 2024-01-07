#!/usr/bin/env bash

# CoolerControl - monitor and control your cooling and other devices
# Copyright (c) 2023  Guy Boldon
# |
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
# |
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
# |
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.
#

# this is run AFTER version_bump.sh
cd coolercontrold || exit
eval RELEASE_VERSION="$(cargo get package.version)"
cd ..
git add CHANGELOG.md \
    coolercontrold/Cargo.toml \
    coolercontrold/Cargo.lock \
    coolercontrol-liqctld/pyproject.toml \
    coolercontrol-liqctld/setup.cfg \
    coolercontrol-liqctld/coolercontrol_liqctld/liqctld.py \
    coolercontrol-ui/package.json \
    coolercontrol-ui/package-lock.json \
    coolercontrol-ui/src-tauri/Cargo.toml \
    coolercontrol-ui/src-tauri/Cargo.lock \
    coolercontrol-ui/src-tauri/tauri.conf.json \
    packaging/metadata/org.coolercontrol.CoolerControl.metainfo.xml \
    packaging/fedora/coolercontrol.spec \
    packaging/fedora/coolercontrold.spec \
    packaging/fedora/coolercontrol-liqctld.spec \
    packaging/debian/changelog
git commit -S -m "Release ${RELEASE_VERSION}"
git tag -s "$RELEASE_VERSION" -m "$RELEASE_VERSION"
