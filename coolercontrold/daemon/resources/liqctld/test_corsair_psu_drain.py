# SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Tests that Corsair HID PSU commands read the report that answers them.

liquidctl writes a command and asserts that the very next report echoes it.
`initialize` knocks that out of step: it writes a wake-up command and blind-reads
one report, so a single stale report queued over a suspend makes that read take
the stale one and leaves the wake-up's reply in the stream, to be read as the
answer to the next command. The PSU then fails every init retry after a resume.

Draining first cannot fix it, because `clear_enqueued_reports` is non-blocking
and the offending reply is still in flight when it runs. The device below models
exactly that: a report that has landed, and one that has not yet.

No USB hardware, no liquidctl drivers and no HTTP layer are involved.
"""

import os
import sys
import unittest

from liquidctl.pmbus import CommandCode as CMD
from liquidctl.pmbus import WriteBit

# `main.py` lives next to this test; make it importable regardless of
# the caller's CWD or PYTHONPATH so the test runs from anywhere.
_THIS_DIR = os.path.dirname(os.path.abspath(__file__))
if _THIS_DIR not in sys.path:
    sys.path.insert(0, _THIS_DIR)

from main import (  # noqa: E402  (sys.path mutation above)
    _CORSAIR_MAX_RESYNC_READS,
    _CORSAIR_REPORT_LENGTH,
    _ORIGINAL_CORSAIR_PSU_EXEC,
    LiquidctlException,
    _exec_resyncing_reads,
)

# What the device sends back after the wake-up command `initialize` opens with. This is the report
# that ends up answering the wrong request.
_WAKE_REPLY = [0xFE, 0x03] + [0] * (_CORSAIR_REPORT_LENGTH - 2)


class FakeCorsairPsu:
    """A PSU with both a landed report queue and reports still in flight.

    Stands in for the driver and its `device` at once, since `_exec` reaches through `self._write`
    and `self._read` to the one and `self.device` to the other.
    """

    def __init__(self, landed=(), in_flight=()):
        self.landed = list(landed)
        self.in_flight = list(in_flight)

    @property
    def device(self):
        return self

    def clear_enqueued_reports(self):
        # Non-blocking, exactly like hidapi's: it discards what has already arrived and cannot
        # touch what is still in flight, which is the whole reason draining did not fix this.
        self.landed.clear()

    def write(self, packet):
        # The real device answers by echoing the command bytes back.
        self.landed.append(list(packet[1:3]) + [0] * (_CORSAIR_REPORT_LENGTH - 2))

    def read(self, length, timeout=None):
        # A report in flight arrives ahead of the reply to whatever was just written.
        if self.in_flight:
            return self.in_flight.pop(0)
        return self.landed.pop(0)

    def _write(self, data):
        # liquidctl prefixes a report number byte that this device does not use.
        self.write(bytes([0, *data]))

    def _read(self):
        return self.read(_CORSAIR_REPORT_LENGTH)


class CorsairPsuReportResyncTestCase(unittest.TestCase):
    def test_an_in_flight_reply_defeats_the_unpatched_driver(self):
        # Goal: reproduce the exact resume failure, and show why draining alone did not fix it.
        # The wake-up reply has not landed when the drain runs, so it arrives right after and is
        # read as the answer. Method: liquidctl's own `_exec` against an in-flight wake reply.
        psu = FakeCorsairPsu(in_flight=[_WAKE_REPLY])
        with self.assertRaises(AssertionError):
            _ORIGINAL_CORSAIR_PSU_EXEC(psu, WriteBit.WRITE, CMD.PAGE, [0])

    def test_an_in_flight_reply_is_skipped(self):
        # Goal: the fix. Matching the reply against the request steps over a report that answers
        # something else, whenever it happens to arrive. Method: the same in-flight wake reply.
        psu = FakeCorsairPsu(in_flight=[_WAKE_REPLY])
        reply = _exec_resyncing_reads(psu, WriteBit.WRITE, CMD.PAGE, [0])
        self.assertEqual(reply[1], int(CMD.PAGE))

    def test_a_landed_backlog_is_cleared_before_the_write(self):
        # Goal: a backlog that has already arrived is discarded cheaply rather than read through
        # one report at a time. Method: several stale reports, all landed.
        psu = FakeCorsairPsu(landed=[_WAKE_REPLY] * 6)
        reply = _exec_resyncing_reads(psu, WriteBit.WRITE, CMD.PAGE, [0])
        self.assertEqual(reply[1], int(CMD.PAGE))
        self.assertEqual(psu.landed, [])

    def test_an_in_step_stream_is_unaffected(self):
        # Goal: the ordinary case must behave exactly as before, since this runs on every command.
        # Method: nothing queued and nothing in flight.
        psu = FakeCorsairPsu()
        reply = _exec_resyncing_reads(psu, WriteBit.WRITE, CMD.PAGE, [0])
        self.assertEqual(reply[1], int(CMD.PAGE))

    def test_a_device_answering_nothing_recognisable_raises(self):
        # Goal: the negative space. Skipping reports must be bounded, or a genuinely broken PSU
        # would be read forever. Method: more junk in flight than the resync budget allows.
        psu = FakeCorsairPsu(in_flight=[_WAKE_REPLY] * (_CORSAIR_MAX_RESYNC_READS + 2))
        with self.assertRaises(LiquidctlException):
            _exec_resyncing_reads(psu, WriteBit.WRITE, CMD.PAGE, [0])


if __name__ == "__main__":
    unittest.main()
