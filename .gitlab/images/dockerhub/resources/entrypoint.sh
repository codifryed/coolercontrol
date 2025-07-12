#!/bin/sh
exec coolercontrol-liqctld "$@" &
exec coolercontrold "$@" &
wait
