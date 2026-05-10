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
# Updates the source-of-truth versions (cargo, constants.h, package.json), then
# propagates the new version + current UTC date to every version-stamped
# packaging file: metainfo.xml, debian/changelog, fedora *.spec (regular and
# rc1), OBS .dsc and include-binaries. Substitutions match by regex rather than
# the previous version, so the script self-heals if files were out of sync.

set -euo pipefail

echo "Bumping version: $1"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

# coolercontrold daemon (cargo bump is the source of truth for NEW_VER)
cd coolercontrold/daemon
cargo install cargo-edit
cargo install cargo-get
cargo set-version --offline --bump "$1"
eval NEW_VER="$(cargo get package.version)"
cd "${REPO_ROOT}"
echo "Setting all application version to $NEW_VER"

# ui-qt
cd coolercontrol
sed -i -E 's|COOLER_CONTROL_VERSION = "[0-9]+\.[0-9]+\.[0-9]+"|COOLER_CONTROL_VERSION = "'"$NEW_VER"'"|' constants.h
cd "${REPO_ROOT}"

# ui
cd coolercontrol-ui
npm version --allow-same-version --no-commit-hooks --no-git-tag-version --no-workspaces-update "$NEW_VER"
cd "${REPO_ROOT}"

# rc1 fedora specs lead the regular specs by one patch (e.g., regular 4.2.2 -> rc1 4.2.3~rc1).
IFS='.' read -ra NEW_PARTS <<<"${NEW_VER}"
NEW_RC_VER="${NEW_PARTS[0]}.${NEW_PARTS[1]}.$((NEW_PARTS[2] + 1))~rc1"

# Force C locale so month/day abbreviations are English regardless of system locale.
DATE_ISO=$(LC_ALL=C date -u +%Y-%m-%d)
DATE_DEB=$(LC_ALL=C date -uR)
DATE_RPM=$(LC_ALL=C date -u +"%a %b %d %Y")

# metainfo.xml: prepend new <release> entry just inside <releases>
metainfo=packaging/metadata/org.coolercontrol.CoolerControl.metainfo.xml
awk -v ver="${NEW_VER}" -v iso="${DATE_ISO}" '
    /<releases>/ {
        print
        print "        <release version=\"" ver "\" date=\"" iso "\"/>"
        next
    }
    { print }
' "${metainfo}" >"${metainfo}.tmp"
mv "${metainfo}.tmp" "${metainfo}"

# debian/changelog: prepend new UNRELEASED entry block
deb_changelog=packaging/debian/changelog
{
    printf 'coolercontrol (%s) UNRELEASED; urgency=medium\n\n  * %s Release\n\n -- Guy Boldon <gb@guyboldon.com>  %s\n\n' \
        "${NEW_VER}" "${NEW_VER}" "${DATE_DEB}"
    cat "${deb_changelog}"
} >"${deb_changelog}.tmp"
mv "${deb_changelog}.tmp" "${deb_changelog}"

# fedora regular specs: rewrite Version: line + prepend %changelog entry
for spec in packaging/fedora/coolercontrol.spec packaging/fedora/coolercontrold.spec; do
    sed -i -E "s/^(Version:[[:space:]]+)[0-9]+\.[0-9]+\.[0-9]+\$/\1${NEW_VER}/" "${spec}"
    awk -v ver="${NEW_VER}" -v rpm="${DATE_RPM}" '
        /^%changelog$/ {
            print
            print "* " rpm " Guy Boldon <gb@guyboldon.com> - " ver "-1"
            print "- " ver " Release"
            print ""
            next
        }
        { print }
    ' "${spec}" >"${spec}.tmp"
    mv "${spec}.tmp" "${spec}"
done

# fedora rc1 specs: rewrite Version: line only (these use %autochangelog)
for spec in packaging/fedora/coolercontrol-rc1.spec packaging/fedora/coolercontrold-rc1.spec; do
    sed -i -E "s/^(Version:[[:space:]]+)[0-9]+\.[0-9]+\.[0-9]+~rc1\$/\1${NEW_RC_VER}/" "${spec}"
done

# OBS .dsc: Version: line + version-stamped tarball references
dsc=packaging/obs/coolercontrol.dsc
sed -i -E "s/^Version: [0-9]+\.[0-9]+\.[0-9]+\$/Version: ${NEW_VER}/" "${dsc}"
sed -i -E "s|coolercontrold-vendor-[0-9]+\.[0-9]+\.[0-9]+\.tar\.gz|coolercontrold-vendor-${NEW_VER}.tar.gz|g" "${dsc}"
sed -i -E "s|coolercontrol-[0-9]+\.[0-9]+\.[0-9]+\.tar\.gz|coolercontrol-${NEW_VER}.tar.gz|g" "${dsc}"

# OBS include-binaries: vendor tarball name
sed -i -E "s|coolercontrold-vendor-[0-9]+\.[0-9]+\.[0-9]+\.tar\.gz|coolercontrold-vendor-${NEW_VER}.tar.gz|g" packaging/obs/debian/source/include-binaries

echo "New version successfully set: ${NEW_VER} (rc1 specs at ${NEW_RC_VER})"
