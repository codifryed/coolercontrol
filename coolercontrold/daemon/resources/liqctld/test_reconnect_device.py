# SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Tests for reconnecting a single device's USB handle.

This is the lighter half of recovery. Restarting the service reopens every
device to reach one and costs seconds; what a restart does to a device is
release its interface and open it again, which can be done to one device in
milliseconds. The driver object is kept, which is what lets a legacy690 flip and
a forced direct access survive it.

These drive stub drivers through `DeviceService.reconnect_device`; no USB
hardware, no liquidctl drivers and no HTTP layer are involved.
"""

import os
import sys
import unittest

# `main.py` lives next to this test; make it importable regardless of
# the caller's CWD or PYTHONPATH so the test runs from anywhere.
_THIS_DIR = os.path.dirname(os.path.abspath(__file__))
if _THIS_DIR not in sys.path:
    sys.path.insert(0, _THIS_DIR)

from main import (  # noqa: E402  (sys.path mutation above)
    DeviceService,
    LiqctldException,
    LiquidctlException,
)

DEVICE_ID = 1


class RecordingDriver:
    """Records the order of the calls a reconnect makes against it."""

    def __init__(self, fail_on=None):
        self.calls = []
        self.fail_on = fail_on

    def _record(self, name):
        self.calls.append(name)
        if self.fail_on == name:
            raise OSError("device is not answering")

    def disconnect(self):
        self._record("disconnect")

    def connect(self):
        self._record("connect")


class ImmediateJob:
    """Stands in for a submitted job; the stub driver's work is synchronous."""

    def __init__(self, fn):
        self.fn = fn

    def result(self, timeout=None):
        return self.fn()


class ImmediateExecutor:
    def submit(self, device_id, fn, **kwargs):
        return ImmediateJob(lambda: fn(**kwargs))


def service_with(driver) -> DeviceService:
    device_service = DeviceService()
    device_service.devices[DEVICE_ID] = driver
    device_service.device_executor = ImmediateExecutor()
    return device_service


class ReconnectDeviceTestCase(unittest.TestCase):
    def test_the_handle_is_released_before_it_is_reopened(self):
        # Goal: the order is the whole point. Opening before releasing would claim an interface
        # that is still held, so a reconnect has to close first.
        # Method: a driver that records both calls.
        driver = RecordingDriver()
        service_with(driver).reconnect_device(DEVICE_ID)
        self.assertEqual(driver.calls, ["disconnect", "connect"])

    def test_the_driver_object_is_kept(self):
        # Goal: keeping the object is what lets a legacy690 flip and a forced direct access
        # survive, since both live on it. Method: compare identity across the reconnect.
        driver = RecordingDriver()
        device_service = service_with(driver)
        device_service.reconnect_device(DEVICE_ID)
        self.assertIs(device_service.devices[DEVICE_ID], driver)

    def test_a_device_that_will_not_reopen_reports_a_failure(self):
        # Goal: the caller has to learn that the light touch did not work, so it can go on to
        # restarting the service. Method: a driver that fails to reopen.
        driver = RecordingDriver(fail_on="connect")
        with self.assertRaises(LiquidctlException):
            service_with(driver).reconnect_device(DEVICE_ID)
        self.assertEqual(driver.calls, ["disconnect", "connect"])

    def test_an_unknown_device_is_not_found(self):
        # Goal: the negative space. A request for a device we do not have is a client error, not a
        # communication failure, and the two are answered with different statuses.
        # Method: an id that was never registered.
        with self.assertRaises(LiqctldException):
            service_with(RecordingDriver()).reconnect_device(DEVICE_ID + 99)


if __name__ == "__main__":
    unittest.main()
