#! /usr/bin/env python3

# SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
# SPDX-License-Identifier: GPL-3.0-or-later

import concurrent
import importlib.metadata
import io
import json
import logging
import logging as log
import os
import queue
import signal
import socket
import sys
import threading
import time
import traceback
from concurrent.futures import Future, ThreadPoolExecutor
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any, Callable, Dict, List, Optional, Tuple, Union

import liquidctl

# these imports need to be dynamic for older liquidctl versions:
try:
    from liquidctl.driver.aquacomputer import Aquacomputer
    from liquidctl.driver.asetek_pro import HydroPro
    from liquidctl.driver.aura_led import AuraLed
    from liquidctl.driver.commander_core import CommanderCore
    from liquidctl.driver.smart_device import H1V2
except ImportError:
    Aquacomputer = None
    HydroPro = None
    AuraLed = None
    CommanderCore = None
    H1V2 = None
try:  # >= 1.16.0
    from liquidctl.driver.control_hub import ControlHub
    from liquidctl.driver.ga2_lcd import GA2LCD
    from liquidctl.driver.lianli_uni import LianLiUni
except ImportError:
    LianLiUni = None
    ControlHub = None
    GA2LCD = None
from liquidctl.driver.asetek import Legacy690Lc, Modern690Lc
from liquidctl.driver.base import BaseDriver
from liquidctl.driver.commander_pro import CommanderPro
from liquidctl.driver.corsair_hid_psu import CorsairHidPsu
from liquidctl.driver.hydro_platinum import HydroPlatinum
from liquidctl.driver.kraken2 import Kraken2
from liquidctl.driver.kraken3 import KrakenZ3
from liquidctl.driver.smart_device import SmartDevice, SmartDevice2
from liquidctl.error import NotSupportedByDriver, Timeout
from PIL import Image

_ORIGINAL_KRAKENZ3_CONNECT = KrakenZ3.connect
_ORIGINAL_SWITCH_BUCKET = KrakenZ3._switch_bucket
_ORIGINAL_SEND_DATA = KrakenZ3._send_data
_ORIGINAL_SET_SCREEN = KrakenZ3.set_screen
_ORIGINAL_INITIALIZE = KrakenZ3.initialize
_ORIGINAL_SEND_2023_FW2 = getattr(KrakenZ3, "_send_2023_data_fw2", None)
# Which packing layouts the C path replaced, reported per device by the profile line.
_LCD_PACKING_PATCHED: set = set()

#####################################################################
# Basic Setup
#####################################################################

SOCKET_ADDRESS: str = "/run/coolercontrold-liqctld.sock"
DEVICE_TIMEOUT_SECS: float = 9.5
DEVICE_READ_STATUS_TIMEOUT_SECS: float = 0.550
# A worker still on the same job past this is treated as wedged: a hung USB/HID call we cannot
# interrupt from this thread. New requests for that device then fail fast (instead of piling up
# behind it) until the call returns or the device is reconnected. Kept above the 9.5s job timeout so
# a legitimately slow command is never mistaken for a wedge.
WEDGE_THRESHOLD_SECS: float = DEVICE_TIMEOUT_SECS * 2
# How long a cached status may stand in for a device that is failing its background refreshes.
# Past this the read fails instead, so the daemon sees a missing status and its failsafe can latch.
# Generous against the ~1-2s refresh interval of a device that is merely slow.
STALE_STATUS_BUDGET_SECS: float = 10.0
MAX_CONNECT_LIQUIDCTL_RETRIES: int = 5
# Hard cap from the first shutdown signal to os._exit(). Best-effort device reset (which can include
# an in-flight LCD write of 2+ seconds) runs within this window, then the process exits
# unconditionally so a device wedged in an uninterruptible USB read can never stall systemd past its
# TimeoutStopSec.
SHUTDOWN_DEADLINE_SECS: float = 4.0

#####################################################################
# liquidctl patches
#####################################################################

# Probe used to validate a packing replacement against the original. Square, like every
# LCD liquidctl supports, with all three channels distinct so a swapped channel or a
# wrong rotation cannot pass unnoticed.
_PACKING_PROBE_SIZE: int = 8


# A Kraken LCD frame is written to a "bucket" of device memory, then displayed by switching
# to it.  liquidctl caps each bulk transfer at 512 bytes for the Z-series (every 2023/2024
# model already uses 2 MB), so a 320x320 frame becomes 800 synchronous USB transfers, and it
# rescans all 16 buckets on every frame.  That is where an LCD update's 1.2-2.0 s goes.
# KrakenZPlayground drives the same device (0x1e71:0x3008) at 30 fps by writing the frame in
# one transfer and alternating between two pinned buckets.
_LCD_BULK_TRANSFER_BYTES: int = 2 * 1024 * 1024
# Prefix of the first bulk message, copied from liquidctl's `_send_data`.
_LCD_BULK_HEADER: tuple = (
    0x12,
    0xFA,
    0x01,
    0xE8,
    0xAB,
    0xCD,
    0xEF,
    0x98,
    0x76,
    0x54,
    0x32,
    0x10,
)
# Reply prefix for a bucket query, checked before trusting the offsets it carries.
_LCD_BUCKET_REPLY_PREFIX: list = [0x31, 0x04]
# Bucket memory is accounted in 1 KiB slots.
_LCD_BUCKET_SLOT_BYTES: int = 1024
# `_switch_bucket`'s mode for displaying a bucket. Its other caller passes 0x2, which selects
# liquid mode and carries a bucket index of 0 that means nothing.
_LCD_SWITCH_DISPLAY_MODE: int = 0x4
# The one model liquidctl refuses gifs on, and only at firmware 2.x.
_LCD_GIF_UNSUPPORTED_PID: int = 0x300E


def _connect_with_whole_frame_transfers(self, **kwargs):
    """Raises the bulk transfer cap so a frame is written in one transfer, not 800.

    liquidctl's own cap is kept: a transfer that fails at the raised size returns to it.
    """
    result = _ORIGINAL_KRAKENZ3_CONNECT(self, **kwargs)
    stock = getattr(self, "bulk_buffer_size", _LCD_BULK_TRANSFER_BYTES)
    if stock < _LCD_BULK_TRANSFER_BYTES:
        self._cc_stock_bulk_size = stock
        self.bulk_buffer_size = _LCD_BULK_TRANSFER_BYTES
    return result


def _switch_bucket_tracking_active(self, *args, **kwargs):
    """Records the displayed bucket and the pair we rotate between.

    The pair is learned rather than assumed to be 0 and 1: buckets already in use, or a
    differently sized upload, send liquidctl's allocator elsewhere and the rotation has to
    follow it instead of switching itself off.
    """
    switched = _ORIGINAL_SWITCH_BUCKET(self, *args, **kwargs)
    if switched.__bool__() is False or args.__len__() == 0:
        return switched
    mode = kwargs.get(
        "mode", args[1] if args.__len__() > 1 else _LCD_SWITCH_DISPLAY_MODE
    )
    if mode != _LCD_SWITCH_DISPLAY_MODE:
        # `set_screen(.., "liquid")` and `_delete_all_buckets` both switch mode with bucket 0.
        # Learning that pair would rotate the next frames into a bucket holding no frame.
        return switched
    bucket = args[0]
    pair = getattr(self, "_cc_bucket_pair", ())
    if bucket not in pair:
        pair = (pair[-1], bucket) if pair else (bucket,)
    self._cc_bucket_pair = pair
    self._cc_active_bucket = bucket
    return switched


def _log_lcd_profile_once(self, path, frame_bytes, slots):
    """Reports what this screen is and how it is driven, once per device.

    Info level so a bug report carries it without the reporter re-running with debug. Every
    field is read defensively: a diagnostic must not be why an LCD apply fails on a model
    nobody here has.
    """
    if getattr(self, "_cc_profile_logged", False):
        return
    self._cc_profile_logged = True
    firmware = getattr(self, "_fw", None)
    resolution = getattr(self, "lcd_resolution", None)
    packing = "rgb565" if path == "fw2" else "rgbx"
    replaced = (
        "_prepare_static_file_rgb16" if packing == "rgb565" else "_prepare_static_file"
    )
    log.info(
        "LCD %s pid=%s fw=%s res=%s bulk=%s path=%s packing=%s%s frame=%s slots=%s",
        type(self).__name__,
        f"0x{getattr(self.device, 'product_id', 0):04x}",
        ".".join(str(part) for part in firmware) if firmware else "unknown",
        f"{resolution[0]}x{resolution[1]}" if resolution else "unknown",
        getattr(self, "bulk_buffer_size", "unknown"),
        path,
        packing,
        "" if replaced in _LCD_PACKING_PATCHED else "-unpatched",
        frame_bytes,
        slots,
    )


def _initialize_repriming_the_lcd(self, *args, **kwargs):
    """Re-arms the firmware 2.x doubled first frame.

    Called at boot, on resume and on reconnect, which is the state liquidctl's own comment
    says that second transfer is there for.
    """
    self._cc_fw2_primed = False
    return _ORIGINAL_INITIALIZE(self, *args, **kwargs)


def _write_frame_fw2(self, data, bulk_info):
    """One firmware 2.x transfer: open, the frame, close.

    No buckets. This firmware streams the frame straight at the screen, which is why none of
    the rotation applies here and why there is nothing to keep off the display.
    """
    self._write_then_read([0x36, 0x01, 0x00, 0x01, 0x06])
    _bulk_write_frame(self, list(_LCD_BULK_HEADER) + bulk_info, data)
    self._write_then_read([0x36, 0x02])


def _send_frame_fw2(self, data, bulk_info):
    """Kraken 2023 firmware 2.x transfer, doubled only on the first frame after an initialize.

    liquidctl sends every frame twice here, commenting that the second is "only required
    once after initialization" and that NZXT's own software does the same at init. A daemon
    that stays connected can take that at its word: prime once, then one frame per apply
    instead of two. Why the device wants the second is unestablished, so it is kept for the
    frame liquidctl says needs it rather than reasoned away.

    The drain matters more than it looks. `_write_then_read` returns whichever report
    arrives next and checks nothing, so a status report the device sent unasked while the
    frame was decoded becomes the answer to the transfer-start command, and every read after
    it is one behind until something clears the queue.
    """
    _log_lcd_profile_once(self, "fw2", len(data), "n/a")
    self.device.clear_enqueued_reports()
    _write_frame_fw2(self, data, bulk_info)
    if getattr(self, "_cc_fw2_primed", False):
        return
    _write_frame_fw2(self, data, bulk_info)
    self._cc_fw2_primed = True


def lcd_gif_supported(lc_device) -> Optional[bool]:
    """Whether this screen can show a gif, or None where the device does not answer for it.

    liquidctl refuses gifs on the Kraken 2023 at firmware 2.x (its issue #631) and gates
    that on the product id and the firmware major together. The daemon never sees a product
    id, so the answer travels rather than the inputs. Read after `initialize`, which is what
    populates `_fw`.
    """
    if isinstance(lc_device, KrakenZ3) is False:
        return None
    if getattr(lc_device.device, "product_id", None) != _LCD_GIF_UNSUPPORTED_PID:
        return True
    firmware = getattr(lc_device, "_fw", None)
    return None if firmware is None else firmware[0] != 2


