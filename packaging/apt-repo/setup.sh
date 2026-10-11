#!/bin/sh
# SPDX-FileCopyrightText: 2026 Guy Boldon and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Configure apt to install CoolerControl from apt.coolercontrol.org.
#
#   curl -fsSL https://apt.coolercontrol.org/setup.sh | sudo sh
#
# Set TREE=debian or TREE=ubuntu to override the detected package tree, for derivatives that
# report an ID this script does not know. Note the -E, without it sudo drops the variable:
#
#   curl -fsSL https://apt.coolercontrol.org/setup.sh | sudo -E TREE=debian sh
#
# Safe to re-run: it rewrites its own source file and leaves everything else alone.
set -eu

REPO_URL=${REPO_URL:-https://apt.coolercontrol.org}
KEYRING=/usr/share/keyrings/coolercontrol-archive-keyring.gpg
SOURCES=/etc/apt/sources.list.d/coolercontrol.sources

log() { printf '==> %s\n' "$*"; }
die() {
    printf 'coolercontrol setup: %s\n' "$*" >&2
    exit 1
}

uid=$(id -u)
[ "${uid}" -eq 0 ] || die "must run as root, try piping to 'sudo sh' instead of 'sh'"

# Two package trees: /debian holds bookworm builds, /ubuntu holds jammy builds. Routing is by
# distribution only, never by architecture, and both trees serve amd64 and arm64.
pick_tree() {
    id=
    id_like=
    if [ -r /etc/os-release ]; then
        # shellcheck disable=SC1091
        . /etc/os-release
        id=${ID-}
        id_like=${ID_LIKE-}
    fi

    # Ubuntu derivatives first: Kali and Linux Mint LMDE both report ID_LIKE=debian, and only
    # LMDE belongs on the Debian side.
    case " ${id} " in
    " ubuntu " | " pop " | " elementary " | " neon " | " zorin " | " kali ")
        echo ubuntu
        return
        ;;
    " debian ")
        echo debian
        return
        ;;
    *) ;;
    esac

    case " ${id_like} " in
    *" ubuntu "*)
        echo ubuntu
        return
        ;;
    *" debian "*)
        echo debian
        return
        ;;
    *) ;;
    esac

    echo ubuntu
}

fetch() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$2" "$1"
    else
        die "neither curl nor wget is available"
    fi
}

# The old Cloudsmith entry has to stop being consulted or apt would keep hitting it. Match on
# content, not file name: users have hand written variants.
disable_cloudsmith_sources() {
    found=0
    for file in /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources; do
        [ -f "${file}" ] || continue
        grep -q 'dl\.cloudsmith\.io' "${file}" 2>/dev/null || continue
        grep -qi 'coolercontrol' "${file}" 2>/dev/null || continue
        mv "${file}" "${file}.disabled"
        log "disabled old Cloudsmith source ${file}"
        found=1
    done
    [ "${found}" -eq 0 ] || log "the old repository can be removed with: rm ${SOURCES%/*}/*.disabled"
}

case ${TREE-} in
"") tree=$(pick_tree) ;;
debian | ubuntu) tree=${TREE} ;;
*) die "TREE must be debian or ubuntu, got '${TREE}'" ;;
esac
log "using the ${tree} package tree at ${REPO_URL}/${tree}"

arch=$(dpkg --print-architecture 2>/dev/null) || die "dpkg is not available, is this a Debian based system?"

log "installing signing key to ${KEYRING}"
mkdir -p "$(dirname "${KEYRING}")"
fetch "${REPO_URL}/coolercontrol-archive-keyring.gpg" "${KEYRING}.new"
[ -s "${KEYRING}.new" ] || die "downloaded keyring is empty"
chmod 644 "${KEYRING}.new"
mv "${KEYRING}.new" "${KEYRING}"

log "writing ${SOURCES}"
mkdir -p "$(dirname "${SOURCES}")"
# The native architecture only. Without the field a multiarch host asks for every foreign
# architecture too, and apt prints a notice for each one the repository does not name.
cat >"${SOURCES}" <<EOF
Types: deb
URIs: ${REPO_URL}/${tree}
Suites: stable
Components: main
Architectures: ${arch}
Signed-By: ${KEYRING}
EOF

disable_cloudsmith_sources

log "updating package lists"
apt-get update

log "done, install with: apt-get install coolercontrol"
