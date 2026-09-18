# SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Tests for the ceiling on serving a cached device status.

A device whose reads blow the short future timeout gets its last known status
served while a background refresh runs behind it. That is right for a slow
device and wrong for a dead one: the cache freezes at its last good value, the
daemon never sees a missing status, its failsafe never latches, and a fan curve
keeps running off a temperature from hours ago.

These exercise `_cached_status_is_servable` and the serving path in
`_get_current_or_cached_device_status` against stubs; no USB hardware, no
liquidctl drivers and no HTTP layer are involved.
"""

import concurrent.futures
import os
import sys
import time
import unittest

# `main.py` lives next to this test; make it importable regardless of
# the caller's CWD or PYTHONPATH so the test runs from anywhere.
_THIS_DIR = os.path.dirname(os.path.abspath(__file__))
if _THIS_DIR not in sys.path:
    sys.path.insert(0, _THIS_DIR)

from main import (  # noqa: E402  (sys.path mutation above)
    STALE_STATUS_BUDGET_SECS,
    DeviceService,
)

DEVICE_ID = 1
CACHED = [("temperature", "30.0", "°C")]


class FakeDevice:
    """Stands in for a liquidctl driver; only its class name is ever read here."""

    def get_status(self):
        raise AssertionError("the stub executor never runs the job")


class FakeJob:
    """A submitted job whose future always times out, as a dead device's read does."""

    def result(self, timeout=None):
        raise concurrent.futures.TimeoutError()

    def cancel(self):
        return False


class FakeExecutor:
    """Records what was submitted and always hands back a timing-out job."""

    def __init__(self, queue_empty: bool) -> None:
        self.queue_empty = queue_empty
        self.submitted = []

    def submit(self, device_id, fn, **kwargs):
        self.submitted.append(getattr(fn, "__name__", str(fn)))
        return FakeJob()

    def device_queue_empty(self, device_id) -> bool:
        return self.queue_empty


def service_with_cache(queue_empty: bool = True) -> DeviceService:
    device_service = DeviceService()
    device_service.devices[DEVICE_ID] = FakeDevice()
    device_service.device_executor = FakeExecutor(queue_empty)
    device_service._cache_status(DEVICE_ID, CACHED)
    return device_service


class CachedStatusServabilityTestCase(unittest.TestCase):
    def test_a_slow_device_is_served_from_cache_indefinitely(self):
        # Goal: the false-positive guard. A device that is merely slower than the read timeout has
        # its cache refreshed behind every read, so age alone must never make it unservable.
        # Method: an old cache entry that has not failed a refresh.
        device_service = service_with_cache()
        health = device_service.device_status_health[DEVICE_ID]
        health.cached_at = time.monotonic() - (STALE_STATUS_BUDGET_SECS * 100)
        self.assertTrue(device_service._cached_status_is_servable(DEVICE_ID))

    def test_a_recent_cache_is_served_even_after_a_failed_refresh(self):
        # Goal: one failed refresh is not yet a dead device, so the cache still stands in while
        # the next refresh is attempted. Method: a failed refresh against a fresh entry.
        device_service = service_with_cache()
        device_service.device_status_health[DEVICE_ID].refresh_failed = True
        self.assertTrue(device_service._cached_status_is_servable(DEVICE_ID))

    def test_an_old_cache_with_a_failed_refresh_is_not_servable(self):
        # Goal: the negative space, both conditions together. This is the dead device.
        # Method: age past the budget plus a demonstrated refresh failure.
        device_service = service_with_cache()
        health = device_service.device_status_health[DEVICE_ID]
        health.refresh_failed = True
        health.cached_at = time.monotonic() - (STALE_STATUS_BUDGET_SECS + 1)
        self.assertFalse(device_service._cached_status_is_servable(DEVICE_ID))

    def test_an_unfilled_cache_is_not_servable(self):
        # Goal: a device that has never answered has nothing to serve.
        # Method: a service with no cache entry at all.
        device_service = DeviceService()
        self.assertFalse(device_service._cached_status_is_servable(DEVICE_ID))

    def test_a_successful_read_ends_the_stale_episode(self):
        # Goal: recovery. A device that answers again must go back to being servable and must be
        # able to log a fresh episode later. Method: drive it stale, then cache a good read.
        device_service = service_with_cache()
        health = device_service.device_status_health[DEVICE_ID]
        health.refresh_failed = True
        health.cached_at = time.monotonic() - (STALE_STATUS_BUDGET_SECS + 1)
        device_service._log_stale_status_once(DEVICE_ID)
        device_service._cache_status(DEVICE_ID, CACHED)
        self.assertTrue(device_service._cached_status_is_servable(DEVICE_ID))
        self.assertFalse(device_service.device_status_health[DEVICE_ID].refresh_failed)
        self.assertFalse(device_service.device_status_health[DEVICE_ID].stale_logged)


class StatusServingPathTestCase(unittest.TestCase):
    def test_a_servable_cache_is_returned_and_a_refresh_is_submitted(self):
        # Goal: the unchanged happy path for a slow device. Method: a read that times out against
        # a fresh cache entry and an empty queue.
        device_service = service_with_cache()
        status = device_service._get_current_or_cached_device_status(DEVICE_ID)
        self.assertEqual(status, CACHED)
        self.assertIn(
            "_long_async_status_request", device_service.device_executor.submitted
        )

    def test_a_stale_cache_raises_but_still_submits_a_refresh(self):
        # Goal: refusing to serve must not also stop the device from ever recovering. The refresh
        # is what lets it come back, so it has to be submitted before the read fails.
        # Method: drive the cache stale, then take the same timing-out read path.
        device_service = service_with_cache()
        health = device_service.device_status_health[DEVICE_ID]
        health.refresh_failed = True
        health.cached_at = time.monotonic() - (STALE_STATUS_BUDGET_SECS + 1)
        with self.assertRaises(concurrent.futures.TimeoutError):
            device_service._get_current_or_cached_device_status(DEVICE_ID)
        self.assertIn(
            "_long_async_status_request", device_service.device_executor.submitted
        )

    def test_a_busy_queue_still_serves_a_slow_device(self):
        # Goal: the LCD case. With a busy queue the background refresh is never submitted, so
        # refresh_failed never gets set and an age-only rule would falsely escalate.
        # Method: a non-empty queue and a cache far past the age budget.
        device_service = service_with_cache(queue_empty=False)
        health = device_service.device_status_health[DEVICE_ID]
        health.cached_at = time.monotonic() - (STALE_STATUS_BUDGET_SECS * 100)
        status = device_service._get_current_or_cached_device_status(DEVICE_ID)
        self.assertEqual(status, CACHED)
        self.assertEqual(device_service.device_executor.submitted, ["get_status"])


if __name__ == "__main__":
    unittest.main()