def _set_screen_with_drained_queue(self, channel, mode, value, **kwargs):
    """Drains queued reports before liquidctl reads the screen's brightness and orientation.

    `set_screen` opens with a `_read_until` for the [0x31, 0x01] reply that gives up after twelve
    reports, raising "missing messages (attempts=12, missing=1)". The device streams status
    reports unasked, which is why `_get_status_directly` reads one without writing first, and
    nothing consumes them while a frame is prepared and uploaded, so the backlog accumulates
    across applies until that read starves.

    The drain in `_send_frame_to_spare_bucket` runs after that read and cannot cover it. The
    per-apply `initialize` drained here, which is why this appeared only once it was dropped.
    """
    self.device.clear_enqueued_reports()
    try:
        return _ORIGINAL_SET_SCREEN(self, channel, mode, value, **kwargs)
    except NotSupportedByDriver:
        # Firmware 2.x cannot show gifs at all (liquidctl issue #631). Said once per device:
        # the daemon re-applies on a schedule, and the error alone reads as a fault.
        if mode == "gif" and not getattr(self, "_cc_gif_unsupported_logged", False):
            self._cc_gif_unsupported_logged = True
            log.warning(
                "This Kraken's firmware does not support LCD gifs; still images work. "
                "Choose a non-gif LCD mode to stop the repeated errors."
            )
        raise


def _log_lcd_size_fallback_once(self, bucket, capacity, slots_needed):
    """Says once per device that frames are outgrowing the bucket they rotate through.

    The rotation reuses an allocation, so a frame larger than the one that made it has
    nowhere to go and takes liquidctl's own upload, re-initialize and all. Only variable
    content does this: a static image is a fixed size for a given screen, a gif is not, and
    a 640x640 screen is where gifs get big enough to leave no room for a second bucket. At
    debug this left an LCD that is slow for a knowable reason looking mysterious.
    """
    if getattr(self, "_cc_size_fallback_logged", False):
        return
    self._cc_size_fallback_logged = True
    log.info(
        "LCD frames are outgrowing bucket %s (needs %s KiB, holds %s), so these frames take "
        "liquidctl's slower upload. Expected with gifs, which vary in size.",
        bucket,
        slots_needed,
        capacity,
    )


def _fall_back_to_liquidctl(self, data, bulk_info, reason):
    """Hands the frame to liquidctl's own upload, recording which gate turned it away.

    A silent fallback reads as a working rotation while every frame pays the re-initialize.
    """
    log.debug("LCD rotation skipped (%s); using liquidctl's own upload", reason)
    return _send_data_with_clean_slate(self, data, bulk_info)


def _send_data_with_clean_slate(self, data, bulk_info):
    """liquidctl's own frame upload, preceded by the re-initialization it needs.

    This path can rewrite the displayed bucket, so it keeps the artifact clearing. A failed
    initialize still uploads: a possible artifact beats no image.
    """
    try:
        self.initialize()
    except Exception as err:  # noqa: BLE001
        log.warning("Could not re-initialize before an LCD frame upload: %s", err)
    return _ORIGINAL_SEND_DATA(self, data, bulk_info)


def _bulk_write_frame(self, header, data):
    """Writes the frame at the connection's transfer size, whole where it fits.

    Handed over unsliced in that case: slicing a 400 KB frame copies it, every frame. The
    loop is what makes the cap mean something once it is lowered.

    A write that timed out cannot be resumed, so the retry is the next frame, and it takes
    liquidctl's own transfer size. liquidctl cannot do that, since each run starts over; a
    daemon that stays up is what makes remembering the failure worth anything.
    """
    chunk = getattr(self, "bulk_buffer_size", 0) or len(data)
    try:
        self._bulk_write(header)
        if len(data) <= chunk:
            self._bulk_write(data)
            return
        for start in range(0, len(data), chunk):
            end = start + chunk
            self._bulk_write(data[start:end])
    except Timeout:
        # Only a timeout says anything about the transfer size. A disconnect raises
        # `USBError`, and blaming the size for that would slow every later frame.
        stock = getattr(self, "_cc_stock_bulk_size", chunk)
        if stock < chunk:
            self.bulk_buffer_size = stock
            log.warning(
                "An LCD frame timed out in one %d byte write; falling back to "
                "liquidctl's %d byte transfers for this device.",
                chunk,
                stock,
            )
        raise


