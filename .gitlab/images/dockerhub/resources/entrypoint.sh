#!/bin/sh
exec python3 -m coolercontrol_liqctld "$@" &
exec /usr/local/bin/coolercontrold "$@" &
wait
