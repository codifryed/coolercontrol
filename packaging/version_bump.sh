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

echo "Bumping version: $1"
# liqctld
cd coolercontrol-liqctld || exit
poetry version "$1"
eval NEW_VER="$(poetry version -s)"
echo "Setting application version to $NEW_VER"
sed -i -E "s|__version__: str = '[0-9]+\.[0-9]+\.[0-9]+'|__version__: str = '""$NEW_VER""'|" coolercontrol_liqctld/liqctld.py
# gui
cd ../coolercontrol-gui || exit
poetry version "$1"
sed -i -E 's|"version": "[0-9]+\.[0-9]+\.[0-9]+"|"version": "'"$NEW_VER"'"|' coolercontrol/resources/settings.json
# cargo version update
cd ../coolercontrold || exit
cargo install cargo-edit
cargo set-version --bump "$1"
echo "New version set: $NEW_VER"
