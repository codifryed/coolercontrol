#!/bin/bash

###############################################################################
#                                                                             #
#   Setup                                                                     #
#                                                                             #
###############################################################################

set -euo pipefail

readonly TRUNK_LAUNCHER_VERSION="1.2.7" # warning: this line is auto-updated

readonly SUCCESS_MARK="\033[0;32m✔\033[0m"
readonly FAIL_MARK="\033[0;31m✘\033[0m"
readonly PROGRESS_MARKS=("⡿" "⢿" "⣻" "⣽" "⣾" "⣷" "⣯" "⣟")

# This is how mktemp(1) decides where to create stuff in tmpfs.
readonly TMPDIR="${TMPDIR:-/tmp}"

KERNEL=$(uname | tr "[:upper:]" "[:lower:]")
if [[ ${KERNEL} == mingw64* || ${KERNEL} == msys* ]]; then
  KERNEL="mingw"
fi
readonly KERNEL

MACHINE=$(uname -m)
if [[ $MACHINE == "aarch64" ]]; then
  MACHINE="arm64";
fi

readonly MACHINE

PLATFORM="${KERNEL}-${MACHINE}"
readonly PLATFORM

PLATFORM_UNDERSCORE="${KERNEL}_${MACHINE}"
readonly PLATFORM_UNDERSCORE

# https://en.wikipedia.org/wiki/ANSI_escape_code#CSI_(Control_Sequence_Introducer)_sequences
# [nF is "cursor previous line" and moves to the beginning of the nth previous line
# [0K is "erase display" and clears from the cursor to the end of the screen
readonly CLEAR_LAST_MSG="\033[1F\033[0K"

if [[ ! -z ${CI:-} && "${CI}" = true && -z ${TRUNK_LAUNCHER_QUIET:-} ]]; then
  TRUNK_LAUNCHER_QUIET=1
else
  TRUNK_LAUNCHER_QUIET=${TRUNK_LAUNCHER_QUIET:-${TRUNK_QUIET:-false}}
fi

readonly TRUNK_LAUNCHER_DEBUG

if [[ ${TRUNK_LAUNCHER_QUIET} != false ]]; then
  exec 3>&1 4>&2 &>/dev/null
fi

TRUNK_CACHE="${TRUNK_CACHE:-}"
if [[ -n ${TRUNK_CACHE} ]]; then
  :
elif [[ -n ${XDG_CACHE_HOME:-} ]]; then
  TRUNK_CACHE="${XDG_CACHE_HOME}/trunk"
else
  TRUNK_CACHE="${HOME}/.cache/trunk"
fi
readonly TRUNK_CACHE
readonly CLI_DIR="${TRUNK_CACHE}/cli"
mkdir -p "${CLI_DIR}"

# platform check
readonly MINIMUM_MACOS_VERSION="10.15"
check_darwin_version() {
  local osx_version
  osx_version="$(sw_vers -productVersion)"

  # trunk-ignore-begin(shellcheck/SC2312): the == will fail if anything inside the $() fails
  if [[ "$(printf "%s\n%s\n" "${MINIMUM_MACOS_VERSION}" "${osx_version}" |
    sort --version-sort |
    head -n 1)" == "${MINIMUM_MACOS_VERSION}"* ]]; then
    return
  fi
  # trunk-ignore-end(shellcheck/SC2312)

  echo -e "${FAIL_MARK} Trunk requires at least MacOS ${MINIMUM_MACOS_VERSION}" \
    "(yours is ${osx_version}). See https://docs.trunk.io for more info."
  exit 1
}

if [[ ${PLATFORM} == "darwin-x86_64" || ${PLATFORM} == "darwin-arm64" ]]; then
  check_darwin_version
elif [[ ${PLATFORM} == "linux-x86_64" || ${PLATFORM} == "linux-arm64" || ${PLATFORM} == "windows-x86_64" || ${PLATFORM} == "mingw-x86_64" ]]; then
  :
else
  echo -e "${FAIL_MARK} Trunk is only supported on Linux (x64_64, arm64), MacOS (x86_64, arm64), and Windows (x86_64)." \
    "See https://docs.trunk.io for more info."
  exit 1
fi

TRUNK_TMPDIR="${TMPDIR}/trunk-$(
  set -e
  id -u
)/launcher_logs"
readonly TRUNK_TMPDIR
mkdir -p "${TRUNK_TMPDIR}"

