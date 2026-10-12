# SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Tests that a Corsair Commander ST is written reports of its own length.

liquidctl's CommanderCore driver serves three models and pads every write to the
Commander Core's 96 bytes. The Commander ST's reports are 64 bytes, so each
command reached it with 32 surplus bytes, and its firmware sporadically answered
with an `f0` report and then dropped a reply.

These drive liquidctl's own driver against a recording HID device; no USB
hardware and no HTTP layer are involved.
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

import main  # noqa: E402  (sys.path mutation above)
from main import (  # noqa: E402
    _COMMANDER_ST_PRODUCT_ID,
    _COMMANDER_ST_REPORT_LENGTH,
    _ORIGINAL_COMMANDER_CORE_CONNECT,
    CommanderCore,
    _connect_sizing_st_reports,
)

_COMMANDER_CORE_PRODUCT_ID = 0x0C1C
_COMMANDER_CORE_XT_PRODUCT_ID = 0x0C2A

_CMD_WRITE = (0x06, 0x00)
# The fixed duty table an ST was sent in the capture this was diagnosed from: seven channels,
# pump at 66% and the fans at 34, 34, 32, 32, 32 and 0.
_DUTY_TABLE = bytes.fromhex("110000000400" "07" "4200220022002000200020000000")
# What a fan curve sized for a 96 byte report can put in one write.
_DATA_PAST_AN_ST_REPORT = bytes(range(1, 81))


class RecordingHidDevice:
    """Records each report written and answers it the way the hardware does."""

    def __init__(self, product_id):
        self.product_id = product_id
        self.path = (
            b"not-a-hidraw-node"  # liquidctl looks for a kernel driver behind it
        )
        self.written = []
        self._replies = []

    def open(self):
        pass

    def clear_enqueued_reports(self):
        pass

    def write(self, data):
        self.written.append(bytes(data))
        reply = bytearray(_COMMANDER_ST_REPORT_LENGTH)
        reply[1] = data[2]  # the device echoes the command byte
        self._replies.append(reply)
        return len(data)

    def read(self, length, timeout=None):
        return self._replies.pop(0)


def _written_by(connect, product_id, data):
    """Returns the reports liquidctl's driver writes for one command after `connect`."""
    driver = CommanderCore(RecordingHidDevice(product_id), "a commander", True)
    connect(driver)
    driver._send_command(_CMD_WRITE, data)
    return driver.device.written


@unittest.skipIf(CommanderCore is None, "this liquidctl has no CommanderCore driver")
class CommanderStReportLengthTestCase(unittest.TestCase):
    def test_the_unpatched_driver_overfills_an_st_report(self):
        # Goal: reproduce the defect, so a liquidctl release that sizes the ST itself is noticed.
        # Method: liquidctl's own connect and write against an ST.
        (report,) = _written_by(
            _ORIGINAL_COMMANDER_CORE_CONNECT, _COMMANDER_ST_PRODUCT_ID, _DUTY_TABLE
        )
        self.assertGreater(len(report) - 1, _COMMANDER_ST_REPORT_LENGTH)

    def test_an_st_report_is_written_at_its_own_length(self):
        # Goal: the fix. The ST gets one full report, with the command and its data intact.
        # Method: the duty table write from the capture, compared byte for byte.
        (report,) = _written_by(
            _connect_sizing_st_reports, _COMMANDER_ST_PRODUCT_ID, _DUTY_TABLE
        )
        expected = bytes([0x00, 0x08, *_CMD_WRITE]) + _DUTY_TABLE
        expected += bytes(_COMMANDER_ST_REPORT_LENGTH + 1 - len(expected))
        self.assertEqual(report, expected)

    def test_the_other_models_keep_liquidctls_writes(self):
        # Goal: the patch must not reach the models whose reports liquidctl sizes correctly.
        # Method: the same command through the patched and the stock connect, per model.
        for product_id in (_COMMANDER_CORE_PRODUCT_ID, _COMMANDER_CORE_XT_PRODUCT_ID):
            with self.subTest(product_id=hex(product_id)):
                patched = _written_by(
                    _connect_sizing_st_reports, product_id, _DUTY_TABLE
                )
                stock = _written_by(
                    _ORIGINAL_COMMANDER_CORE_CONNECT, product_id, _DUTY_TABLE
                )
                self.assertEqual(patched, stock)

    def test_data_past_an_st_report_is_not_truncated(self):
        # Goal: the negative space. Only padding may be dropped, never data.
        # Method: a write whose data runs past 64 bytes must match the stock driver's.
        patched = _written_by(
            _connect_sizing_st_reports,
            _COMMANDER_ST_PRODUCT_ID,
            _DATA_PAST_AN_ST_REPORT,
        )
        stock = _written_by(
            _ORIGINAL_COMMANDER_CORE_CONNECT,
            _COMMANDER_ST_PRODUCT_ID,
            _DATA_PAST_AN_ST_REPORT,
        )
        self.assertEqual(patched, stock)


class CommanderStPatchInstallTestCase(unittest.TestCase):
    def test_the_patch_is_installed_on_connect(self):
        # Goal: the wrapper only takes effect if the driver's connect is replaced by it.
        # Method: install onto a stand-in class, so the real driver stays as liquidctl made it.
        stub = type("CommanderCoreStub", (), {})
        with mock.patch.object(main, "CommanderCore", stub):
            installed = main.patch_commander_st_report_length()
        self.assertTrue(installed)
        self.assertIs(stub.connect, _connect_sizing_st_reports)

    def test_a_liquidctl_without_the_driver_is_left_alone(self):
        # Goal: an old liquidctl that lacks the driver must still start.
        # Method: install with the driver import having failed.
        with mock.patch.object(main, "CommanderCore", None):
            installed = main.patch_commander_st_report_length()
        self.assertFalse(installed)


if __name__ == "__main__":
    unittest.main()
