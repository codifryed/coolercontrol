#!/usr/bin/env bash

# Coolero - monitor and control your cooling and other devices
# Copyright (c) 2021  Guy Boldon
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

eval RELEASE_VERSION="$(poetry version -s)"
git add CHANGELOG.md pyproject.toml coolero/resources/settings.json
git commit -S -m "Release ${RELEASE_VERSION}"
git tag -s "$RELEASE_VERSION" -m "$RELEASE_VERSION"