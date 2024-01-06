#  CoolerControl - monitor and control your cooling and other devices
#  Copyright (c) 2023  Guy Boldon
#  |
#  This program is free software: you can redistribute it and/or modify
#  it under the terms of the GNU General Public License as published by
#  the Free Software Foundation, either version 3 of the License, or
#  (at your option) any later version.
#  |
#  This program is distributed in the hope that it will be useful,
#  but WITHOUT ANY WARRANTY; without even the implied warranty of
#  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
#  GNU General Public License for more details.
#  |
#  You should have received a copy of the GNU General Public License
#  along with this program.  If not, see <https://www.gnu.org/licenses/>.
# ----------------------------------------------------------------------------------------------------------------------

import logging
import queue
from concurrent.futures import Future, ThreadPoolExecutor
from typing import Callable, Dict

log = logging.getLogger(__name__)


class _DeviceJob:
    def __init__(self, future: Future, fn: Callable, **kwargs) -> None:
        self.future = future
        self.fn = fn
        self.kwargs = kwargs

    def run(self) -> None:
        if not self.future.set_running_or_notify_cancel():
            return
        try:
            result = self.fn(**self.kwargs)
        except BaseException as exc:
            self.future.set_exception(exc)
            # Break a reference cycle with the exception 'exc'
            self = None
        else:
            self.future.set_result(result)


def _queue_worker(dev_queue: queue.SimpleQueue) -> None:
    try:
        while True:
            device_job: _DeviceJob = dev_queue.get()
            if device_job is None:
                return
            device_job.run()
            del device_job
    except BaseException:
        log.critical("Exception in worker", exc_info=True)


class DeviceExecutor:
    """
    Simultaneous communications per device results in mangled data, so we keep each device to its own job queue.
    We simultaneously use a Thread Pool to handle communication with separate devices.
    This enables us to talk in parallel to multiple devices, but keep communication for each device synchronous,
    which results in a pretty big speedup for people who have multiple devices.
    """

    def __init__(self) -> None:
        self._device_channels: Dict[int, queue.SimpleQueue] = {}
        self._thread_pool: ThreadPoolExecutor = None

    def set_number_of_devices(self, number_of_devices: int) -> None:
        if number_of_devices < 1:
            return  # don't set any workers if there are no devices
        self._thread_pool = ThreadPoolExecutor(max_workers=number_of_devices)
        for dev_id in range(1, number_of_devices + 1):
            dev_queue = queue.SimpleQueue()
            self._device_channels[dev_id] = dev_queue
            self._thread_pool.submit(_queue_worker, dev_queue)

    def submit(self, device_id: int, fn: Callable, **kwargs) -> Future:
        assert self._thread_pool is not None
        future = Future()
        device_job = _DeviceJob(future, fn, **kwargs)
        self._device_channels[device_id].put(device_job)
        return future

    def device_queue_empty(self, device_id: int) -> bool:
        return self._device_channels[device_id].empty()

    def shutdown(self) -> None:
        for channel in self._device_channels.values():
            channel.put(None)  # ends queue_worker loops
        if self._thread_pool is not None:
            self._thread_pool.shutdown()
        self._device_channels.clear()