def _send_frame_to_spare_bucket(self, data, bulk_info):
    """Writes a frame to the bucket that is not on screen, reusing its existing allocation.

    liquidctl queries all 16 buckets and allocates a fresh one per frame, ~16 HID round trips,
    and once all 16 are occupied it rewrites whichever is on screen. That rewriting produced
    the occasional image artifacts, and the per-apply `initialize` is what cleared them. This
    rotation never touches the displayed bucket, so it needs none of that, which is what makes
    dropping that initialize safe; the fallback keeps it, since it returns to the allocator.

    Every value on the wire comes from liquidctl's helpers or the device (the memory offset is
    read back, never computed). Anything unexpected falls back to `_send_data` unchanged.
    """
    header = list(_LCD_BULK_HEADER) + bulk_info
    slots_needed = -(-(len(header) + len(data)) // _LCD_BUCKET_SLOT_BYTES)
    _log_lcd_profile_once(self, "rotation", len(data), slots_needed)

    pair = getattr(self, "_cc_bucket_pair", ())
    active_bucket = getattr(self, "_cc_active_bucket", None)
    if len(pair) < 2 or active_bucket not in pair:
        # Not rotating yet: let liquidctl allocate and learn the bucket it picks. Two frames
        # establish it, so this repeating means `_switch_bucket` is not reporting success.
        return _fall_back_to_liquidctl(
            self, data, bulk_info, f"pair={pair} active={active_bucket}"
        )

    target_bucket = pair[0] if pair[1] == active_bucket else pair[1]

    # Drain again: `set_screen` drained on entry, but decoding and resizing the frame since
    # then gave the stream time to refill. What a backlog costs depends on which read meets it.
    # `_write_then_read` takes a leftover as the answer and goes silently off by one, which the
    # prefix check below guards; `_delete_bucket` burns its twelve attempts and raises.
    # Whether this path orphans its own report is unestablished: `_write([0x36, 0x02])` below
    # reads no reply, and the firmware-2 path issues that command through `_write_then_read`.
    self.device.clear_enqueued_reports()

    bucket = self._write_then_read([0x30, 0x04, target_bucket])
    if list(bucket[0:2]) != _LCD_BUCKET_REPLY_PREFIX:
        # Not the bucket's reply, so its bytes mean nothing: an offset read out of an
        # unrelated report would place the frame anywhere.
        return _fall_back_to_liquidctl(
            self, data, bulk_info, f"reply prefix {list(bucket[0:2])}"
        )
    occupied = any(bucket[15:])
    capacity = int.from_bytes([bucket[19], bucket[20]], "little")
    if not occupied or capacity < slots_needed:
        # First use of this bucket, or the frame outgrew it. Only the second is worth
        # reporting: the first is how a rotation starts.
        if occupied:
            _log_lcd_size_fallback_once(self, target_bucket, capacity, slots_needed)
        return _fall_back_to_liquidctl(
            self,
            data,
            bulk_info,
            f"bucket {target_bucket} occupied={occupied} "
            f"capacity={capacity} needs={slots_needed}",
        )

    memory_start = [bucket[17], bucket[18]]
    self._write_then_read([0x36, 0x03])
    # One delete, though liquidctl's `_prepare_bucket` deletes twice when it reuses a bucket
    # that holds data. It reaches that only with all 16 occupied, and this device answers one
    # delete here: a redundant one costs a round trip per frame, and if the device does not
    # reply to it `_read_until_first_match` burns twelve reads and then asserts mid-frame.
    # An incomplete free is caught by the setup below instead.
    if not self._delete_bucket(target_bucket):
        # The device refused (0x9, in use). liquidctl answers that by moving to another
        # bucket, which is the allocation this rotation exists to avoid doing itself.
        return _fall_back_to_liquidctl(
            self, data, bulk_info, f"bucket {target_bucket} delete refused"
        )
    if not self._setup_bucket(
        target_bucket,
        target_bucket + 1,
        memory_start,
        list(slots_needed.to_bytes(2, "little")),
    ):
        # The allocation was never re-established, so writing now puts a torn frame in
        # memory and the switch below would then display it.
        return _fall_back_to_liquidctl(
            self, data, bulk_info, f"bucket {target_bucket} setup refused"
        )
    self._write_then_read([0x36, 0x01, target_bucket])
    _bulk_write_frame(self, header, data)
    self._write([0x36, 0x02])
    self._switch_bucket(target_bucket)
    # Record what we asked for rather than what the switch reported. liquidctl never reads
    # the end-of-transfer reply above, so `_switch_bucket`'s success flag is taken from that
    # stale message instead of its own; believing a false negative would leave the rotation
    # pointed at the bucket that is now on screen, and the next frame would overwrite it.
    self._cc_active_bucket = target_bucket


_ORIGINAL_CORSAIR_PSU_EXEC = getattr(CorsairHidPsu, "_exec", None)


def _exec_with_drained_queue(self, writebit, command, data=None):
    """Drains queued reports before every Corsair HID PSU command.

    `_exec` writes a command and asserts that the report it reads back echoes it, but the PSU also
    sends reports unasked. One that arrives while nothing is reading becomes the answer to the next
    command, and every read after it is one behind, which surfaces as "invalid response (possible
    conflict with another program)".

    liquidctl drains for this in `_get_status_directly` and nowhere else, so `initialize` walks
    straight into it, and a resume is exactly when a backlog is waiting: the device kept sending
    while the system was asleep. That failure is what made a PSU exhaust every init retry after a
    wake and stay unusable for minutes, until enough reads had drained the queue by hand.

    Draining before the write is always safe. Anything already queued was sent before the request
    went out, so it can never be that request's answer.
    """
    self.device.clear_enqueued_reports()
    return _ORIGINAL_CORSAIR_PSU_EXEC(self, writebit, command, data)


def patch_corsair_psu_report_drain() -> bool:
    """Installs the drain above. See `_exec_with_drained_queue`."""
    if _ORIGINAL_CORSAIR_PSU_EXEC is None:
        log.warning("liquidctl CorsairHidPsu is missing _exec; report drain unpatched")
        return False
    CorsairHidPsu._exec = _exec_with_drained_queue
    log.debug("Corsair PSU commands patched to drain queued reports first")
    return True


def patch_kraken_lcd_transfer() -> bool:
    """Installs the whole-frame transfer and the two-bucket rotation.

    Returns whether both were installed.  These change how the device is driven rather than
    what bytes it receives, so unlike the packing replacement they cannot be verified against
    a reference here; they degrade to liquidctl's own path whenever their preconditions do
    not hold.
    """
    required = (
        "_send_data",
        "_switch_bucket",
        "_delete_bucket",
        "_setup_bucket",
        "_bulk_write",
        "_write_then_read",
        "_write",
        "set_screen",
        "initialize",
    )
    missing = [name for name in required if hasattr(KrakenZ3, name) is False]
    if missing:
        log.warning("liquidctl KrakenZ3 is missing %s; LCD transfer unpatched", missing)
        return False
    KrakenZ3.connect = _connect_with_whole_frame_transfers
    KrakenZ3._switch_bucket = _switch_bucket_tracking_active
    KrakenZ3._send_data = _send_frame_to_spare_bucket
    KrakenZ3.set_screen = _set_screen_with_drained_queue
    KrakenZ3.initialize = _initialize_repriming_the_lcd
    if _ORIGINAL_SEND_2023_FW2 is not None:
        KrakenZ3._send_2023_data_fw2 = _send_frame_fw2
    log.debug("LCD transfer patched to whole-frame writes with a two-bucket rotation")
    return True


# Channel-to-bit-position maps for the RGB565 frame buffer the Kraken 2023 firmware 2.x wants,
# applied with bytes.translate. Each places a channel's bits where they belong in one of the two
# output bytes; the ranges never overlap, so the halves combine with a plain OR.
_RGB565_RED_HIGH: bytes = bytes(value & 0xF8 for value in range(256))
_RGB565_GREEN_HIGH: bytes = bytes(value >> 5 for value in range(256))
_RGB565_GREEN_LOW: bytes = bytes((value & 0x1C) << 3 for value in range(256))
_RGB565_BLUE_LOW: bytes = bytes(value >> 3 for value in range(256))


def _packed_static_file(self, path, rotation):
    """Drop-in for `KrakenZ3._prepare_static_file` that packs the framebuffer in C.

    liquidctl builds it with a per-pixel python loop: 409,600 `list.append` calls for a
    320x320 screen, about 12 ms of CPU per image. Pillow emits the same RGBX bytes in one
    call; only its padding byte differs (0xFF where the loop writes 0x00), so this zeroes
    it. `_send_data` consumes the result with `len()` and slicing, which a bytearray
    supports, so the transfer path is unchanged.
    """
    image = (
        Image.open(path)
        .resize(self.lcd_resolution)
        .rotate(rotation * -90)
        .convert("RGB")
    )
    packed = bytearray(image.tobytes("raw", "RGBX"))
    packed[3::4] = b"\x00" * (len(packed) // 4)
    return packed


def _packed_static_file_rgb16(self, path, rotation):
    """Drop-in for `KrakenZ3._prepare_static_file_rgb16`, the Kraken 2023 firmware 2.x path.

    Same per-pixel loop problem as the RGBX packing, but this one emits RGB565, high byte
    first: RRRRRGGG GGGBBBBB. Pillow has no encoder for that layout, so the channels are
    combined here instead: translate moves each channel's bits into place, and the two halves
    are OR-ed as whole buffers (their bit ranges are disjoint, so nothing carries). Everything
    stays in C, which is what makes it cheap.
    """
    image = (
        Image.open(path)
        .resize(self.lcd_resolution)
        .rotate(rotation * -90)
        .convert("RGB")
    )
    pixels = image.tobytes("raw", "RGB")
    red, green, blue = pixels[0::3], pixels[1::3], pixels[2::3]
    pixel_count = len(red)
    high = int.from_bytes(red.translate(_RGB565_RED_HIGH), "big") | int.from_bytes(
        green.translate(_RGB565_GREEN_HIGH), "big"
    )
    low = int.from_bytes(green.translate(_RGB565_GREEN_LOW), "big") | int.from_bytes(
        blue.translate(_RGB565_BLUE_LOW), "big"
    )
    frame_buffer = bytearray(pixel_count * 2)
    frame_buffer[0::2] = high.to_bytes(pixel_count, "big")
    frame_buffer[1::2] = low.to_bytes(pixel_count, "big")
    return frame_buffer


def _packing_matches(original, candidate) -> bool:
    """Whether `candidate` reproduces `original` byte for byte, at every rotation.

    Run before installing a replacement so an unexpected Pillow build cannot put wrong
    bytes on someone's screen: a mismatch keeps liquidctl's own implementation.
    """

    class _Probe:
        lcd_resolution = (_PACKING_PROBE_SIZE, _PACKING_PROBE_SIZE)

    image = Image.new("RGB", (_PACKING_PROBE_SIZE, _PACKING_PROBE_SIZE))
    pixels = image.load()
    for y in range(_PACKING_PROBE_SIZE):
        for x in range(_PACKING_PROBE_SIZE):
            pixels[x, y] = (x * 31, y * 31, 255 - x * 31)
    encoded = io.BytesIO()
    image.save(encoded, format="PNG")
    probe = _Probe()
    for rotation in (0, 1, 2, 3):
        encoded.seek(0)
        expected = bytes(original(probe, encoded, rotation))
        encoded.seek(0)
        try:
            actual = bytes(candidate(probe, encoded, rotation))
        except Exception:  # noqa: BLE001 - any failure means keep the original
            return False
        if actual != expected:
            return False
    return True


def patch_kraken_lcd_packing() -> bool:
    """Installs the C-level framebuffer packing, where it matches liquidctl's output.

    Two layouts: RGBX for every Kraken with a screen, and RGB565 for the Kraken 2023 on
    firmware 2.x. Each is checked against liquidctl's own implementation before being
    installed, so a mismatch costs CPU rather than putting wrong bytes on a screen.

    A liquidctl older than the one the C path reproduces will not match, and will not
    carry the RGB565 method at all. Both are supported: the stock implementation is used
    and everything still works, which is why neither reports as a warning.

    Returns whether both were installed.
    """
    replacements = (
        ("_prepare_static_file", _packed_static_file),
        ("_prepare_static_file_rgb16", _packed_static_file_rgb16),
    )
    installed = 0
    for name, replacement in replacements:
        original = getattr(KrakenZ3, name, None)
        if original is None:
            log.info(
                "This liquidctl has no KrakenZ3.%s, so that LCD packing stays on "
                "liquidctl's own code. Expected on liquidctl older than its Kraken "
                "2023 firmware 2.x support, such as the one Debian bookworm ships.",
                name,
            )
            continue
        if not _packing_matches(original, replacement):
            log.info(
                "This liquidctl packs %s differently to the implementation the C path "
                "reproduces, so liquidctl's own code stays in use. Expected on older "
                "liquidctl, such as the one Debian bookworm ships. The only effect is "
                "that LCD updates cost more CPU.",
                name,
            )
            continue
        setattr(KrakenZ3, name, replacement)
        _LCD_PACKING_PATCHED.add(name)
        installed += 1
    log.debug("LCD framebuffer packing patched to Pillow's C path (%d of 2)", installed)
    return installed == len(replacements)


#####################################################################
# Models
#####################################################################

Statuses = List[Tuple[str, str, str]]


class LiquidctlException(Exception):
    pass


class LiqctldException(Exception):
    def __init__(self, code: HTTPStatus, message: str) -> None:
        json_message: str = json.dumps(self._to_dict(code, message))
        super().__init__(json_message)
        self.message = json_message
        self.code = code

    @staticmethod
    def _to_dict(code: HTTPStatus, message: str) -> Dict[str, Any]:
        return {
            "code": code.value,
            "message": message,
        }

    def __str__(self) -> str:
        return self.message


class BaseModel:
    def to_dict(self) -> Dict[str, Any]:
        return self.__dict__

    def to_dict_no_none(self) -> Dict[str, Any]:
        """Returns a dictionary without any None values"""
        return {k: v for k, v in self.to_dict().items() if v is not None}

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> Any:
        cls.__dict__.update(data)

    def to_json(self) -> str:
        return json.dumps(self.to_dict())


class DeviceProperties(BaseModel):
    def __init__(
        self,
        speed_channels=None,
        color_channels=None,
        supports_cooling: Optional[bool] = None,
        supports_cooling_profiles: Optional[bool] = None,
        supports_lighting: Optional[bool] = None,
        led_count: Optional[int] = None,
        lcd_resolution: Optional[Tuple[int, int]] = None,
    ):
        if color_channels is None:
            color_channels = list()
        if speed_channels is None:
            speed_channels = []
        self.speed_channels: List[str] = speed_channels
        self.color_channels: List[str] = color_channels
        self.supports_cooling: Optional[bool] = supports_cooling
        self.supports_cooling_profiles: Optional[bool] = supports_cooling_profiles
        self.supports_lighting: Optional[bool] = supports_lighting
        self.led_count: Optional[int] = led_count
        self.lcd_resolution: Optional[Tuple[int, int]] = lcd_resolution

    def to_dict(self) -> Dict[str, Any]:
        return {
            "speed_channels": self.speed_channels,
            "color_channels": self.color_channels,
            "supports_cooling": self.supports_cooling,
            "supports_cooling_profiles": self.supports_cooling_profiles,
            "supports_lighting": self.supports_lighting,
            "led_count": self.led_count,
            "lcd_resolution": self.lcd_resolution,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "DeviceProperties":
        return cls(
            speed_channels=data.get("speed_channels", []),
            color_channels=data.get("color_channels", []),
            supports_cooling=data.get("supports_cooling", None),
            supports_cooling_profiles=data.get("supports_cooling_profiles", None),
            supports_lighting=data.get("supports_lighting", None),
            led_count=data.get("led_count", None),
            lcd_resolution=data.get("lcd_resolution", None),
        )


class Device(BaseModel):
    def __init__(
        self,
        id: int,
        description: str,
        device_type: str,
        properties: DeviceProperties,
        liquidctl_version: str,
        serial_number: Optional[str] = None,
        hid_address: Optional[str] = None,
        hwmon_address: Optional[str] = None,
    ):
        self.id: int = id
        self.description: str = description
        self.device_type: str = device_type
        self.properties: DeviceProperties = properties
        self.liquidctl_version: str = liquidctl_version
        self.serial_number: Optional[str] = serial_number
        self.hid_address: Optional[str] = hid_address
        self.hwmon_address: Optional[str] = hwmon_address

    def to_dict(self) -> Dict[str, Any]:
        return {
            "id": self.id,
            "description": self.description,
            "device_type": self.device_type,
            "serial_number": self.serial_number,
            "properties": self.properties.to_dict(),
            "liquidctl_version": self.liquidctl_version,
            "hid_address": self.hid_address,
            "hwmon_address": self.hwmon_address,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Device":
        try:
            return cls(
                id=data["id"],
                description=data["description"],
                device_type=data["device_type"],
                properties=DeviceProperties.from_dict(data["properties"]),
                liquidctl_version=data["liquidctl_version"],
                serial_number=data.get("serial_number", None),
                hid_address=data.get("hid_address", None),
                hwmon_address=data.get("hwmon_address", None),
            )
        except KeyError:
            raise LiqctldException(
                HTTPStatus.BAD_REQUEST, f"Invalid Device Body: {data}"
            ) from None


class Handshake(BaseModel):
    def __init__(self, shake: bool = False):
        self.shake: bool = shake

    def to_dict(self) -> Dict[str, Any]:
        return {
            "shake": self.shake,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Handshake":
        return cls(
            shake=data.get("shake", False),
        )


class InitRequest(BaseModel):
    def __init__(self, pump_mode: Optional[str] = None):
        self.pump_mode: Optional[str] = pump_mode

    def to_dict(self) -> Dict[str, Any]:
        return {
            "pump_mode": self.pump_mode,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "InitRequest":
        return cls(
            pump_mode=data.get("pump_mode", None),
        )


class FixedSpeedRequest(BaseModel):
    def __init__(self, channel: str, duty: int):
        self.channel: str = channel
        self.duty: int = duty

    def to_dict(self) -> Dict[str, Any]:
        return {
            "channel": self.channel,
            "duty": self.duty,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "FixedSpeedRequest":
        try:
            return cls(
                channel=data["channel"],
                duty=data["duty"],
            )
        except KeyError:
            raise LiqctldException(
                HTTPStatus.BAD_REQUEST, f"Invalid FixedSpeedRequest Body: {data}"
            ) from None


class SpeedProfileRequest(BaseModel):
    def __init__(
        self, channel: str, profile=None, temperature_sensor: Optional[int] = None
    ):
        self.channel: str = channel
        if profile is None:
            profile = []
        self.profile: List[Tuple[float, int]] = profile
        self.temperature_sensor: Optional[int] = temperature_sensor

    def to_dict(self) -> Dict[str, Any]:
        return {
            "channel": self.channel,
            "profile": self.profile,
            "temperature_sensor": self.temperature_sensor,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "SpeedProfileRequest":
        try:
            model = cls(
                channel=data["channel"],
                profile=data.get("profile", []),
                temperature_sensor=data.get("temperature_sensor", None),
            )
            # Pydantic used to auto cast floats sent by the daemon to int.
            # This is still wanted because several liquidctl drivers require int as temps.
            # Also, the default liquidctl CLI operation uses int for temps,
            #  so it is consistent with the daemon and the UI doesn't allow <1C intervals.
            # If one wants more precise control, the user should use a Standard Function to avoid
            # use of the in-built speed profiles.
            model.profile = [
                (int(temp_duty[0]), temp_duty[1]) for temp_duty in list(model.profile)
            ]
            return model
        except KeyError:
            raise LiqctldException(
                HTTPStatus.BAD_REQUEST, f"Invalid SpeedProfileRequest Body: {data}"
            ) from None


class ControlModeRequest(BaseModel):
    def __init__(self, channel: str, desired_state: str):
        self.channel: str = channel
        self.desired_state: str = desired_state

    def to_dict(self) -> Dict[str, Any]:
        return {
            "channel": self.channel,
            "desired_state": self.desired_state,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "ControlModeRequest":
        try:
            return cls(
                channel=data["channel"],
                desired_state=data["desired_state"],
            )
        except KeyError:
            raise LiqctldException(
                HTTPStatus.BAD_REQUEST, f"Invalid ControlModeRequest Body: {data}"
            ) from None


class ColorRequest(BaseModel):
    def __init__(
        self,
        channel: str,
        mode: str,
        colors: List[List[int]],
        time_per_color: Optional[int] = None,
        speed: Optional[str] = None,
        direction: Optional[str] = None,
    ):
        self.channel: str = channel
        self.mode: str = mode
        self.colors: List[List[int]] = colors
        self.time_per_color: Optional[int] = time_per_color
        self.speed: Optional[str] = speed
        self.direction: Optional[str] = direction

    def to_dict(self) -> Dict[str, Any]:
        return {
            "channel": self.channel,
            "mode": self.mode,
            "colors": self.colors,
            "time_per_color": self.time_per_color,
            "speed": self.speed,
            "direction": self.direction,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "ColorRequest":
        try:
            return cls(
                channel=data["channel"],
                mode=data["mode"],
                colors=data["colors"],
                time_per_color=data.get("time_per_color", None),
                speed=data.get("speed", None),
                direction=data.get("direction", None),
            )
        except KeyError:
            raise LiqctldException(
                HTTPStatus.BAD_REQUEST, f"Invalid ColorRequest Body: {data}"
            ) from None


class ScreenRequest(BaseModel):
    channel: str
    mode: str
    value: Optional[str]

    def __init__(
        self,
        channel: str,
        mode: str,
        value: Optional[str] = None,
    ):
        self.channel: str = channel
        self.mode: str = mode
        self.value: Optional[str] = value

    def to_dict(self) -> Dict[str, Any]:
        return {
            "channel": self.channel,
            "mode": self.mode,
            "value": self.value,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "ScreenRequest":
        try:
            return cls(
                channel=data["channel"],
                mode=data["mode"],
                value=data.get("value", None),
            )
        except KeyError:
            raise LiqctldException(
                HTTPStatus.BAD_REQUEST, f"Invalid ScreenRequest Body: {data}"
            ) from None


#####################################################################
# Executor
#####################################################################


class _DeviceJob:
    def __init__(
        self,
        future: Future,
        fn: Callable,
        dedup_key: Optional[str] = None,
        **kwargs,
    ) -> None:
        self.future = future
        self.fn = fn
        self.kwargs = kwargs
        # When set, this job is coalescable: the worker pulls the latest
        # entry for this key from `_pending_writes` rather than running
        # the marker itself, so a newer submission for the same channel
        # can supersede an earlier one.
        self.dedup_key: Optional[str] = dedup_key

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


def _queue_worker(executor: "DeviceExecutor", device_id: int) -> None:
    """Drain one device's queue. For markers carrying a `dedup_key`,
    pop the *latest* job from `executor._pending_writes` and run that
    instead of the marker itself; this is what makes a newer write
    supersede an earlier waiting one.
    """
    dev_queue = executor._device_queues[device_id]
    try:
        while True:
            marker: _DeviceJob = dev_queue.get()
            # our logic for stoping the queue loop
            if marker is None:
                return
            dedup_key = marker.dedup_key
            if dedup_key is not None:
                with executor._lock:
                    pending = executor._pending_writes.get(device_id)
                    job = pending.pop(dedup_key, None) if pending else None
                if job is None:
                    # Already resolved: a later submission popped it via
                    # supersession before we got here, or the marker
                    # raced shutdown. Either way, nothing to run.
                    continue
                to_run = job
            else:
                to_run = marker
            # Mark the device busy so submit() can spot a worker wedged on a hung USB call: a truly
            # wedged run() never returns, so _job_running_since stays set (the finally never fires)
            # until the device recovers.
            executor._job_running_since[device_id] = time.monotonic()
            try:
                to_run.run()
            finally:
                executor._job_running_since[device_id] = None
            del to_run
            del marker
    except BaseException as exc:
        sys.stderr.write(f"Exception in worker: {exc}\n")


class DeviceExecutor:
    """
    Simultaneous communications per device result in mangled data,
    so we keep each device to its own job queue.
    We simultaneously use a Thread Pool to handle communication with separate devices.
    This enables us to talk in parallel to multiple devices,
    but keep communication for each device synchronous,
    which results in a pretty big speedup for people who have multiple devices.

    Per-channel coalescing: control writes (set_fixed_speed,
    set_speed_profile, set_color, set_screen) submit through
    `submit_write(device_id, dedup_key, ...)`. At most one pending
    write per `(device, dedup_key)` is queued; a newer submission
    replaces the older one and resolves the older `Future` with
    `None` (success) — the latest target is what the device will see,
    so the older caller's intent has been honored. Non-coalescable
    ops (connect, initialize, get_status, disconnect, etc.) keep
    going through `submit` and run FIFO.
    """

    def __init__(self) -> None:
        self._device_queues: Dict[int, queue.SimpleQueue] = {}
        # device_id -> {dedup_key -> latest _DeviceJob}; bounded at one
        # entry per (device, dedup_key) by submit_write supersession.
        self._pending_writes: Dict[int, Dict[str, _DeviceJob]] = {}
        # Guards _pending_writes only. Held briefly; never across run().
        self._lock: threading.Lock = threading.Lock()
        self._thread_pool: ThreadPoolExecutor = None
        # device_id -> monotonic start time of the job its worker is currently running, or None when
        # idle. Written only by that worker; read locklessly by submit() (a single dict-slot access
        # is atomic under the GIL). A wedged run() never returns, so this stays set, which is the
        # signal submit() uses to fail fast.
        self._job_running_since: Dict[int, Optional[float]] = {}
        # device_id -> the _job_running_since value last logged as wedged, so each wedge episode is
        # reported at most once.
        self._wedge_logged_for: Dict[int, Optional[float]] = {}

    def set_number_of_devices(self, number_of_devices: int) -> None:
        if number_of_devices < 1:
            return  # don't set any workers if there are no devices
        self._thread_pool = ThreadPoolExecutor(max_workers=number_of_devices)
        for dev_id in range(1, number_of_devices + 1):
            self._device_queues[dev_id] = queue.SimpleQueue()
            self._pending_writes[dev_id] = {}
            self._job_running_since[dev_id] = None
            self._wedge_logged_for[dev_id] = None
            self._thread_pool.submit(_queue_worker, self, dev_id)

    def submit(self, device_id: int, fn: Callable, **kwargs) -> Future:
        future = Future()
        if self._reject_if_wedged(device_id, future):
            return future
        device_job = _DeviceJob(future, fn, **kwargs)
        self._device_queues[device_id].put(device_job)
        return future

    def _reject_if_wedged(self, device_id: int, future: Future) -> bool:
        """Fail ``future`` immediately (and return True) when device ``device_id``'s worker is still
        stuck on a job past ``WEDGE_THRESHOLD_SECS``. A hung USB/HID call cannot be interrupted from
        another thread, so rather than letting requests pile up behind it (and making each wait the
        full timeout), we reject them fast until the worker recovers. Each wedge episode logs once.
        """
        since = self._job_running_since.get(device_id)
        if since is None:
            return False
        stuck_for = time.monotonic() - since
        if stuck_for <= WEDGE_THRESHOLD_SECS:
            return False
        if self._wedge_logged_for.get(device_id) != since:
            self._wedge_logged_for[device_id] = since
            log.warning(
                f"Device #{device_id} appears wedged: its worker has been blocked on one command "
                f"for {stuck_for:.0f}s (likely an unresponsive USB device). Failing new requests "
                f"for it until it recovers."
            )
        future.set_exception(
            concurrent.futures.TimeoutError(
                f"device #{device_id} unresponsive (worker wedged {stuck_for:.0f}s)"
            )
        )
        return True

    def submit_write(
        self,
        device_id: int,
        dedup_key: str,
        fn: Callable,
        **kwargs,
    ) -> Future:
        """Coalescable submission: a newer write for the same
        `(device_id, dedup_key)` supersedes any earlier waiting one.
        The superseded `Future` resolves with `None` (success) since
        the latest target replaces it before any device I/O.
        """
        future = Future()
        if self._reject_if_wedged(device_id, future):
            return future
        new_job = _DeviceJob(future, fn, dedup_key=dedup_key, **kwargs)
        superseded: Optional[_DeviceJob] = None
        enqueue_marker = False
        with self._lock:
            pending = self._pending_writes.setdefault(device_id, {})
            superseded = pending.get(dedup_key)
            pending[dedup_key] = new_job
            # First submission for this dedup_key gets a marker on the
            # queue; subsequent submissions update pending in place so
            # the queue cannot grow past one marker per key per device.
            enqueue_marker = superseded is None
        if superseded is not None:
            try:
                superseded.future.set_result(None)
            except concurrent.futures.InvalidStateError:
                # Future already finished (cancelled or set elsewhere);
                # nothing useful we can do here.
                pass
        if enqueue_marker:
            self._device_queues[device_id].put(new_job)
        return future

    def device_queue_empty(self, device_id: int) -> bool:
        with self._lock:
            if self._pending_writes.get(device_id):
                return False
        return self._device_queues[device_id].empty()

    def shutdown(self) -> None:
        for channel in self._device_queues.values():
            channel.put(None)  # our logic to end queue_worker loops
        if self._thread_pool is not None:
            # blocks until all threads are no longer processing work
            self._thread_pool.shutdown()
        self._device_queues.clear()
        with self._lock:
            self._pending_writes.clear()


#####################################################################
# Service
#####################################################################


def get_liquidctl_version() -> str:
    try:
        return importlib.metadata.version("liquidctl")
    except importlib.metadata.PackageNotFoundError:
        return getattr(liquidctl, "__version__", "unknown")


class _StatusCacheHealth:
    """Tracks whether a device's cached status is still fit to serve in place of a fresh read.

    Serving the cache is correct for a device that is merely slow: every read blows the short
    future timeout while the background refresh keeps the cache current behind it. It is wrong for
    a device that has stopped answering, where the cache freezes at its last good value and the
    daemon never learns the device is dead.

    Age alone cannot separate the two. The background refresh is only submitted when the device's
    queue is empty, so a device taking frequent writes (an LCD Kraken) can age its cache without
    ever having failed a read, and an age-only rule would drive it into failsafe for nothing. Both
    conditions together can: old *and* demonstrably failing its refreshes.
    """

    __slots__ = ("cached_at", "refresh_failed", "stale_logged")

    def __init__(self, cached_at: float) -> None:
        self.cached_at: float = cached_at
        self.refresh_failed: bool = False
        self.stale_logged: bool = False


class DeviceService:
    """
    The Service which keeps track of devices and handles all communication
    """

    def __init__(self) -> None:
        """
        To enable synchronous & parallel device communication, we use our own DeviceExecutor
        """
        self.devices: Dict[int, BaseDriver] = {}
        # this can be used to set specific flags like legacy/type/special
        #  from settings in coolercontrold
        self.device_infos: Dict[int, Any] = {}
        self.device_executor: DeviceExecutor = DeviceExecutor()
        self.device_status_cache: Dict[int, Statuses] = {}
        # Parallel to device_status_cache: when each entry was written and whether the device has
        # since failed a background refresh. See `_StatusCacheHealth`.
        self.device_status_health: Dict[int, _StatusCacheHealth] = {}
        self.liquidctl_version: str = get_liquidctl_version()
        # Guards shutdown(): the /quit handler and the signal handler can both call it, so the
        # actual device teardown must run at most once. Only the check-and-set is under the lock.
        self._shutdown_lock: threading.Lock = threading.Lock()
        self._shutdown_started: bool = False

    ###########################################################################
    # Device Startup

    def get_devices(self) -> List[Device]:
        log.debug("Getting device list")
        # if we've already searched for devices, don't do so again, just retrieve device info
        if self.devices:
            devices = [
                Device(
                    id=index_id,
                    description=lc_device.description,
                    device_type=type(lc_device).__name__,
                    serial_number=lc_device.serial_number,
                    properties=self._get_device_properties(lc_device),
                    liquidctl_version=self.liquidctl_version,
                    hid_address=(str(lc_device.address) if lc_device.address else None),
                    hwmon_address=(
                        str(lc_device._hwmon.path)
                        if hasattr(lc_device, "_hwmon")
                        and lc_device._hwmon is not None
                        and hasattr(lc_device._hwmon, "path")
                        else None
                    ),
                )
                for index_id, lc_device in self.devices.items()
            ]
            return devices
        try:  # otherwise find devices
            log.info("Searching for liquidctl devices...")
            log.debug("liquidctl.find_liquidctl_devices()")
            devices: List[Device] = []
            found_devices = list(liquidctl.find_liquidctl_devices())
            # sets the number of threads to be used per device:
            self.device_executor.set_number_of_devices(len(found_devices))
            for index, lc_device in enumerate(found_devices):
                index_id = index + 1
                # connect to the device before adding it to the list as "ready"
                self._connect_device(index_id, lc_device)
                self.devices[index_id] = lc_device
                description: str = getattr(lc_device, "description", "")
                serial_number: Optional[str] = getattr(
                    lc_device, "_serial_number", None
                )  # Aquacomputer devices read their serial number
                if not serial_number:
                    try:
                        serial_number = getattr(lc_device, "serial_number", None)
                    except ValueError:
                        log.warning(
                            f"No serial number info found for LC #{index_id} {description}"
                        )
                hwmon = getattr(lc_device, "_hwmon", None)
                devices.append(
                    Device(
                        id=index_id,
                        description=description,
                        device_type=type(lc_device).__name__,
                        serial_number=serial_number,
                        properties=self._get_device_properties(lc_device),
                        liquidctl_version=self.liquidctl_version,
                        hid_address=(
                            str(lc_device.address) if lc_device.address else None
                        ),
                        hwmon_address=str(hwmon.path) if hwmon else None,
                    )
                )
            device_names = [device.description for device in devices]
            log.info(f"liquidctl devices found: {device_names}")
            return devices
        except ValueError as ve:  # ValueError can happen when no devices were found
            log.debug(f"ValueError when trying to find all devices: {ve}")
            log.info("No Liquidctl devices detected")
            return []
        except BaseException as e:
            self.devices.clear()  # in case a single device has an issue and gets hung up
            raise e
        # all other exceptions are handled on startup with retry

    def scan_devices(self) -> List[Device]:
        """Perform a fresh device scan without modifying the cached device state.

        This is used by the daemon's device change detection to compare currently
        connected devices against the initial baseline.
        """
        log.debug("Scanning for currently connected liquidctl devices")
        try:
            found_devices = list(liquidctl.find_liquidctl_devices())
            devices: List[Device] = []
            for index, lc_device in enumerate(found_devices):
                index_id = index + 1
                description: str = getattr(lc_device, "description", "")
                serial_number: Optional[str] = getattr(lc_device, "serial_number", None)
                devices.append(
                    Device(
                        id=index_id,
                        description=description,
                        device_type=type(lc_device).__name__,
                        serial_number=serial_number,
                        properties=self._get_device_properties(lc_device),
                        liquidctl_version=self.liquidctl_version,
                        hid_address=(
                            str(lc_device.address) if lc_device.address else None
                        ),
                        hwmon_address=None,
                    )
                )
            log.debug(
                f"Scan found {len(devices)} liquidctl device(s): "
                f"{[d.description for d in devices]}"
            )
            return devices
        # A scan that blows up is not an empty bus, and the caller cannot tell the two apart from
        # an empty list. It used to get one, which made the daemon report every known device as
        # removed and fire a desktop notification telling the user to restart, for hardware that
        # never left. Raising lets the daemon skip the comparison instead.
        except ValueError as ve:
            log.debug(f"ValueError when scanning for devices: {ve}")
            raise LiquidctlException(f"Device scan failed: {ve}") from ve
        except Exception as e:
            log.warning(f"Error scanning for liquidctl devices: {e}")
            raise LiquidctlException(f"Device scan failed: {e}") from e

    @staticmethod
    def _get_device_properties(lc_device: BaseDriver) -> DeviceProperties:
        """
        Get device instance attributes to determine the specific configuration for a given device.
        """
        speed_channels: List[str] = []
        color_channels: List[str] = []
        supports_cooling: Optional[bool] = None
        supports_cooling_profiles: Optional[bool] = None
        supports_lighting: Optional[bool] = None
        led_count: Optional[int] = None
        lcd_resolution: Optional[Tuple[int, int]] = None
        if isinstance(lc_device, (SmartDevice2, SmartDevice)):
            speed_channel_dict = getattr(lc_device, "_speed_channels", {})
            speed_channels = list(speed_channel_dict.keys())
            color_channel_dict = getattr(lc_device, "_color_channels", {})
            color_channels = list(color_channel_dict.keys())
        elif ControlHub is not None and isinstance(lc_device, ControlHub):
            speed_channel_dict = getattr(lc_device, "_speed_channels", {})
            speed_channels = list(speed_channel_dict.keys())
            color_channel_dict = getattr(lc_device, "_color_channels", {})
            color_channels = list(color_channel_dict.keys())
        elif H1V2 is not None and isinstance(lc_device, H1V2):
            speed_channel_dict = getattr(lc_device, "_speed_channels", {})
            speed_channels = list(speed_channel_dict.keys())
            color_channel_dict = getattr(lc_device, "_color_channels", {})
            color_channels = list(color_channel_dict.keys())
        elif isinstance(lc_device, Kraken2):
            supports_cooling = getattr(lc_device, "supports_cooling", None)
            # this property in particular requires connect() to already have been called:
            supports_cooling_profiles = getattr(
                lc_device, "supports_cooling_profiles", None
            )
            supports_lighting = getattr(lc_device, "supports_lighting", None)
        elif isinstance(lc_device, KrakenZ3):
            color_channel_dict = getattr(lc_device, "_color_channels", {})
            color_channels = list(color_channel_dict.keys())
            lcd_resolution = getattr(lc_device, "lcd_resolution", None)
        elif Aquacomputer is not None and isinstance(lc_device, Aquacomputer):
            device_info_dict = getattr(lc_device, "_device_info", {})
            controllable_pump_and_fans = device_info_dict.get("fan_ctrl", {})
            speed_channels = list(controllable_pump_and_fans.keys())
        elif CommanderCore is not None and isinstance(lc_device, CommanderCore):
            if _ := getattr(lc_device, "_has_pump", False):
                speed_channels = ["pump"]
        elif isinstance(lc_device, CommanderPro):
            if fan_names := getattr(lc_device, "_fan_names", []):
                speed_channels = fan_names
            if led_names := getattr(lc_device, "_led_names", []):
                color_channels = led_names
        elif isinstance(lc_device, CorsairHidPsu):
            if hasattr(liquidctl.driver.corsair_hid_psu, "_MIN_FAN_DUTY"):
                # set the minimum fan speed to 15%, tested working -
                # lower produces a different response from the hardware with poor results.
                liquidctl.driver.corsair_hid_psu._MIN_FAN_DUTY = 15
        elif isinstance(lc_device, HydroPlatinum):
            if fan_names := getattr(lc_device, "_fan_names", []):
                speed_channels = fan_names
            if led_count_found := getattr(lc_device, "_led_count", 0):
                color_channels = ["led"]
                led_count = led_count_found
        elif HydroPro is not None and isinstance(lc_device, HydroPro):
            if fan_count := getattr(lc_device, "_fan_count", 0):
                speed_channels = [
                    f"fan{fan_number + 1}" for fan_number in range(fan_count)
                ]
        return DeviceProperties(
            speed_channels=speed_channels,
            color_channels=color_channels,
            supports_cooling=supports_cooling,
            supports_cooling_profiles=supports_cooling_profiles,
            supports_lighting=supports_lighting,
            led_count=led_count,
            lcd_resolution=lcd_resolution,
        )

    def _connect_device(self, device_id: int, lc_device: BaseDriver) -> None:
        log.info(f"Connecting to LC #{device_id} {lc_device.__class__.__name__}")
        log.debug(f"LC #{device_id} {lc_device.__class__.__name__}.connect() ")
        # currently only smbus devices have options for connect()
        connect_job = self.device_executor.submit(device_id, lc_device.connect)
        try:
            connect_job.result(timeout=DEVICE_TIMEOUT_SECS)
        except RuntimeError as err:
            if "already open" in str(err):
                log.warning(f"{lc_device.description} already connected")
            else:
                raise LiquidctlException(
                    "Unexpected Device Communication Error"
                ) from err

    ###########################################################################
    # Device Management

    def set_device_as_legacy690(self, device_id: int) -> Device:
        """
        Modern and Legacy Asetek 690Lc devices have the same device ID.
        We ask the user to verify which device is connected
        so that we can correctly communicate with the device.
        """
        if self.devices.get(device_id) is None:
            raise LiqctldException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        lc_device = self.devices[device_id]
        if isinstance(lc_device, Legacy690Lc):
            log.warning(f"Device #{device_id} is already set as a Legacy690Lc device")
            return Device(
                id=device_id,
                description=lc_device.description,
                device_type=type(lc_device).__name__,
                serial_number=lc_device.serial_number,
                properties=DeviceProperties(),
                liquidctl_version=self.liquidctl_version,
                hid_address=(str(lc_device.address) if lc_device.address else None),
                hwmon_address=None,
            )
        elif not isinstance(lc_device, Modern690Lc):
            message = f"Device #{device_id} is not applicable to be downgraded to a Legacy690Lc"
            log.warning(message)
            raise LiqctldException(HTTPStatus.EXPECTATION_FAILED, message)
        log.info(f"Setting device #{device_id} as legacy690")
        self._disconnect_device(device_id, lc_device)
        log.debug("Legacy690Lc.find_liquidctl_devices()")
        legacy_job = self.device_executor.submit(
            device_id, Legacy690Lc.find_supported_devices
        )
        asetek690s = list(legacy_job.result(timeout=DEVICE_TIMEOUT_SECS))
        if not asetek690s:
            log.error("Could not find any Legacy690Lc devices. This shouldn't happen")
            raise LiquidctlException("Could not find any Legacy690Lc devices.")
        elif len(asetek690s) > 1:
            # if there are multiple options, we need to find the correlating device
            current_asetek690_ids = [
                asetek690_id
                for asetek690_id, device in self.devices.items()
                if isinstance(device, (Modern690Lc, Legacy690Lc))
            ]
            device_index: int = 0
            for asetek690_id in current_asetek690_ids:
                if asetek690_id == device_id:
                    break
                else:
                    device_index += 1
            self.devices[device_id] = asetek690s[device_index]
            lc_device = self.devices[device_id]
        else:
            self.devices[device_id] = asetek690s[0]
            lc_device = self.devices[device_id]

        self._connect_device(device_id, lc_device)
        description: str = getattr(lc_device, "description", "")
        serial_number: Optional[str] = None
        try:
            serial_number = getattr(lc_device, "serial_number", None)
        except ValueError:
            log.warning(
                f"No serial number info found for LC #{device_id} {description}"
            )
        return Device(
            id=device_id,
            description=description,
            device_type=type(lc_device).__name__,
            serial_number=serial_number,
            properties=DeviceProperties(),
            liquidctl_version=self.liquidctl_version,
            hid_address=(str(lc_device.address) if lc_device.address else None),
            hwmon_address=None,
        )

    def initialize_device(self, device_id: int, init_args: Dict[str, str]) -> Statuses:
        if self.devices.get(device_id) is None:
            raise LiqctldException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        log.debug(
            f"Initializing Liquidctl device #{device_id} with arguments: {init_args}"
        )
        try:
            lc_device = self.devices[device_id]
            if AuraLed is not None and isinstance(lc_device, AuraLed):
                log.debug("Skipping AuraLed device initialization, not needed.")
                # also has negative side effects of clearing previously set lighting settings
                return []
            log.debug(
                f"LC #{device_id} {lc_device.__class__.__name__}.initialize({init_args}) "
            )
            init_job = self.device_executor.submit(
                device_id, lc_device.initialize, **init_args
            )
            lc_init_status: Union[
                List[Tuple[str, Union[str, int, float], str]],
                None,  # a few devices can return None on initialization like the Legacy690Lc
            ] = init_job.result(timeout=DEVICE_TIMEOUT_SECS)
            log.debug(
                f"LC #{device_id} {lc_device.__class__.__name__}initialize() "
                f"RESPONSE: {lc_init_status}"
            )
            statuses = self._stringify_status(lc_init_status)
            gif_supported = lcd_gif_supported(lc_device)
            if gif_supported is not None:
                statuses.append(("LCD Gif Support", str(gif_supported).lower(), ""))
            return statuses
        except BaseException as os_exc:
            # OSError can happen when a device was found and there's a permissions error
            # OSError: read error sometimes happens when the OS/Device isn't ready.
            # ValueError: not open, can happen when device is no longer _connected()
            # To not output too many logs, and since CC auto-retries this:
            if log.getLogger().isEnabledFor(logging.DEBUG):
                log.error(f"Device Initialization Error - {traceback.format_exc()}")
            raise LiquidctlException(
                f"Unexpected Device Communication Error - {traceback.format_exc()}"
            ) from os_exc

    @staticmethod
    def _stringify_status(
        statuses: Union[List[Tuple[str, Union[str, int, float], str]], None],
    ) -> Statuses:
        return (
            [(str(status[0]), str(status[1]), str(status[2])) for status in statuses]
            if statuses is not None
            else []
        )

    def force_direct_access(self, device_id: int) -> None:
        """
        Force a liquidctl device to use direct access mode.
        This should be called before initializing the device.

        This method forces the specified device to switch to direct access mode,
        bypassing any kernel drivers. This is useful when CoolerControl's regular communication
        and kernel drivers interfere with liquidctl's ability to communicate with the device.

        Args:
            device_id: The unique identifier of the device to force into direct access mode.

        Raises:
            LiqctldException: If the device with the specified ID is not found.
            LiquidctlException: If there is an unexpected error during device communication.
        """
        if self.devices.get(device_id) is None:
            raise LiqctldException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        log.debug(f"Forcing Liquidctl device #{device_id} to use direct access.")
        try:
            lc_device = self.devices[device_id]
            # Removing the hwmon driver info object also removes the warning logs that liquidctl
            # would then output with every status update when using `direct_access=True`. This is
            # preferred for our use case and ensures that liquidctl has no chance of using the
            # hwmon driver.
            lc_device._hwmon = None
        except BaseException as err:
            if log.getLogger().isEnabledFor(logging.DEBUG):
                log.error(
                    f"Liquidctl Error forcing direct access for device "
                    f"#{device_id} - {traceback.format_exc()}"
                )
            raise LiquidctlException(
                f"Unexpected Device communication error: {err}"
            ) from err

    def get_status(self, device_id: int) -> Statuses:
        if self.devices.get(device_id) is None:
            raise LiqctldException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        log.debug(f"Getting status for device: {device_id}")
        try:
            return self._get_current_or_cached_device_status(device_id)
        except BaseException as err:
            log.debug(
                f"Liquidctl Error getting status for device "
                f"#{device_id} - {traceback.format_exc()}"
            )
            raise LiquidctlException(
                f"Unexpected Device communication error: {err}"
            ) from err

    def _get_current_or_cached_device_status(self, device_id: int) -> Statuses:
        lc_device = self.devices[device_id]
        if AuraLed is not None and isinstance(lc_device, AuraLed):
            log.debug("Skipping AuraLed device status, not needed.")
            return []
        log.debug(f"LC #{device_id} {lc_device.__class__.__name__}.get_status()")
        status_job = self.device_executor.submit(device_id, lc_device.get_status)
        try:
            status: List[Tuple[str, Union[str, int, float], str]] = status_job.result(
                timeout=DEVICE_READ_STATUS_TIMEOUT_SECS
            )
            log.debug(
                f"LC #{device_id} {lc_device.__class__.__name__}.get_status() RESPONSE: {status}"
            )
            serialized_status = self._stringify_status(status)
            self._cache_status(device_id, serialized_status)
            return serialized_status
        except concurrent.futures.TimeoutError as te:
            log.debug(
                f"Timeout occurred while trying to get device status for LC #{device_id}. "
                f"Reusing last status if possible."
            )
            cached_status = self.device_status_cache.get(device_id)
            servable = cached_status is not None and self._cached_status_is_servable(
                device_id
            )
            if self.device_executor.device_queue_empty(
                device_id
            ):  # if emtpy this was likely a device timeout with a single job
                log.debug("Running long-lasting async get_status() call")
                async_status_job = self.device_executor.submit(
                    device_id, self._long_async_status_request, dev_id=device_id
                )
                if servable:
                    # return the currently cached status immediately and
                    #  let the async request above refresh the cache in the background
                    return cached_status
                if cached_status is not None:
                    # The cache has gone stale on a device that is failing its refreshes. The
                    # refresh submitted above is left running so the device can still recover, but
                    # answering with a frozen value would hide the failure completely: the daemon
                    # needs a missing status before its failsafe can latch.
                    self._log_stale_status_once(device_id)
                    raise te
                # else rerun the status request with a very long timeout
                #  and wait for the output so that the cache fills up at least once
                try:
                    return async_status_job.result(timeout=DEVICE_TIMEOUT_SECS)
                except concurrent.futures.TimeoutError as te:
                    log.error(
                        f"Unknown issue with device communication "
                        f"and no Status Cache yet filled for device LC #{device_id}"
                    )
                    raise te
                except BaseException as exc:
                    log.debug(
                        f"Unexpected error during async status request "
                        f"for device LC #{device_id}: {exc}"
                    )
                    raise
                finally:
                    async_status_job.cancel()
            # otherwise, this was a future timeout with a job still running in the queue
            if servable:
                return cached_status
            if cached_status is None:
                log.error(f"No Status Cache yet filled for device LC #{device_id}")
            else:
                self._log_stale_status_once(device_id)
            raise te
        except BaseException as exc:
            log.debug(
                f"Unexpected error getting status for device LC #{device_id}: {exc}"
            )
            raise
        finally:
            status_job.cancel()

    def _long_async_status_request(self, dev_id: int) -> Statuses:
        """
        This function is used to get the status in a non-blocking way for devices
        that have extreme latency.
        """
        lc_device = self.devices[dev_id]
        log.debug(f"LC #{dev_id} {lc_device.__class__.__name__}.get_status()")
        try:
            status = lc_device.get_status()
        except BaseException as exc:
            log.debug(
                f"LC #{dev_id} {lc_device.__class__.__name__}.get_status() "
                f"failed: {exc}"
            )
            # This is the only place the device demonstrates that it is not answering, as opposed
            # to merely being slower than the short read timeout. It is half of what lets a stale
            # cache be refused without punishing a device that is simply slow.
            health = self.device_status_health.get(dev_id)
            if health is not None:
                health.refresh_failed = True
            raise
        log.debug(
            f"LC #{dev_id} {lc_device.__class__.__name__}.get_status() RESPONSE: {status}"
        )
        serialized_status = self._stringify_status(status)
        self._cache_status(dev_id, serialized_status)
        return serialized_status

    def _cache_status(self, device_id: int, serialized_status: Statuses) -> None:
        """Records a fresh status, ending any stale-serving episode for this device."""
        previous = self.device_status_health.get(device_id)
        if previous is not None and previous.stale_logged:
            log.warning(f"Device LC #{device_id} is answering status requests again.")
        self.device_status_cache[device_id] = serialized_status
        self.device_status_health[device_id] = _StatusCacheHealth(time.monotonic())

    def _cached_status_is_servable(self, device_id: int) -> bool:
        """Whether the cache may still stand in for a fresh read. See `_StatusCacheHealth`."""
        health = self.device_status_health.get(device_id)
        if health is None:
            return False
        if health.refresh_failed is False:
            return True
        return time.monotonic() - health.cached_at <= STALE_STATUS_BUDGET_SECS

    def _log_stale_status_once(self, device_id: int) -> None:
        """One line per stale episode, not one per poll."""
        health = self.device_status_health.get(device_id)
        if health is None:
            return
        if health.stale_logged:
            return
        health.stale_logged = True
        log.warning(
            f"Device LC #{device_id} has not answered a status request in over "
            f"{STALE_STATUS_BUDGET_SECS}s. Reporting it as unavailable so the daemon "
            f"can apply its failsafe."
        )

    def set_fixed_speed(
        self, device_id: int, speed_kwargs: Dict[str, Union[str, int]]
    ) -> None:
        if self.devices.get(device_id) is None:
            raise LiqctldException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        log.debug(
            f"Setting fixes speed for device: {device_id} with args: {speed_kwargs}"
        )
        try:
            lc_device = self.devices[device_id]
            log.debug(
                f"LC #{device_id} {lc_device.__class__.__name__}.set_fixed_speed({speed_kwargs}) "
            )
            # `set_fixed_speed` and `set_speed_profile` are mutually
            # exclusive on the same channel, so they share one dedup
            # namespace: the latest of either wins.
            dedup_key = f"speed:{speed_kwargs['channel']}"
            status_job = self.device_executor.submit_write(
                device_id, dedup_key, lc_device.set_fixed_speed, **speed_kwargs
            )
            # maximum timeout for setting data on the device:
            status_job.result(timeout=DEVICE_TIMEOUT_SECS)
        except BaseException as err:
            if log.getLogger().isEnabledFor(logging.DEBUG):
                log.error(
                    f"Liquidctl Error setting fixed speed: "
                    f"{speed_kwargs} - {traceback.format_exc()}"
                )
            raise LiquidctlException(
                f"Unexpected Device communication error: {err}"
            ) from err

    def set_speed_profile(self, device_id: int, speed_kwargs: Dict[str, Any]) -> None:
        if self.devices.get(device_id) is None:
            raise LiqctldException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        log.debug(
            f"Setting speed profile for device: {device_id} with args: {speed_kwargs}"
        )
        try:
            lc_device = self.devices[device_id]
            log.debug(
                f"LC #{device_id} {lc_device.__class__.__name__}.set_speed_profile({speed_kwargs}) "
            )
            # Same dedup namespace as `set_fixed_speed` since both
            # control the channel's speed and supersede each other.
            dedup_key = f"speed:{speed_kwargs['channel']}"
            status_job = self.device_executor.submit_write(
                device_id, dedup_key, lc_device.set_speed_profile, **speed_kwargs
            )
            status_job.result(timeout=DEVICE_TIMEOUT_SECS)
        except BaseException as err:
            if log.getLogger().isEnabledFor(logging.DEBUG):
                log.error(
                    f"Liquidctl Error setting color with {speed_kwargs} - {traceback.format_exc()}"
                )
            raise LiquidctlException(
                f"Unexpected Device communication error: {err}"
            ) from err

    def set_control_mode(
        self, device_id: int, control_mode_kwargs: Dict[str, str]
    ) -> None:
        try:
            if self.devices.get(device_id) is None:
                raise LiqctldException(
                    HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
                )
            lc_device = self.devices[device_id]
            if LianLiUni is None or not isinstance(lc_device, LianLiUni):
                raise LiqctldException(
                    HTTPStatus.BAD_REQUEST,
                    f"Device id:{device_id} does not support control mode",
                )
            log.debug(
                f"Setting control mode for device: {device_id} with args: {control_mode_kwargs}"
            )
            log.debug(
                f"LC #{device_id} {lc_device.__class__.__name__}."
                f"set_fan_control_mode({control_mode_kwargs}) "
            )
            status_job = self.device_executor.submit(
                device_id, lc_device.set_fan_control_mode, **control_mode_kwargs
            )
            # maximum timeout for setting data on the device:
            status_job.result(timeout=DEVICE_TIMEOUT_SECS)
        except LiqctldException as err:
            raise err
        except BaseException as err:
            if log.getLogger().isEnabledFor(logging.DEBUG):
                log.error(
                    f"Liquidctl Error setting control mode: "
                    f"{control_mode_kwargs} - {traceback.format_exc()}"
                )
            raise LiquidctlException(
                f"Unexpected Device communication error: {err}"
            ) from err

    def set_color(self, device_id: int, color_kwargs: Dict[str, Any]) -> None:
        if self.devices.get(device_id) is None:
            raise LiqctldException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        log.debug(f"Setting color for device: {device_id} with args: {color_kwargs}")
        try:
            lc_device = self.devices[device_id]
            log.debug(
                f"LC #{device_id} {lc_device.__class__.__name__}.set_color({color_kwargs}) "
            )
            dedup_key = f"color:{color_kwargs['channel']}"
            status_job = self.device_executor.submit_write(
                device_id, dedup_key, lc_device.set_color, **color_kwargs
            )
            status_job.result(timeout=DEVICE_TIMEOUT_SECS)
        except BaseException as err:
            # color changes are considered non-critical
            if log.getLogger().isEnabledFor(logging.DEBUG):
                log.warning(
                    f"Liquidctl Error setting color with {color_kwargs} - {traceback.format_exc()}"
                )
            raise LiquidctlException(
                f"Unexpected Device communication error: {err}"
            ) from err

    def set_screen(self, device_id: int, screen_kwargs: Dict[str, str]) -> None:
        if self.devices.get(device_id) is None:
            raise LiqctldException(
                HTTPStatus.NOT_FOUND, f"Device with id:{device_id} not found"
            )
        log.debug(f"Setting screen for device: {device_id} with args: {screen_kwargs}")
        try:
            lc_device = self.devices[device_id]
            log.debug(
                f"LC #{device_id} {lc_device.__class__.__name__}.set_screen({screen_kwargs}) "
            )
            start_screen_update = time.time()
            # `mode` is part of the dedup key because LCD modes carry
            # different `value` payloads (liquid stat, static image
            # path, gif path); a queued static image must not be
            # clobbered by a later liquid-mode command and vice versa.
            # Coalescing only kicks in for repeats of the same mode,
            # which is the static-image upload buildup case.
            dedup_key = f"screen:{screen_kwargs['channel']}:{screen_kwargs['mode']}"
            screen_job = self.device_executor.submit_write(
                device_id, dedup_key, lc_device.set_screen, **screen_kwargs
            )
            # after setting the screen, sometimes an immediate status request comes back with 0,
            #  so we wait a small amount
            wait_job = self.device_executor.submit(device_id, lambda: time.sleep(0.01))
            screen_job.result(timeout=DEVICE_TIMEOUT_SECS)
            wait_job.result(timeout=DEVICE_TIMEOUT_SECS)
            log.debug(
                f"Time taken to update the screen for device: {device_id}, "
                f"including waiting on other commands: {time.time() - start_screen_update}"
            )
        except BaseException as err:
            # screen changes are considered non-critical
            if log.getLogger().isEnabledFor(logging.DEBUG):
                log.warning(
                    f"Liquidctl Error setting screen with "
                    f"{screen_kwargs} - {traceback.format_exc()}"
                )
            raise LiquidctlException(
                f"Unexpected Device communication error: {err}"
            ) from err

    ###########################################################################
    # Device Shutdown

    def disconnect_all(self) -> None:
        for device_id, lc_device in self.devices.items():
            self._disconnect_device(device_id, lc_device)
        self.devices.clear()

    def _disconnect_device(self, device_id: int, lc_device: BaseDriver) -> None:
        log.debug(f"LC #{device_id} {lc_device.__class__.__name__}.disconnect() ")
        disconnect_job = self.device_executor.submit(device_id, lc_device.disconnect)
        disconnect_job.result(timeout=DEVICE_TIMEOUT_SECS)

    def shutdown(self) -> None:
        """
        Shutdown all devices and their workers.
        This includes calling initialize() to attempt to reset devices to their default state,
        then calling disconnect() to disconnect all devices.

        Idempotent and thread-safe: the /quit handler and the signal handler may both call this,
        but the device teardown runs at most once. The lock guards only the check-and-set so a
        second caller returns immediately instead of blocking behind the first caller's I/O.
        """
        with self._shutdown_lock:
            if self._shutdown_started:
                return
            self._shutdown_started = True
        for device_id, lc_device in self.devices.items():
            if isinstance(
                lc_device, CorsairHidPsu
            ):  # attempt to reset fan control back to hardware control
                log.debug(
                    f"LC #{device_id} {lc_device.__class__.__name__}.initialize() "
                )
                init_job = self.device_executor.submit(device_id, lc_device.initialize)
                init_job.result(timeout=DEVICE_TIMEOUT_SECS)
        self.disconnect_all()
        self.device_executor.shutdown()


#####################################################################
# HTTP UDS Server
#####################################################################


class HTTPHandler(BaseHTTPRequestHandler):
    server_version = "BasicHTTP/1.0"
    protocol_version = "HTTP/1.1"

    def __init__(self, request, client_address, server):
        self.device_service: DeviceService = server.device_service
        super().__init__(request, client_address, server)

    # get("/handshake")
    def handshake(self):
        log.debug("Exchanging handshake")
        self._send(HTTPStatus.OK, Handshake(shake=True).to_json())

    # post("/quit")
    def quit_server(self):
        log.info("Quit command received. Shutting down.")
        # shutdown device service first so that all jobs finish processing before
        # shutting down the server. The daemon should no longer be sending device requests
        # after making this request.
        self.device_service.shutdown()
        self._send(HTTPStatus.OK, json.dumps({}))
        self.server.shutting_down = True
        self.server.shutdown()
        # this also handles socket close:
        self.server.server_close()

    # get("/devices")
    def get_devices(self):
        devices: List[Device] = self.device_service.get_devices()
        self._send(
            HTTPStatus.OK, json.dumps({"devices": [d.to_dict() for d in devices]})
        )

    # get("/devices/scan")
    def scan_devices(self):
        devices: List[Device] = self.device_service.scan_devices()
        self._send(
            HTTPStatus.OK, json.dumps({"devices": [d.to_dict() for d in devices]})
        )

    # put("/devices/{device_id}/legacy690")
    def set_device_as_legacy690(self, device_id: int):
        device: Device = self.device_service.set_device_as_legacy690(device_id)
        self._send(HTTPStatus.OK, json.dumps(device.to_dict()))

    # put("/devices/{device_id}/direct-access")
    def force_direct_access(self, device_id: int):
        self.device_service.force_direct_access(device_id)
        self._send(HTTPStatus.OK, json.dumps({}))

    # post("/devices/{device_id}/initialize")
    def init_device(self, device_id: int, init_request: dict):
        init_args: dict = InitRequest.from_dict(init_request).to_dict_no_none()
        status_response: Statuses = self.device_service.initialize_device(
            device_id, init_args
        )
        self._send(HTTPStatus.OK, json.dumps({"status": status_response}))

    # get("/devices/{device_id}/status")
    def get_status(self, device_id: int):
        status_response: Statuses = self.device_service.get_status(device_id)
        self._send(HTTPStatus.OK, json.dumps({"status": status_response}))

    # put("/devices/{device_id}/speed/fixed")
    def set_fixed_speed(self, device_id: int, speed_request: dict):
        speed_kwargs = FixedSpeedRequest.from_dict(speed_request).to_dict_no_none()
        self.device_service.set_fixed_speed(device_id, speed_kwargs)
        # empty success response needed for systemd socket service to not error on 0 byte content
        self._send(HTTPStatus.OK, json.dumps({}))

    # put("/devices/{device_id}/speed/profile")
    def set_speed_profile(self, device_id: int, speed_request: dict):
        speed_kwargs = SpeedProfileRequest.from_dict(speed_request).to_dict_no_none()
        self.device_service.set_speed_profile(device_id, speed_kwargs)
        self._send(HTTPStatus.OK, json.dumps({}))

    # put("/devices/{device_id}/control-mode")
    def set_control_mode(self, device_id: int, control_mode_request: dict):
        control_mode_kwargs = ControlModeRequest.from_dict(
            control_mode_request
        ).to_dict_no_none()
        self.device_service.set_control_mode(device_id, control_mode_kwargs)
        # empty success response needed for systemd socket service to not error on 0 byte content
        self._send(HTTPStatus.OK, json.dumps({}))

    # put("/devices/{device_id}/color")
    def set_color(self, device_id: int, color_request: dict):
        color_kwargs = ColorRequest.from_dict(color_request).to_dict_no_none()
        self.device_service.set_color(device_id, color_kwargs)
        self._send(HTTPStatus.OK, json.dumps({}))

    # put("/devices/{device_id}/screen")
    def set_screen(self, device_id: int, screen_request: dict):
        # need None value for liquid mode
        screen_kwargs = ScreenRequest.from_dict(screen_request).to_dict()
        self.device_service.set_screen(device_id, screen_kwargs)
        self._send(HTTPStatus.OK, json.dumps({}))

    def _route_get_requests(self, path: List[str]):
        if not path:
            raise LiqctldException(HTTPStatus.BAD_REQUEST, "Invalid path")
        elif len(path) == 1 and path[0] == "handshake":
            # get("/handshake")
            self.handshake()
        elif len(path) == 2 and path[0] == "devices" and path[1] == "scan":
            # get("/devices/scan")
            self.scan_devices()
        elif len(path) == 1 and path[0] == "devices":
            # get("/devices")
            self.get_devices()
        elif len(path) == 3 and path[0] == "devices" and path[2] == "status":
            # get("/devices/{device_id}/status")
            device_id = self._try_cast_int(path[1])
            self.get_status(device_id)
        else:
            raise LiqctldException(HTTPStatus.NOT_FOUND, f"Path: {path} Not Found")

    def _route_post_requests(self, path: List[str], request_body: dict):
        if not path:
            raise LiqctldException(HTTPStatus.BAD_REQUEST, "Invalid path")
        elif len(path) == 1 and path[0] == "quit":
            # post("/quit")
            self.quit_server()
        elif len(path) == 3 and path[0] == "devices" and path[2] == "initialize":
            # post("/devices/{device_id}/initialize")
            device_id = self._try_cast_int(path[1])
            self.init_device(device_id, request_body)
        else:
            raise LiqctldException(HTTPStatus.NOT_FOUND, f"Path: {path} Not Found")

    def _route_put_requests(self, path: List[str], request_body: dict):
        if not path:
            raise LiqctldException(HTTPStatus.BAD_REQUEST, "Invalid path")
        elif (
            len(path) == 4
            and path[0] == "devices"
            and path[2] == "speed"
            and path[3] == "fixed"
        ):
            # put("/devices/{device_id}/speed/fixed")
            device_id = self._try_cast_int(path[1])
            self.set_fixed_speed(device_id, request_body)
        elif (
            len(path) == 4
            and path[0] == "devices"
            and path[2] == "speed"
            and path[3] == "profile"
        ):
            # put("/devices/{device_id}/speed/profile")
            device_id = self._try_cast_int(path[1])
            self.set_speed_profile(device_id, request_body)
        elif len(path) == 3 and path[0] == "devices" and path[2] == "color":
            # put("/devices/{device_id}/color")
            device_id = self._try_cast_int(path[1])
            self.set_color(device_id, request_body)
        elif len(path) == 3 and path[0] == "devices" and path[2] == "screen":
            # put("/devices/{device_id}/screen")
            device_id = self._try_cast_int(path[1])
            self.set_screen(device_id, request_body)
        elif len(path) == 3 and path[0] == "devices" and path[2] == "legacy690":
            # put("/devices/{device_id}/legacy690")
            device_id = self._try_cast_int(path[1])
            self.set_device_as_legacy690(device_id)
        elif len(path) == 3 and path[0] == "devices" and path[2] == "direct-access":
            # put("/devices/{device_id}/direct-access")
            device_id = self._try_cast_int(path[1])
            self.force_direct_access(device_id)
        elif len(path) == 3 and path[0] == "devices" and path[2] == "control-mode":
            # put("/devices/{device_id}/control-mode")
            device_id = self._try_cast_int(path[1])
            self.set_control_mode(device_id, request_body)
        else:
            raise LiqctldException(HTTPStatus.NOT_FOUND, f"Path: {path} Not Found")

    def _send(self, status: HTTPStatus, reply: str) -> None:
        # avoid exception in server.py address_string()
        self.client_address = ("",)
        self.send_response(status.value)
        reply_bytes = reply.encode("utf-8")
        self.send_header("Content-type", "application/json")
        self.send_header("Content-Length", str(len(reply_bytes)))
        self.send_header("Connection", "keep-alive")
        self.end_headers()
        self.wfile.write(reply_bytes)

    def _parse_path(self) -> List[str]:
        return [x for x in self.path.strip().split("/") if x]

    @staticmethod
    def _try_cast_int(value: str) -> int:
        try:
            return int(value)
        except ValueError:
            raise LiqctldException(
                HTTPStatus.BAD_REQUEST, f"Invalid path. Expected Integer: {value}"
            ) from None

    def log_message(self, format, *args):
        # server request logs are disabled by default and will use our logger
        if self.server.log_level > logging.DEBUG:
            return
        # super().log_message(format, *args)
        message = format % args
        log.debug(
            "%s - - [%s] %s\n"
            % (
                self.address_string(),
                self.log_date_time_string(),
                message.translate(self._control_char_table),
            )
        )

    def do_GET(self):
        try:
            self._route_get_requests(self._parse_path())
        except LiquidctlException as e:
            self._send(HTTPStatus.BAD_GATEWAY, str(e))
        except LiqctldException as e:
            self._send(e.code, e.message)
        except Exception:
            self._send(HTTPStatus.INTERNAL_SERVER_ERROR, str(traceback.format_exc()))

    def do_POST(self):
        size = int(self.headers.get("Content-Length", 0))
        raw_body = self.rfile.read(size)
        request_body: dict = json.loads(raw_body) if raw_body else {}
        try:
            self._route_post_requests(self._parse_path(), request_body)
        except LiquidctlException as e:
            self._send(HTTPStatus.BAD_GATEWAY, str(e))
        except LiqctldException as e:
            self._send(e.code, e.message)
        except Exception:
            self._send(HTTPStatus.INTERNAL_SERVER_ERROR, str(traceback.format_exc()))

    def do_PUT(self):
        size = int(self.headers.get("Content-Length", 0))
        raw_body = self.rfile.read(size)
        request_body: dict = json.loads(raw_body) if raw_body else {}
        try:
            self._route_put_requests(self._parse_path(), request_body)
        except LiquidctlException as e:
            self._send(HTTPStatus.BAD_GATEWAY, str(e))
        except LiqctldException as e:
            self._send(e.code, e.message)
        except Exception:
            self._send(HTTPStatus.INTERNAL_SERVER_ERROR, str(traceback.format_exc()))


def server_run(device_service: DeviceService) -> None:
    try:
        os.remove(SOCKET_ADDRESS)
    except OSError:
        pass
    server = ThreadingHTTPServer(SOCKET_ADDRESS, HTTPHandler, False)
    server.log_level = log.getLogger().getEffectiveLevel()
    server.device_service = device_service
    server.shutting_down = False
    server.socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.socket.bind(SOCKET_ADDRESS)
    # restrict access to the socket
    os.chmod(SOCKET_ADDRESS, 0o600)
    # creates a listener thread for each connection:
    queue_size = max(1, len(device_service.devices))
    log.info("liqctld initialized and listening for connections")
    server.socket.listen(queue_size)

    def finish_shutdown() -> None:
        # Best-effort hardware cleanup, run in a daemon thread so a worker wedged in an
        # uninterruptible USB read cannot delay the hard exit below. device_service.shutdown() is
        # idempotent, so it is safe alongside a concurrent /quit.
        try:
            server.device_service.shutdown()
        except BaseException as exc:
            sys.stderr.write(f"liqctld shutdown error: {exc}\n")
        server.shutting_down = True
        try:
            server.server_close()
        except BaseException:
            pass

    def shutdown_gracefully(_signum, _frame) -> None:
        # A repeated signal means the first attempt is still running (or is wedged): exit now, since
        # systemd is already counting down and no further cleanup can be trusted to finish.
        if getattr(server, "forced_exit", False):
            os._exit(0)
        server.forced_exit = True
        # Do the cleanup off the main thread and cap the whole shutdown at a hard deadline. os._exit
        # skips atexit and thread joins, so even a permanently wedged worker cannot hang the exit.
        finisher = threading.Thread(
            target=finish_shutdown, name="liqctld-shutdown", daemon=True
        )
        finisher.start()
        finisher.join(SHUTDOWN_DEADLINE_SECS)
        os._exit(0)

    signal.signal(signal.SIGINT, shutdown_gracefully)
    signal.signal(signal.SIGTERM, shutdown_gracefully)
    server.serve_forever()


#####################################################################
# Main
#####################################################################


def setup_logging() -> None:
    env_log_level: Optional[str] = os.getenv("CC_LOG")
    env_liquidctl_log_level: Optional[str] = os.getenv("LIQUIDCTL_LOG")
    # default & "INFO" levels:
    log_level = logging.INFO
    liquidctl_level = logging.WARNING
    if env_log_level:
        if env_log_level.lower() == "trace":
            log_level = logging.DEBUG
            liquidctl_level = logging.DEBUG
        elif env_log_level.lower() == "debug":
            log_level = logging.DEBUG
            # liquidctl outputs a lot of data on each poll, so we set it to one level higher:
            liquidctl_level = logging.INFO
        elif env_log_level.lower() == "warn":
            log_level = logging.WARNING
            liquidctl_level = logging.WARNING
        elif env_log_level.lower() == "error":
            log_level = logging.ERROR
            liquidctl_level = logging.ERROR
    # override liquidctl log level if set
    if env_liquidctl_log_level:
        if (
            env_liquidctl_log_level.lower() == "debug"
            or env_liquidctl_log_level.lower() == "trace"
        ):
            liquidctl_level = logging.DEBUG
        elif env_liquidctl_log_level.lower() == "info":
            liquidctl_level = logging.INFO
        elif env_liquidctl_log_level.lower() == "warn":
            liquidctl_level = logging.WARNING
        elif env_liquidctl_log_level.lower() == "error":
            liquidctl_level = logging.ERROR
        elif env_liquidctl_log_level.lower() == "critical":
            liquidctl_level = logging.CRITICAL
    root_logger = logging.getLogger("root")
    root_logger.setLevel(log_level)
    liquidctl_logger = logging.getLogger("liquidctl")
    liquidctl_logger.setLevel(liquidctl_level)
    formatter = log.Formatter("%(levelname)s%(message)s")
    console_handler = log.StreamHandler()
    console_handler.setFormatter(formatter)
    root_logger.addHandler(console_handler)


def try_connect_to_liquidctl_devices(device_service: DeviceService) -> None:
    for attempt in range(MAX_CONNECT_LIQUIDCTL_RETRIES):
        try:
            device_service.get_devices()
            return
        except BaseException as e:
            if attempt < MAX_CONNECT_LIQUIDCTL_RETRIES - 1:
                log.info(
                    "Exception when trying to connect to liquidctl devices "
                    f"(attempt {attempt + 1}/{MAX_CONNECT_LIQUIDCTL_RETRIES}): {e}"
                )
                time.sleep(1)
            else:
                log.error(
                    f"Failed to connect to liquidctl devices after {MAX_CONNECT_LIQUIDCTL_RETRIES} "
                    f"attempts. Shutting down liqctld: {traceback.format_exc()}"
                )
                raise e


def main() -> None:
    setup_logging()
    # set the process name
    with open("/proc/self/comm", "w") as f:
        f.write("coolercontrol-liqctld")
    log.info("liqctld service starting...")
    patch_kraken_lcd_packing()
    patch_kraken_lcd_transfer()
    patch_corsair_psu_report_drain()
    device_service = DeviceService()
    # We call liquidctl to find all devices, so that we can adjust the number of threads needed
    #  for parallel device communication.
    try_connect_to_liquidctl_devices(device_service)

    server_run(device_service)
    log.info("liqctld service shutdown complete.")


if __name__ == "__main__":
    main()
