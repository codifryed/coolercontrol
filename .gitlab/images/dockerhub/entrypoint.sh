#!/bin/sh
set -e

# Initialize the OpenRC runtime so rc-service can manage plugin services.
# rc_sys="docker" (set in /etc/rc.conf) makes OpenRC skip hardware/fs operations.
mkdir -p /run/openrc
openrc default 2>/dev/null || true

exec /usr/local/bin/coolercontrold "$@"