# For the `mv $TOOL_TMPDIR/trunk $TOOL_DIR` to be atomic (i.e. just inode renames), the source and destination filesystems need to be the same
TOOL_TMPDIR=$(mktemp -d "${CLI_DIR}/tmp.XXXXXXXXXX")
readonly TOOL_TMPDIR

cleanup() {
  rm -rf "${TOOL_TMPDIR}"
  if [[ $1 == "0" ]]; then
    rm -rf "${TRUNK_TMPDIR}"
  fi
}
trap 'cleanup $?' EXIT

# e.g. 2022-02-16-20-40-31-0800
dt_str() { date +"%Y-%m-%d-%H-%M-%S%z"; }

LAUNCHER_TMPDIR="${TOOL_TMPDIR}/launcher"
readonly LAUNCHER_TMPDIR
mkdir -p "${LAUNCHER_TMPDIR}"

if [[ -n ${TRUNK_LAUNCHER_DEBUG:-} ]]; then
  set -x
fi

# launcher awk
#
# BEGIN{ORS="";}
#   use "" as the output record separator
#   ORS defaults to "\n" for bwk, which results in
#     $(printf "foo bar" | awk '{print $2}') == "bar\n"
#
# {gsub(/\r/, "", $0)}
#   for every input record (i.e. line), the regex "\r" should be replaced with ""
#   This is necessary to handle CRLF files in a portable fashion.
#
# Some StackOverflow answers suggest using RS="\r?\n" to handle CRLF files (RS is the record
# separator, i.e. the line delimiter); unfortunately, original-awk only allows single-character
# values for RS (see https://www.gnu.org/software/gawk/manual/gawk.html#awk-split-records).
lawk() {
  awk 'BEGIN{ORS="";}{gsub(/\r/, "", $0)}'"${1}" "${@:2}"
}
awk_test() {
  # trunk-ignore-begin(shellcheck/SC2310,shellcheck/SC2312)
  # SC2310 and SC2312 are about set -e not propagating to the $(); if that happens, the string
  # comparison will fail and we'll claim the user's awk doesn't work
  if [[ $(
    set -e
    printf 'k1: v1\n \tk2: v2\r\n' | lawk '/[ \t]+k2:/{print $2}'
  ) == 'v2' &&
  $(
    set -e
    printf 'k1: v1\r\n\t k2: v2\r\n' | lawk '/[ \t]+k2:/{print $2}'
  ) == 'v2' ]]; then
    return
  fi
  # trunk-ignore-end(shellcheck/SC2310,shellcheck/SC2312)

  echo -e "${FAIL_MARK} Trunk does not work with your awk;" \
    "please report this at https://slack.trunk.io."
  echo -e "Your version of awk is:"
  awk --version || awk -Wversion
  exit 1
}
awk_test

readonly CURL_FLAGS="${CURL_FLAGS:- -vvv --max-time 120 --retry 3 --fail}"
readonly WGET_FLAGS="${WGET_FLAGS:- --verbose --tries=3 --limit-rate=10M}"
TMP_DOWNLOAD_LOG="${TRUNK_TMPDIR}/download-$(
  set -e
  dt_str
).log"
readonly TMP_DOWNLOAD_LOG

# Detect whether we should use wget or curl.
if command -v wget &>/dev/null; then
  download_cmd() {
    local url="${1}"
    local output_to="${2}"
    # trunk-ignore-begin(shellcheck/SC2312): we don't care if wget --version errors
    cat >>"${TMP_DOWNLOAD_LOG}" <<EOF
Using wget to download '${url}' to '${output_to}'

Is Trunk up?: https://status.trunk.io

WGET_FLAGS: ${WGET_FLAGS}

wget --version:
$(wget --version 2>&1)

EOF
    # trunk-ignore-end(shellcheck/SC2312)

    # Support BusyBox wget
    if wget --help 2>&1 | grep BusyBox; then
      wget "${url}" -O "${output_to}" 2>>"${TMP_DOWNLOAD_LOG}" &
    else
      # trunk-ignore(shellcheck/SC2086): we deliberately don't quote WGET_FLAGS
      wget ${WGET_FLAGS} "${url}" --output-document "${output_to}" 2>>"${TMP_DOWNLOAD_LOG}" &
    fi
  }
