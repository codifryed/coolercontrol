#!/bin/sh
# SPDX-FileCopyrightText: 2026 Guy Boldon and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

set -e

# Initialize the OpenRC runtime so rc-service can manage plugin services.
# rc_sys="docker" (set in /etc/rc.conf) makes OpenRC skip hardware/fs operations.
mkdir -p /run/openrc
openrc default 2>/dev/null || true

# D-Bus needs the host's system bus socket mounted, so follow the mount unless CC_DBUS is set.
# A set DBUS_SYSTEM_BUS_ADDRESS names the bus elsewhere, so the daemon default applies.
if [ -z "${CC_DBUS+x}" ] && [ -z "${DBUS_SYSTEM_BUS_ADDRESS:-}" ]; then
    if [ -S /run/dbus/system_bus_socket ]; then
        export CC_DBUS=ON
    else
        export CC_DBUS=OFF
    fi
fi

exec /usr/local/bin/coolercontrold "$@"
