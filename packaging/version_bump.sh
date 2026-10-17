#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2021 Guy Boldon and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# VERSION bumping:
##################
# Valid version arguments are:
# a valid bump rule: patch, minor, major
# if nothing is explicitly specified with `make release` the default is set to patch
# Updates the source-of-truth versions (cargo, constants.h, package.json), then
# propagates the new version + current UTC date to every version-stamped
# packaging file: metainfo.xml, debian/changelog, fedora *.spec (regular and
# rc1). Substitutions match by regex rather than the previous version, so the
# script self-heals if files were out of sync.
# The OpenAPI spec stamps the daemon version too, so it is regenerated last.

set -euo pipefail

echo "Bumping version: $1"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${REPO_ROOT}"

# coolercontrold daemon (cargo bump is the source of truth for NEW_VER)
# Only the daemon: the other workspace crates are versioned by hand.
cd coolercontrold/daemon
cargo install cargo-edit
cargo install cargo-get
cargo set-version --offline --bump "$1" --package coolercontrold
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

# OpenAPI spec: carries the daemon version in info.version, and a daemon test fails when the
# checked-in file is stale. Regenerated last so a build failure here does not interrupt the
# cheap file substitutions above. Needs no running daemon and no root.
make -C "${REPO_ROOT}" openapi

echo "New version successfully set: ${NEW_VER} (rc1 specs at ${NEW_RC_VER})"