elif command -v curl &>/dev/null; then
  download_cmd() {
    local url="${1}"
    local output_to="${2}"
    # trunk-ignore-begin(shellcheck/SC2312): we don't care if curl --version errors
    cat >>"${TMP_DOWNLOAD_LOG}" <<EOF
Using curl to download '${url}' to '${output_to}'

Is Trunk up?: https://status.trunk.io

CURL_FLAGS: ${CURL_FLAGS}

curl --version:
$(curl --version)

EOF
    # trunk-ignore-end(shellcheck/SC2312)

    # trunk-ignore(shellcheck/SC2086): we deliberately don't quote CURL_FLAGS
    curl ${CURL_FLAGS} "${url}" --output "${output_to}" 2>>"${TMP_DOWNLOAD_LOG}" &
  }
else
  download_cmd() {
    echo -e "${FAIL_MARK} Cannot download '${url}'; please install curl or wget."
    exit 1
  }
fi

download_url() {
  local url="${1}"
  local output_to="${2}"
  local progress_message="${3:-}"

  if [[ -n ${progress_message} ]]; then
    echo -e "${PROGRESS_MARKS[0]} ${progress_message}..."
  fi

  download_cmd "${url}" "${output_to}"
  local download_pid="$!"

  local i_prog=0
  while [[ -d "/proc/${download_pid}" && -n ${progress_message} ]]; do
    echo -e "${CLEAR_LAST_MSG}${PROGRESS_MARKS[${i_prog}]} ${progress_message}..."
    sleep 0.2
    i_prog=$(((i_prog + 1) % ${#PROGRESS_MARKS[@]}))
  done

  local download_log
  if ! wait "${download_pid}"; then
    download_log="${TRUNK_TMPDIR}/launcher-download-$(
      set -e
      dt_str
    ).log"
    mv "${TMP_DOWNLOAD_LOG}" "${download_log}"
    echo -e "${CLEAR_LAST_MSG}${FAIL_MARK} ${progress_message}... FAILED (see ${download_log})"
    echo -e "Please check your connection and try again." \
      "If you continue to see this error message," \
      "consider reporting it to us at https://slack.trunk.io."
    exit 1
  fi

  if [[ -n ${progress_message} ]]; then
    echo -e "${CLEAR_LAST_MSG}${SUCCESS_MARK} ${progress_message}... done"
  fi

}

# sha256sum is in coreutils, so we prefer that over shasum, which is installed with perl
if command -v sha256sum &>/dev/null; then
  :
elif command -v shasum &>/dev/null; then
  sha256sum() { shasum -a 256 "$@"; }
else
  sha256sum() {
    echo -e "${FAIL_MARK} Cannot compute sha256; please install sha256sum or shasum"
    exit 1
  }
fi

###############################################################################
#                                                                             #
#   CLI resolution functions                                                  #
#                                                                             #
###############################################################################

trunk_yaml_abspath() {
  local repo_head
  local cwd

  if repo_head=$(git rev-parse --show-toplevel 2>/dev/null); then
    echo "${repo_head}/.trunk/trunk.yaml"
  elif [[ -f .trunk/trunk.yaml ]]; then
    cwd="$(pwd)"
    echo "${cwd}/.trunk/trunk.yaml"
  else
    echo ""
  fi
}

read_cli_version_from() {
  local config_abspath="${1}"
  local cli_version

  cli_version="$(
    set -e
    lawk '/[ \t]+version:/{print $2; exit;}' "${config_abspath}"
  )"
  if [[ -z ${cli_version} ]]; then
    echo -e "${FAIL_MARK} Invalid .trunk/trunk.yaml, no cli version found." \
      "See https://docs.trunk.io for more info." >&2
    exit 1
  fi

  echo "${cli_version}"
}

download_cli() {
  local dl_version="${1}"
  local expected_sha256="${2}"
  local actual_sha256

  readonly TMP_INSTALL_DIR="${LAUNCHER_TMPDIR}/install"
  mkdir -p "${TMP_INSTALL_DIR}"

  TRUNK_NEW_URL_VERSION=0.10.2-beta.1
  if sort --help 2>&1 | grep BusyBox; then
    readonly URL="https://trunk.io/releases/${dl_version}/trunk-${dl_version}-${PLATFORM}.tar.gz"
  else
    if [[ "$(printf "%s\n%s\n" "${TRUNK_NEW_URL_VERSION}" "${dl_version}" |
      sort --version-sort |
      head -n 1 || true)" == "${TRUNK_NEW_URL_VERSION}"* ]]; then
      readonly URL="https://trunk.io/releases/${dl_version}/trunk-${dl_version}-${PLATFORM}.tar.gz"
    else
      readonly URL="https://trunk.io/releases/trunk-${dl_version}.${KERNEL}.tar.gz"
    fi
  fi

  readonly DOWNLOAD_TAR_GZ="${TMP_INSTALL_DIR}/download-${dl_version}.tar.gz"

  download_url "${URL}" "${DOWNLOAD_TAR_GZ}" "Downloading Trunk ${dl_version}"

  if [[ -n ${expected_sha256:-} ]]; then
    local verifying_text="Verifying Trunk sha256..."
    echo -e "${PROGRESS_MARKS[0]} ${verifying_text}"

    actual_sha256="$(
      set -e
      sha256sum "${DOWNLOAD_TAR_GZ}" | lawk '{print $1}'
    )"

    if [[ ${actual_sha256} != "${expected_sha256}" ]]; then
      echo -e "${CLEAR_LAST_MSG}${FAIL_MARK} ${verifying_text} FAILED"
      echo "Expected sha256: ${expected_sha256}"
      echo "  Actual sha256: ${actual_sha256}"
      exit 1
    fi

    echo -e "${CLEAR_LAST_MSG}${SUCCESS_MARK} ${verifying_text} done"
  fi

  local unpacking_text="Unpacking Trunk..."
  echo -e "${PROGRESS_MARKS[0]} ${unpacking_text}"
  tar --strip-components=1 -C "${TMP_INSTALL_DIR}" -xf "${DOWNLOAD_TAR_GZ}"
  echo -e "${CLEAR_LAST_MSG}${SUCCESS_MARK} ${unpacking_text} done"

  rm -f "${DOWNLOAD_TAR_GZ}"
  mkdir -p "${TOOL_DIR}"
  readonly OLD_TOOL_DIR="${CLI_DIR}/${version}"
  # Create a backwards compatability link for old versions of trunk that want to write their
  # crashpad_handlers to that dir.
  if [[ ! -e ${OLD_TOOL_DIR} ]]; then
    ln -sf "${TOOL_PART}" "${OLD_TOOL_DIR}"
  fi
  mv -n "${TMP_INSTALL_DIR}/trunk" "${TOOL_DIR}/" || true
  rm -rf "${TMP_INSTALL_DIR}"
}

###############################################################################
#                                                                             #
#   CLI resolution                                                            #
#                                                                             #
###############################################################################

CONFIG_ABSPATH="$(
  set -e
  trunk_yaml_abspath
)"
readonly CONFIG_ABSPATH

