# SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Tests that a failed device scan is reported as a failure, not as an empty bus.

`scan_devices` used to swallow every exception and return `[]`. The daemon
compares that list against its baseline to detect hotplug changes, so an
enumeration that raised looked exactly like every device having been unplugged,
and fired a "Device Removed, restart the daemon" desktop notification for
hardware that was still connected.

These exercise `DeviceService.scan_devices` against a stubbed
`liquidctl.find_liquidctl_devices`; no USB hardware or HTTP layer is involved.
"""

import os
import sys
import unittest
from unittest import mock

# `main.py` lives next to this test; make it importable regardless of
# the caller's CWD or PYTHONPATH so the test runs from anywhere.
_THIS_DIR = os.path.dirname(os.path.abspath(__file__))
if _THIS_DIR not in sys.path:
    sys.path.insert(0, _THIS_DIR)

from main import DeviceService, LiquidctlException  # noqa: E402


class ScanDevicesFailureTestCase(unittest.TestCase):
    def setUp(self):
        self.device_service = DeviceService()

    def test_an_empty_bus_is_still_an_empty_list(self):
        # Goal: the positive space. Finding nothing is a legitimate answer and must not raise,
        # or the daemon could never detect the last device being unplugged.
        # Method: enumeration succeeds and yields nothing.
        with mock.patch("main.liquidctl.find_liquidctl_devices", return_value=iter([])):
            self.assertEqual(self.device_service.scan_devices(), [])

    def test_a_raising_enumeration_is_reported_as_a_failure(self):
        # Goal: a scan that blows up must be distinguishable from an empty bus.
        # Method: the generic failure path, which liquidctl can hit walking a device whose
        # descriptor read times out.
        with mock.patch(
            "main.liquidctl.find_liquidctl_devices",
            side_effect=OSError("[Errno 110] Operation timed out"),
        ):
            with self.assertRaises(LiquidctlException):
                self.device_service.scan_devices()

    def test_a_value_error_is_reported_as_a_failure(self):
        # Goal: the same for ValueError, which liquidctl raises on its own for some devices and
        # which was previously swallowed even more quietly, at debug level.
        # Method: the ValueError path.
        with mock.patch(
            "main.liquidctl.find_liquidctl_devices",
            side_effect=ValueError("no runtime information available"),
        ):
            with self.assertRaises(LiquidctlException):
                self.device_service.scan_devices()


if __name__ == "__main__":
    unittest.main()
