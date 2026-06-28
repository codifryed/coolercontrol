"""Tests for DeviceExecutor's per-channel write coalescing.

These exercise `submit_write`'s cap-at-1 supersession against a stub
`fn` that records calls; no liquidctl drivers or HTTP layer are
involved.

Run from anywhere; the test inserts its own directory into `sys.path`
so `import main` works regardless of CWD. Three invocations all work:

    # from this directory:
    python3 -m unittest test_executor

    # from anywhere via the script entry point:
    python3 coolercontrold/daemon/resources/liqctld/test_executor.py

    # from anywhere via unittest discover:
    python3 -m unittest discover -s coolercontrold/daemon/resources/liqctld
"""

import concurrent.futures
import os
import sys
import threading
import time
import unittest

# `main.py` lives next to this test; make it importable regardless of
# the caller's CWD or PYTHONPATH so the test runs from anywhere.
_THIS_DIR = os.path.dirname(os.path.abspath(__file__))
if _THIS_DIR not in sys.path:
    sys.path.insert(0, _THIS_DIR)

from main import (  # noqa: E402  (sys.path mutation above)
    WEDGE_THRESHOLD_SECS,
    DeviceExecutor,
)


class _CallRecorder:
    """Thread-safe call log; the stub `fn` we hand to the executor.

    Each invocation appends `kwargs` to `calls`. A `gate` threading
    Event lets a test hold the worker mid-job to control timing.
    """

    def __init__(self):
        self.calls = []
        self.gate = threading.Event()
        self.gate.set()  # default: no blocking
        self._lock = threading.Lock()

    def __call__(self, **kwargs):
        # Block here if the test has cleared the gate; lets us drive
        # the worker into a controlled "busy" state to exercise the
        # supersession path before the marker is consumed.
        self.gate.wait()
        with self._lock:
            self.calls.append(kwargs)

    def block(self):
        self.gate.clear()

    def release(self):
        self.gate.set()

    @property
    def call_count(self):
        with self._lock:
            return len(self.calls)


class DeviceExecutorCoalescerTests(unittest.TestCase):
    def setUp(self):
        self.executor = DeviceExecutor()
        self.executor.set_number_of_devices(1)
        self.device_id = 1

    def tearDown(self):
        self.executor.shutdown()

    def test_submit_write_coalesces_same_key(self):
        """Goal: three submissions for the same (device, dedup_key)
        with the worker held busy collapse to a single call against
        the stub fn. The first two futures resolve `None` via
        supersession (no kwargs ever reach fn); only the latest
        target is applied.
        """
        recorder = _CallRecorder()
        recorder.block()

        # Prime the worker: enqueue an unrelated FIFO job that holds
        # the worker thread, so subsequent submit_write calls all
        # land in pending_writes / the queue without being drained.
        primer_recorder = _CallRecorder()
        primer_recorder.block()
        primer = self.executor.submit(self.device_id, primer_recorder, mark="primer")
        # Wait briefly for the worker to pick up the primer.
        time.sleep(0.05)

        # Three submissions, same dedup_key, latest wins.
        f1 = self.executor.submit_write(
            self.device_id, "speed:fan1", recorder, channel="fan1", duty=10
        )
        f2 = self.executor.submit_write(
            self.device_id, "speed:fan1", recorder, channel="fan1", duty=20
        )
        f3 = self.executor.submit_write(
            self.device_id, "speed:fan1", recorder, channel="fan1", duty=30
        )

        # f1 and f2 resolve immediately on supersession (no I/O).
        self.assertIsNone(f1.result(timeout=1.0))
        self.assertIsNone(f2.result(timeout=1.0))
        self.assertFalse(f3.done(), "latest waiter must still be pending")

        # Release the primer + the latest write to drain.
        primer_recorder.release()
        recorder.release()
        primer.result(timeout=1.0)
        f3.result(timeout=1.0)

        self.assertEqual(
            recorder.calls,
            [{"channel": "fan1", "duty": 30}],
            "only the latest target hits fn",
        )

    def test_submit_write_independent_keys_run_separately(self):
        """Goal: writes with different dedup_keys do not supersede
        each other. Both run, both record their call.
        """
        recorder = _CallRecorder()
        f_speed = self.executor.submit_write(
            self.device_id, "speed:fan1", recorder, channel="fan1", duty=50
        )
        f_color = self.executor.submit_write(
            self.device_id, "color:logo", recorder, channel="logo", mode="static"
        )
        f_speed.result(timeout=1.0)
        f_color.result(timeout=1.0)
        # Order matters less than presence; both keys must have run.
        self.assertEqual(recorder.call_count, 2)
        kwargs_set = {tuple(sorted(c.items())) for c in recorder.calls}
        self.assertIn(
            tuple(sorted({"channel": "fan1", "duty": 50}.items())), kwargs_set
        )
        self.assertIn(
            tuple(sorted({"channel": "logo", "mode": "static"}.items())), kwargs_set
        )

    def test_submit_fifo_path_unaffected_by_coalescer(self):
        """Goal: plain `submit` (no dedup_key) keeps its FIFO
        semantics — every call runs exactly once even when
        interleaved with `submit_write` for the same device.
        """
        recorder = _CallRecorder()
        # Three FIFO jobs interleaved with two coalescable writes.
        # FIFO jobs must all run; the two coalescable writes
        # collapse to one (latest wins).
        f_fifo_a = self.executor.submit(self.device_id, recorder, tag="fifo_a")
        f_write_a = self.executor.submit_write(
            self.device_id, "speed:fan2", recorder, channel="fan2", duty=11
        )
        f_fifo_b = self.executor.submit(self.device_id, recorder, tag="fifo_b")
        f_write_b = self.executor.submit_write(
            self.device_id, "speed:fan2", recorder, channel="fan2", duty=22
        )
        f_fifo_c = self.executor.submit(self.device_id, recorder, tag="fifo_c")

        f_fifo_a.result(timeout=1.0)
        f_fifo_b.result(timeout=1.0)
        f_fifo_c.result(timeout=1.0)
        # f_write_a was superseded; resolves None without running fn.
        self.assertIsNone(f_write_a.result(timeout=1.0))
        # f_write_b is the surviving latest write.
        f_write_b.result(timeout=1.0)

        tags = [c.get("tag") for c in recorder.calls if "tag" in c]
        self.assertEqual(tags, ["fifo_a", "fifo_b", "fifo_c"], "FIFO order preserved")
        speed_calls = [c for c in recorder.calls if c.get("channel") == "fan2"]
        self.assertEqual(
            speed_calls,
            [{"channel": "fan2", "duty": 22}],
            "only the latest coalesced target runs",
        )

    def test_device_queue_empty_reflects_pending_writes(self):
        """Goal: device_queue_empty returns False while a coalesced
        write is queued or held; True only when both the FIFO queue
        and the per-device pending_writes map are empty.
        """
        recorder = _CallRecorder()
        recorder.block()

        # Pre-seed via FIFO so the worker is held busy and the
        # subsequent coalesced write sits in pending_writes.
        f_busy = self.executor.submit(self.device_id, recorder, tag="busy")
        time.sleep(0.05)  # let worker pick it up

        f_write = self.executor.submit_write(
            self.device_id, "speed:fan3", recorder, channel="fan3", duty=42
        )
        self.assertFalse(
            self.executor.device_queue_empty(self.device_id),
            "pending write must count as non-empty",
        )

        recorder.release()
        f_busy.result(timeout=1.0)
        f_write.result(timeout=1.0)
        # After both drain, no pending writes and the FIFO queue is empty.
        # Allow a brief moment for the worker to settle after run().
        for _ in range(20):
            if self.executor.device_queue_empty(self.device_id):
                break
            time.sleep(0.01)
        self.assertTrue(self.executor.device_queue_empty(self.device_id))