version="${TRUNK_CLI_VERSION:-}"
if [[ -n ${version:-} ]]; then
  :
elif [[ -f ${CONFIG_ABSPATH} ]]; then
  version="$(
    set -e
    read_cli_version_from "${CONFIG_ABSPATH}"
  )"
  version_sha256="$(
    set -e
    lawk "/${PLATFORM_UNDERSCORE}:/"'{print $2}' "${CONFIG_ABSPATH}"
  )"
else
  readonly LATEST_FILE="${LAUNCHER_TMPDIR}/latest"
  download_url "https://trunk.io/releases/latest" "${LATEST_FILE}"
  version=$(
    set -e
    lawk '/version:/{print $2}' "${LATEST_FILE}"
  )
  version_sha256=$(
    set -e
    lawk "/${PLATFORM_UNDERSCORE}:/"'{print $2}' "${LATEST_FILE}"
  )
fi

readonly TOOL_PART="${version}-${PLATFORM}"
readonly TOOL_DIR="${CLI_DIR}/${TOOL_PART}"

if [[ ! -e ${TOOL_DIR}/trunk ]]; then
  download_cli "${version}" "${version_sha256:-}"
  echo # add newline between launcher and CLI output
fi

if [[ ${TRUNK_LAUNCHER_QUIET} != false ]]; then
  exec 1>&3 3>&- 2>&4 4>&-
fi

###############################################################################
#                                                                             #
#   CLI invocation                                                            #
#                                                                             #
###############################################################################

if [[ -n ${LATEST_FILE:-} ]]; then
  mv -n "${LATEST_FILE}" "${TOOL_DIR}/version" >/dev/null 2>&1 || true
fi

# NOTE: exec will overwrite the process image, so trap will not catch the exit signal.
# Therefore, run cleanup manually here.
cleanup 0

exec \
  env TRUNK_LAUNCHER_VERSION="${TRUNK_LAUNCHER_VERSION}" \
  env TRUNK_LAUNCHER_PATH="${BASH_SOURCE[0]}" \
  "${TOOL_DIR}/trunk" "$@"
