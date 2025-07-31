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

# VERSION bumping:
##################
# Valid version arguments are:
# a valid bump rule: patch, minor, major
# if nothing is explicitly specified with `make release` the default is set to patch

echo "Bumping version: $1"
# coolercontrold and bump logic
cd coolercontrold || exit
cargo install cargo-edit
cargo install cargo-get
cargo set-version --offline --bump "$1"
eval NEW_VER="$(cargo get package.version)"
echo "Setting all application version to $NEW_VER"
# ui-qt
cd ../coolercontrol/ || exit
sed -i -E 's|COOLER_CONTROL_VERSION = "[0-9]+\.[0-9]+\.[0-9]+"|COOLER_CONTROL_VERSION = "'"$NEW_VER"'"|' constants.h
# ui
cd ../coolercontrol-ui/ || exit
npm version --allow-same-version --no-commit-hooks --no-git-tag-version --no-workspaces-update "$NEW_VER"
cd ../
echo "New version successfully set: $NEW_VER"
