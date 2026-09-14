# SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Tests that Corsair HID PSU commands drain queued reports before writing.

The PSU sends reports unasked. One that arrives while nothing is reading becomes
the answer to the next command, and every read after it is one behind, which
liquidctl reports as "invalid response (possible conflict with another program)".
A resume is when a backlog is waiting, and `initialize` is the first thing called
after one, so the PSU would exhaust every init retry and stay unusable.

These drive a stand-in device that models the report queue; no USB hardware and no
HTTP layer are involved.
"""

import os
import sys
import unittest

from liquidctl.driver.corsair_hid_psu import CMD, WriteBit

# `main.py` lives next to this test; make it importable regardless of
# the caller's CWD or PYTHONPATH so the test runs from anywhere.
_THIS_DIR = os.path.dirname(os.path.abspath(__file__))
if _THIS_DIR not in sys.path:
    sys.path.insert(0, _THIS_DIR)

from main import (  # noqa: E402  (sys.path mutation above)
    _ORIGINAL_CORSAIR_PSU_EXEC,
    _exec_with_drained_queue,
)

_REPORT_LENGTH = 64
_STALE_REPORT = [0xDE, 0xAD] + [0] * (_REPORT_LENGTH - 2)


class FakeCorsairPsu:
    """A PSU whose report queue can already hold something when a command goes out.

    Stands in for both the driver and its `device`, since `_exec` reaches through
    `self._write` / `self._read` to the one and `self.device` to the other.
    """

    def __init__(self, queued_reports):
        self.queue = list(queued_reports)
        self.device = self

    def clear_enqueued_reports(self):
        self.queue.clear()

    def write(self, packet):
        # The real device answers by echoing the command bytes back.
        self.queue.append(list(packet[1:3]) + [0] * (_REPORT_LENGTH - 2))

    def read(self, length):
        return self.queue.pop(0)

    def _write(self, data):
        packet = bytearray(1 + _REPORT_LENGTH)
        packet[1 : 1 + len(data)] = data
        self.write(packet)

    def _read(self):
        return self.read(_REPORT_LENGTH)


class CorsairPsuReportDrainTestCase(unittest.TestCase):
    def test_a_queued_report_desyncs_the_unpatched_driver(self):
        # Goal: prove the patch is load-bearing rather than decorative, by showing the exact
        # failure it prevents. Method: liquidctl's own `_exec` against one stale queued report.
        psu = FakeCorsairPsu([_STALE_REPORT])
        with self.assertRaises(AssertionError):
            _ORIGINAL_CORSAIR_PSU_EXEC(psu, WriteBit.WRITE, CMD.PAGE, [0])

    def test_the_drain_discards_a_stale_report_first(self):
        # Goal: the fix. A report queued before the command cannot be its answer, so dropping it
        # leaves the real reply to be read. Method: the same stale report, through the wrapper.
        psu = FakeCorsairPsu([_STALE_REPORT])
        reply = _exec_with_drained_queue(psu, WriteBit.WRITE, CMD.PAGE, [0])
        self.assertEqual(reply[1], int(CMD.PAGE))

    def test_a_backlog_of_reports_is_cleared_in_one_go(self):
        # Goal: a resume leaves more than one report waiting, which is why liquidctl's single
        # discard read in `initialize` is not enough. Method: several stale reports.
        psu = FakeCorsairPsu([_STALE_REPORT] * 5)
        reply = _exec_with_drained_queue(psu, WriteBit.WRITE, CMD.PAGE, [0])
        self.assertEqual(reply[1], int(CMD.PAGE))
        self.assertEqual(psu.queue, [])

    def test_an_empty_queue_is_unaffected(self):
        # Goal: the ordinary case must behave exactly as before, since this runs on every command.
        # Method: no stale reports at all.
        psu = FakeCorsairPsu([])
        reply = _exec_with_drained_queue(psu, WriteBit.WRITE, CMD.PAGE, [0])
        self.assertEqual(reply[1], int(CMD.PAGE))


if __name__ == "__main__":
    unittest.main()