class DeviceExecutorWedgeTests(unittest.TestCase):
    def setUp(self):
        self.executor = DeviceExecutor()
        self.executor.set_number_of_devices(1)
        self.device_id = 1

    def tearDown(self):
        self.executor.shutdown()

    def test_wedged_worker_rejects_then_recovers(self):
        """Goal: once a worker has been stuck on a job past
        WEDGE_THRESHOLD_SECS, new submits for that device fail fast
        with a TimeoutError instead of piling up, and resume normally
        once the worker is free again. Method: hold the worker on a
        blocked primer, backdate its job-start time past the threshold
        (so the test needs no real multi-second wait), assert
        submit/submit_write reject immediately and never reach fn, then
        release and assert a fresh submit runs.
        """
        held = _CallRecorder()
        held.block()
        primer = self.executor.submit(self.device_id, held, tag="primer")
        # Wait for the worker to pick up the primer so its start time is recorded.
        for _ in range(100):
            if self.executor._job_running_since.get(self.device_id) is not None:
                break
            time.sleep(0.01)
        self.assertIsNotNone(self.executor._job_running_since.get(self.device_id))

        # Backdate the in-flight job past the wedge threshold to simulate a hung USB call.
        self.executor._job_running_since[self.device_id] = time.monotonic() - (
            WEDGE_THRESHOLD_SECS + 1.0
        )

        rejected = _CallRecorder()
        f_read = self.executor.submit(self.device_id, rejected, tag="read")
        f_write = self.executor.submit_write(
            self.device_id, "speed:fan1", rejected, channel="fan1", duty=10
        )
        with self.assertRaises(concurrent.futures.TimeoutError):
            f_read.result(timeout=1.0)
        with self.assertRaises(concurrent.futures.TimeoutError):
            f_write.result(timeout=1.0)
        self.assertEqual(rejected.call_count, 0, "rejected work must never reach fn")

        # Releasing the primer clears the wedge; new submits run again.
        held.release()
        primer.result(timeout=1.0)
        recovered = _CallRecorder()
        self.executor.submit(self.device_id, recovered, tag="after").result(timeout=1.0)
        self.assertEqual(
            recovered.call_count, 1, "submits run again once the worker frees up"
        )


if __name__ == "__main__":
    unittest.main()
