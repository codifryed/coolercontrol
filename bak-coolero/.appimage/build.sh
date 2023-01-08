#!/bin/bash

#
# Coolero - monitor and control your cooling and other devices
# Copyright (c) 2022  Guy Boldon
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

cd /app/coolero || exit 1
# Install all Poetry dependencies
poetry env use python3.10
poetry install --no-root
make prepare-appimage
cp -f .appimage/appimagetool-x86_64.AppImage /tmp/
sed 's|AI\x02|\x00\x00\x00|g' -i /tmp/appimagetool-x86_64.AppImage
read -r -s -n 1 -p "Press any key to compress and sign..."
echo
make docker-install
