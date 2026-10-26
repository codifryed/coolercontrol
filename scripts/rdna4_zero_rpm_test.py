#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Guy Boldon and contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""
Diagnostic harness for the RDNA3/RDNA4 "fan stays stopped" zero RPM latch.

Reported on an RX 9070 XT: after boot the fan sits at 0 RPM and ignores a flat curve applied
with Zero RPM disabled. Applying a higher duty once starts it, and from then on it follows
every curve, even the lowest. The sysfs readback is identical in the stuck and working states,
because the kernel prints its own cached OverDrive table, not the firmware state.

Each trial:
  1. Arm: reset fan control to firmware automatic and wait until the fan stops on its own.
  2. Apply: one write sequence (CoolerControl's, LACT's, reordered, delayed, ...).
  3. Observe: SPUN as soon as the fan turns for a few seconds, STUCK if it never does.

The first trial replays CoolerControl's sequence. If it gets STUCK, a reset reproduces the boot
state and every other trial runs without rebooting. If it SPINS, the reset is not equivalent,
and each trial must be run once per boot with --boot-trial.

Every commit that really reaches the firmware is detected through dynamic debug on the
kernel's OverDrive table dump, which only runs when a commit uploads. A commit that changes
nothing is skipped by the kernel, and that difference matters here.

Run with sudo, with coolercontrold and lactd stopped and the GPU idle.
"""

import argparse
import errno as errno_mod
import glob
import json
import logging
import os
import re
import subprocess  # nosec B404
import sys
from dataclasses import dataclass, field
from pathlib import Path
from time import sleep, time

__VERSION__ = "0.1.0"

LOG_FILE = "rdna4_zero_rpm_test.log"
JSON_FILE = "rdna4_zero_rpm_test.json"

log_format = "%(asctime)-15s %(levelname)-8s %(message)s"
logging.basicConfig(
    level=logging.INFO,
    format=log_format,
    handlers=[logging.FileHandler(LOG_FILE), logging.StreamHandler()],
)
log = logging.getLogger("CoolerControl-RDNA-ZeroRPM")

ERRNO_NAMES = {
    5: "EIO",
    6: "ENXIO",
    13: "EACCES",
    22: "EINVAL",
    38: "ENOSYS",
    95: "EOPNOTSUPP",
    524: "ENOTSUPP (kernel-internal, feature not supported by this ASIC/firmware)",
}

DMESG_HIGHLIGHTS = (
    "Invalid overdrive table content",
    "Failed to upload overdrive table",
    "not supported",
    "must be within",
    "Failed to",
)

# First line of <smu>_dump_od_table(), logged before every real upload once dynamic debug is on.
UPLOAD_MARKER = "OD: Gfxclk"
DUMP_FUNCTIONS = (
    "smu_v14_0_2_dump_od_table",
    "smu_v13_0_0_dump_od_table",
    "smu_v13_0_7_dump_od_table",
)
DYNAMIC_DEBUG_CONTROLS = (
    "/proc/dynamic_debug/control",
    "/sys/kernel/debug/dynamic_debug/control",
)

HEADER_ZERO_RPM_ENABLE = "FAN_ZERO_RPM_ENABLE:"
HEADER_ZERO_RPM_STOP_TEMP = "FAN_ZERO_RPM_STOP_TEMPERATURE:"
GUARDED_SERVICES = ("coolercontrold", "lactd")

CONTROL_DUTY = 20
LADDER_DUTIES = (25, 30, 40, 50, 70, 100)
# Consecutive one second readings that decide a verdict.
SPIN_CONFIRM_SECONDS = 3
STOP_CONFIRM_SECONDS = 3

SPUN = "SPUN"
STUCK = "STUCK"
PARTIAL = "PARTIAL"
NO_DATA = "NO DATA"
NOT_ARMED = "NOT ARMED"

PATTERN_FAN_CURVE_POINT = re.compile(
    r"(?P<index>\d+):\s+(?P<temp>\d+)C\s+(?P<duty>\d+)%"
)
PATTERN_FAN_CURVE_LIMITS_TEMP = re.compile(
    r"FAN_CURVE\(hotspot temp\):\s+(?P<temp_min>\d+)C\s+(?P<temp_max>\d+)C"
)
PATTERN_FAN_CURVE_LIMITS_DUTY = re.compile(
    r"FAN_CURVE\(fan speed\):\s+(?P<duty_min>\d+)%\s+(?P<duty_max>\d+)%"
)


def errno_name(code):
    if code is None:
        return ""
    if code in ERRNO_NAMES:
        return ERRNO_NAMES[code]
    return errno_mod.errorcode.get(code, f"unknown({code})")


def line_filler():
    log.info("#" * 100)


def thin_filler():
    log.info("-" * 100)


@dataclass
class Operation:
    """One sysfs write, with its errno and the kernel messages it produced."""

    trial: str
    label: str
    path: str
    wrote: str = ""
    ok: bool = True
    errno: int = None
    error: str = ""
    kernel_log: list = field(default_factory=list)

    @property
    def errno_name(self):
        return errno_name(self.errno)

    @property
    def uploads(self):
        return sum(1 for entry in self.kernel_log if UPLOAD_MARKER in entry)

    def as_dict(self):
        return {
            "trial": self.trial,
            "label": self.label,
            "path": self.path,
            "wrote": self.wrote,
            "ok": self.ok,
            "errno": self.errno,
            "errno_name": self.errno_name,
            "error": self.error,
            "uploads": self.uploads,
            "kernel_log": self.kernel_log,
        }

    def log_result(self):
        uploaded = f"  [uploaded x{self.uploads}]" if self.uploads else ""
        if self.ok:
            log.info(f"  OK    {self.label}: wrote {self.wrote!r}{uploaded}")
        else:
            log.error(
                f"  FAIL  {self.label}: wrote {self.wrote!r} -> {self.path}\n"
                f"        errno {self.errno} ({self.errno_name}): {self.error}"
            )
        for entry in self.kernel_log:
            log.info(f"        kernel: {entry}")


class KernelLog:
    """Reads only the kernel messages produced since the last checkpoint.

    /dev/kmsg carries every level, debug included, regardless of the console log level.
    """

    def __init__(self):
        self.fd = None
        try:
            self.fd = os.open("/dev/kmsg", os.O_RDONLY | os.O_NONBLOCK)
            os.lseek(self.fd, 0, os.SEEK_END)
        except OSError as e:
            log.warning(f"Could not open /dev/kmsg, kernel messages unavailable: {e}")

    def checkpoint(self):
        self.drain()

    def drain(self):
        if self.fd is None:
            return []
        lines = []
        while True:
            try:
                raw = os.read(self.fd, 8192)
            except BlockingIOError:
                break
            except OSError as e:
                # EPIPE means the ring buffer overtook us; the fd stays usable.
                if e.errno == errno_mod.EPIPE:
                    continue
                break
            if not raw:
                break
            for record in raw.decode("utf-8", "replace").splitlines():
                # Format: "<prio>,<seq>,<usec>,<flags>;<message>"
                _, _, message = record.partition(";")
                message = message.split("\n")[0].strip()
                if message:
                    lines.append(message)
        return lines

    def drain_relevant(self):
        relevant = []
        for entry in self.drain():
            lowered = entry.lower()
            if "amdgpu" in lowered or "amd" in lowered:
                relevant.append(entry)
                continue
            if any(marker in entry for marker in DMESG_HIGHLIGHTS):
                relevant.append(entry)
        return relevant

    def close(self):
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None


class Sysfs:
    """Every write to the GPU goes through here so nothing escapes the record."""

    def __init__(self, kernel_log, dry_run):
        self.kernel_log = kernel_log
        self.dry_run = dry_run
        self.operations = []
        self.trial = "init"

    def read(self, path):
        try:
            return Path(path).read_text()
        except OSError as e:
            log.debug(
                f"Read failed for {path}: errno {e.errno} ({errno_name(e.errno)})"
            )
            return None

    def write(self, path, value, label):
        op = Operation(trial=self.trial, label=label, path=str(path), wrote=value)
        if self.dry_run:
            op.error = "dry-run, not written"
            log.info(f"  DRY   {label}: would write {value!r} -> {path}")
            self.operations.append(op)
            return op
        self.kernel_log.checkpoint()
        try:
            with open(path, "w") as f:
                f.write(value)
        except OSError as e:
            op.ok = False
            op.errno = e.errno
            op.error = e.strerror or str(e)
        # The SMU round-trip is not instant; give the driver a moment to log.
        sleep(0.2)
        op.kernel_log = self.kernel_log.drain_relevant()
        op.log_result()
        self.operations.append(op)
        return op

    def wait(self, seconds):
        if not self.dry_run:
            sleep(seconds)


class DynamicDebug:
    """Turns on the OverDrive table dump so every real firmware upload is logged."""

    def __init__(self, dry_run):
        self.dry_run = dry_run
        self.control = None
        self.enabled = []

    def enable(self):
        if self.dry_run:
            log.info("  dry-run: dynamic debug not touched")
            return
        for candidate in DYNAMIC_DEBUG_CONTROLS:
            if Path(candidate).exists():
                self.control = candidate
                break
        if self.control is None:
            log.warning(
                "Dynamic debug control not found (no CONFIG_DYNAMIC_DEBUG, or debugfs not "
                "mounted). Uploads cannot be told apart from skipped commits."
            )
            return
        for function in DUMP_FUNCTIONS:
            if self._write(f"func {function} +p"):
                self.enabled.append(function)
        if self.enabled:
            log.info(f"  dynamic debug on for {self.enabled} via {self.control}")
        else:
            log.warning(
                "No OverDrive dump function matched (amdgpu built without dynamic debug "
                "symbols?). Uploads cannot be told apart from skipped commits."
            )

    def disable(self):
        for function in self.enabled:
            self._write(f"func {function} -p")
        self.enabled = []

    def _write(self, query):
        try:
            with open(self.control, "w") as f:
                f.write(f"{query}\n")
            return True
        except OSError as e:
            log.debug(f"dynamic debug '{query}' failed: {e}")
            return False


class FanCtrl:
    """The gpu_od/fan_ctrl interface for one AMD GPU."""

    def __init__(self, sysfs, device_path, hwmon_path):
        self.sysfs = sysfs
        self.device_path = device_path
        self.hwmon_path = hwmon_path
        self.fan_ctrl_path = device_path / "gpu_od" / "fan_ctrl"
        self.fan_curve_path = self.fan_ctrl_path / "fan_curve"
        self.zero_rpm_path = self.fan_ctrl_path / "fan_zero_rpm_enable"
        self.zero_rpm_stop_temp_path = (
            self.fan_ctrl_path / "fan_zero_rpm_stop_temperature"
        )
        self.zero_rpm_supported = False
        self.zero_rpm_stop_temp_supported = False
        self.temp_min = 0
        self.temp_max = 0
        self.duty_min = 0
        self.duty_max = 0
        self.point_count = 0

    # ---------------------------------------------------------------- reading

    def probe(self):
        """Reads limits and zero RPM support. Returns an error message, or None."""
        zero_rpm = self.sysfs.read(self.zero_rpm_path) or ""
        self.zero_rpm_supported = HEADER_ZERO_RPM_ENABLE in zero_rpm
        stop_temp = self.sysfs.read(self.zero_rpm_stop_temp_path) or ""
        self.zero_rpm_stop_temp_supported = HEADER_ZERO_RPM_STOP_TEMP in stop_temp
        log.info(f"fan_zero_rpm_enable:\n{zero_rpm.rstrip() or '(absent or empty)'}")
        log.info(
            f"fan_zero_rpm_stop_temperature:\n{stop_temp.rstrip() or '(absent or empty)'}"
        )
        if not self.zero_rpm_supported:
            return (
                f"{self.zero_rpm_path} does not show the {HEADER_ZERO_RPM_ENABLE} header, so "
                "this card does not support Zero RPM control. Writing 'r' to it would break "
                "OverDrive until reboot, and the latch under test cannot happen. Exiting."
            )
        curve = self.sysfs.read(self.fan_curve_path)
        if curve is None:
            return f"{self.fan_curve_path} could not be read. Exiting."
        log.info(f"fan_curve:\n{curve.rstrip()}")
        self.point_count = len(PATTERN_FAN_CURVE_POINT.findall(curve))
        if not self.parse_limits(curve) or self.point_count == 0:
            return (
                "fan_curve has no OD_RANGE or no points. The curve is not changeable "
                "(is overdrive enabled in ppfeaturemask?). Exiting."
            )
        if self.sysfs.read(self.hwmon_path / "fan1_input") is None:
            return f"No readable fan1_input under {self.hwmon_path}. Exiting."
        return None

    def parse_limits(self, content):
        found_temp = found_duty = False
        for line in content.splitlines():
            temp_match = PATTERN_FAN_CURVE_LIMITS_TEMP.match(line.strip())
            if temp_match is not None:
                self.temp_min = int(temp_match.group("temp_min"))
                self.temp_max = int(temp_match.group("temp_max"))
                found_temp = True
                continue
            duty_match = PATTERN_FAN_CURVE_LIMITS_DUTY.match(line.strip())
            if duty_match is not None:
                self.duty_min = int(duty_match.group("duty_min"))
                self.duty_max = int(duty_match.group("duty_max"))
                found_duty = True
        return found_temp and found_duty

    def read_temps(self):
        temps = {}
        for temp_file in sorted(self.hwmon_path.glob("temp*_input")):
            raw = self.sysfs.read(temp_file)
            if raw is None:
                continue
            label_file = temp_file.with_name(temp_file.name.replace("_input", "_label"))
            label = self.sysfs.read(label_file)
            name = label.strip() if label else temp_file.name
            try:
                temps[name] = int(raw.strip()) / 1000.0
            except ValueError:
                continue
        return temps

    def read_fan(self):
        """(pwm 0-255, rpm) as integers, or None where unreadable."""

        def as_int(raw):
            try:
                return int(raw.strip())
            except (AttributeError, ValueError):
                return None

        return (
            as_int(self.sysfs.read(self.hwmon_path / "pwm1")),
            as_int(self.sysfs.read(self.hwmon_path / "fan1_input")),
        )

    def readback(self):
        """What the kernel reports, which is its cache and not the firmware state."""
        curve = self.sysfs.read(self.fan_curve_path) or ""
        zero_rpm = self.sysfs.read(self.zero_rpm_path) or ""
        points = [
            (int(m.group("temp")), int(m.group("duty")))
            for m in PATTERN_FAN_CURVE_POINT.finditer(curve)
        ]
        zero_rpm_lines = zero_rpm.split()
        zero_rpm_value = zero_rpm_lines[1] if len(zero_rpm_lines) > 1 else None
        return {"points": points, "zero_rpm_enable": zero_rpm_value}

    # ---------------------------------------------------------------- writing

    def clamp_duty(self, duty):
        return max(self.duty_min, min(self.duty_max, duty))

    def flat_points(self, duty):
        """The flat curve CoolerControl writes for a fixed duty."""
        points = [(self.temp_min, duty)]
        for _ in range(1, self.point_count):
            points.append((self.temp_max, duty))
        return points

    def stage_curve(self, duty):
        for index, (temp, point_duty) in enumerate(self.flat_points(duty)):
            self.sysfs.write(
                self.fan_curve_path,
                f"{index} {temp} {point_duty}\n",
                f"fan_curve[{index}] {point_duty}%",
            )

    def commit_curve(self):
        return self.sysfs.write(self.fan_curve_path, "c\n", "fan_curve commit")

    def apply_curve(self, duty):
        self.stage_curve(duty)
        return self.commit_curve()

    def stage_zero_rpm(self, enable):
        value = int(enable)
        return self.sysfs.write(
            self.zero_rpm_path, f"{value}\n", f"fan_zero_rpm_enable {value}"
        )

    def apply_zero_rpm(self, enable):
        self.stage_zero_rpm(enable)
        return self.sysfs.write(self.zero_rpm_path, "c\n", "fan_zero_rpm_enable commit")

    def reset_to_auto(self):
        """Firmware automatic mode. Each 'r' commits by itself in the kernel."""
        self.sysfs.write(self.zero_rpm_path, "r\n", "fan_zero_rpm_enable reset")
        if self.zero_rpm_stop_temp_supported:
            self.sysfs.write(
                self.zero_rpm_stop_temp_path,
                "r\n",
                "fan_zero_rpm_stop_temperature reset",
            )
        self.sysfs.write(self.fan_curve_path, "r\n", "fan_curve reset")


# -------------------------------------------------------------------------------- observing


def hottest(temps):
    return max(temps.values()) if temps else None


def pwm_percent(pwm):
    if pwm is None:
        return "n/a"
    return f"{round(pwm / 255 * 100)}%"


def observe(fan, seconds, label):
    """Watches the fan until it spins for SPIN_CONFIRM_SECONDS or the window ends."""
    log.info(f"  observing '{label}' for up to {seconds}s")
    samples = []
    spin_streak = 0
    verdict = None
    for second in range(seconds):
        pwm, rpm = fan.read_fan()
        temps = fan.read_temps()
        samples.append(
            {"t": second, "pwm1": pwm, "rpm": rpm, "hottest": hottest(temps)}
        )
        log.info(
            f"    t+{second:>2}s  pwm1={pwm} ({pwm_percent(pwm)})  rpm={rpm}  "
            f"hottest={hottest(temps)}"
        )
        spin_streak = spin_streak + 1 if rpm else 0
        if spin_streak >= SPIN_CONFIRM_SECONDS:
            verdict = SPUN
            break
        fan.sysfs.wait(1)

    rpms = [s["rpm"] for s in samples if s["rpm"] is not None]
    if verdict is None:
        if not rpms:
            verdict = NO_DATA
        elif max(rpms) == 0:
            verdict = STUCK
        else:
            verdict = PARTIAL
    result = {
        "label": label,
        "verdict": verdict,
        "seconds": len(samples),
        "rpm_max": max(rpms) if rpms else None,
        "rpm_final": rpms[-1] if rpms else None,
        "pwm_final": samples[-1]["pwm1"] if samples else None,
        "hottest_max": max(
            (s["hottest"] for s in samples if s["hottest"] is not None), default=None
        ),
        "samples": samples,
    }
    log.info(
        f"  -> {verdict}: '{label}' rpm max {result['rpm_max']}, final {result['rpm_final']}, "
        f"pwm1 final {result['pwm_final']} ({pwm_percent(result['pwm_final'])})"
    )
    return result


def arm(fan, timeout):
    """Resets to firmware automatic and waits for the firmware to stop the fan."""
    log.info("  arming: reset to firmware automatic, then wait for 0 RPM")
    fan.reset_to_auto()
    if fan.sysfs.dry_run:
        log.info("  dry-run: assuming the fan stopped")
        return True, 0
    start = time()
    stop_streak = 0
    while time() - start < timeout:
        _, rpm = fan.read_fan()
        stop_streak = stop_streak + 1 if rpm == 0 else 0
        if stop_streak >= STOP_CONFIRM_SECONDS:
            elapsed = round(time() - start)
            log.info(f"  armed: fan stopped after {elapsed}s")
            return True, elapsed
        sleep(1)
    log.warning(
        f"  NOT ARMED: the fan did not stop within {timeout}s. The GPU is probably above its "
        f"zero RPM stop temperature (hottest now {hottest(fan.read_temps())}). Is it idle?"
    )
    return False, timeout


# -------------------------------------------------------------------------------- trials


class Trial:
    """Records the steps of one trial and the uploads each one caused."""

    def __init__(self, name, fan, args):
        self.name = name
        self.fan = fan
        self.args = args
        self.steps = []
        self.armed = None
        self.arm_seconds = None

    def step(self, label, writes, seconds):
        """Runs the writes, then observes. Returns the verdict."""
        thin_filler()
        log.info(f"[{self.name}] {label}")
        start_op = len(self.fan.sysfs.operations)
        writes()
        ops = self.fan.sysfs.operations[start_op:]
        result = observe(self.fan, seconds, label)
        result["uploads"] = sum(op.uploads for op in ops)
        result["failed_writes"] = sum(1 for op in ops if not op.ok)
        result["readback"] = self.fan.readback()
        self.steps.append(result)
        return result["verdict"]

    @property
    def verdict(self):
        if self.armed is False:
            return NOT_ARMED
        return self.steps[0]["verdict"] if self.steps else None

    def as_dict(self):
        return {
            "name": self.name,
            "verdict": self.verdict,
            "armed": self.armed,
            "arm_seconds": self.arm_seconds,
            "steps": self.steps,
        }


def control_duty(fan):
    """20%, the reporter's stuck duty, kept above the floor so the floor step uploads."""
    return fan.clamp_duty(max(CONTROL_DUTY, fan.duty_min + 5))


def run_control(trial):
    """CoolerControl's sequence, then the steps that find what restarts a stuck fan."""
    fan, args = trial.fan, trial.args
    duty = control_duty(fan)

    def control_writes():
        fan.apply_zero_rpm(False)
        fan.apply_curve(duty)

    verdict = trial.step(f"zero RPM off + {duty}% curve", control_writes, args.observe)
    if verdict != STUCK:
        return

    # A lower duty that still uploads: tells "any upload" apart from "a higher duty".
    chain = [fan.duty_min, duty + 1]
    chain.extend(d for d in LADDER_DUTIES if duty + 1 < d <= fan.duty_max)
    started_at = None
    for step_duty in chain:
        verdict = trial.step(
            f"{step_duty}% curve from stuck",
            lambda d=step_duty: fan.apply_curve(d),
            args.step,
        )
        if verdict == SPUN:
            started_at = step_duty
            break
    if started_at is None:
        return
    trial.step(
        f"back to {duty}% after starting",
        lambda: fan.apply_curve(duty),
        args.step,
    )


def run_curve_only(trial):
    duty = control_duty(trial.fan)
    trial.step(
        f"{duty}% curve, zero RPM left on",
        lambda: trial.fan.apply_curve(duty),
        trial.args.observe,
    )


def run_zr_only(trial):
    trial.step(
        "zero RPM off, firmware curve",
        lambda: trial.fan.apply_zero_rpm(False),
        trial.args.observe,
    )


def run_curve_first(trial):
    fan = trial.fan
    duty = control_duty(fan)

    def writes():
        fan.apply_curve(duty)
        fan.apply_zero_rpm(False)

    trial.step(f"{duty}% curve, then zero RPM off", writes, trial.args.observe)


def run_single_upload(trial):
    fan = trial.fan
    duty = control_duty(fan)

    def writes():
        fan.stage_zero_rpm(False)
        fan.stage_curve(duty)
        fan.commit_curve()

    trial.step(f"zero RPM off + {duty}% curve, one commit", writes, trial.args.observe)


def run_gap(trial):
    fan, args = trial.fan, trial.args
    duty = control_duty(fan)

    def writes():
        fan.apply_zero_rpm(False)
        log.info(f"  waiting {args.gap}s before the curve")
        fan.sysfs.wait(args.gap)
        fan.apply_curve(duty)

    trial.step(f"zero RPM off, {args.gap}s gap, {duty}% curve", writes, args.observe)


CONTROL = "control"
TRIALS = {
    CONTROL: run_control,
    "curve-only": run_curve_only,
    "zr-only": run_zr_only,
    "curve-first": run_curve_first,
    "single-upload": run_single_upload,
    "gap": run_gap,
    "control-again": run_control,
}


def run_trial(name, fan, args, boot):
    line_filler()
    log.info(f"TRIAL {name}")
    line_filler()
    fan.sysfs.trial = name
    trial = Trial(name, fan, args)
    if boot:
        log.info(f"  boot trial, no reset: uptime {read_uptime()}s")
        log.info(f"  kernel readback before any write: {fan.readback()}")
        _, rpm = fan.read_fan()
        trial.armed = rpm == 0
        trial.arm_seconds = 0
        if not trial.armed:
            log.warning(
                f"  The fan is already turning ({rpm} RPM), so this is not the stopped boot "
                "state. Running anyway; read the result with that in mind."
            )
    else:
        trial.armed, trial.arm_seconds = arm(fan, args.arm_timeout)
        if not trial.armed:
            return trial
    TRIALS[name](trial)
    return trial


# -------------------------------------------------------------------------------- setup


def verify_root(dry_run):
    if dry_run:
        return
    if os.geteuid() != 0:
        log.error(
            "This script must be run with sudo/root privileges to write fan control settings."
        )
        sys.exit(1)


def systemctl(verb, unit):
    try:
        result = subprocess.run(
            ["/usr/bin/systemctl", verb, unit],  # nosec B603
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    return result.stdout.strip()


def verify_services(boot):
    """Another fan controller writing behind our back invalidates every result."""
    if not Path("/usr/bin/systemctl").exists():
        log.warning(
            f"systemctl not found; make sure {', '.join(GUARDED_SERVICES)} are not running."
        )
        return True
    blocking = []
    for unit in GUARDED_SERVICES:
        if systemctl("is-active", unit) == "active":
            blocking.append(f"{unit} is running: sudo systemctl stop {unit}")
        # Enabled means it already wrote at this boot, so the boot state is gone.
        if boot and systemctl("is-enabled", unit) == "enabled":
            blocking.append(
                f"{unit} is enabled, so it ran at boot: sudo systemctl disable {unit}, "
                "then reboot"
            )
    for line in blocking:
        log.error(line)
    return not blocking


def use_device_path(device_path_arg):
    """A device directory given directly, as in the snapshots under scripts/rdna4_data/."""
    device_path = Path(device_path_arg).resolve()
    hwmon_dirs = sorted((device_path / "hwmon").glob("hwmon*"))
    if not device_path.is_dir() or not hwmon_dirs:
        log.error(f"No hwmon* directory under {device_path / 'hwmon'}.")
        return None, None
    return hwmon_dirs[0], device_path


def find_amdgpu_hwmon(device_filter):
    """(hwmon_path, device_path) for the first amdgpu card with a PMFW fan curve."""
    for name_file in sorted(glob.glob("/sys/class/hwmon/hwmon*/name")):
        try:
            if "amdgpu" not in Path(name_file).read_text():
                continue
        except OSError:
            continue
        hwmon_path = Path(name_file).parent
        device_path = (hwmon_path / "device").resolve()
        if device_filter and device_filter not in str(device_path):
            continue
        if (device_path / "gpu_od" / "fan_ctrl" / "fan_curve").exists():
            return hwmon_path, device_path
    return None, None


def read_uptime():
    try:
        return round(float(Path("/proc/uptime").read_text().split()[0]))
    except (OSError, ValueError, IndexError):
        return None


def read_context(sysfs, hwmon_path, device_path):
    """Everything needed to interpret the results on someone else's machine."""
    context = {
        "script_version": __VERSION__,
        "kernel": os.uname().release,
        "uptime_seconds": read_uptime(),
        "hwmon_path": str(hwmon_path),
        "device_path": str(device_path),
    }
    for label, rel in (
        ("pci_device", "device"),
        ("pci_revision", "revision"),
        ("subsystem_device", "subsystem_device"),
        ("subsystem_vendor", "subsystem_vendor"),
        ("vbios_version", "vbios_version"),
        ("power_dpm_force_performance_level", "power_dpm_force_performance_level"),
    ):
        value = sysfs.read(device_path / rel)
        if value is not None:
            context[label] = value.strip()
    fw_dir = device_path / "fw_version"
    if fw_dir.is_dir():
        context["fw_version"] = {
            entry.name: (sysfs.read(entry) or "").strip()
            for entry in sorted(fw_dir.iterdir())
        }
    context["fan_ctrl_files"] = {
        entry.name: sysfs.read(entry)
        for entry in sorted((device_path / "gpu_od" / "fan_ctrl").iterdir())
    }
    return context


# -------------------------------------------------------------------------------- summary


def describe_steps(trial, dynamic_debug_on):
    def uploads(step):
        return step["uploads"] if dynamic_debug_on else "?"

    return "  ".join(
        f"[{step['label']}: {step['verdict']}, uploads {uploads(step)}]"
        for step in trial.steps
    )


def conclude(trials, dynamic_debug_on, boot):
    """Turns the trial verdicts into the findings worth reporting."""
    findings = {}
    by_name = {trial.name: trial for trial in trials}
    control = by_name.get(CONTROL)
    if control is not None and not boot:
        if control.verdict == STUCK:
            findings["reset_reproduces_boot_state"] = True
            log.info(
                "Reset reproduces the stuck state: CoolerControl's sequence left the fan "
                "stopped after a reset, with no reboot needed."
            )
        elif control.verdict == SPUN:
            findings["reset_reproduces_boot_state"] = False
            log.info(
                "Reset does NOT reproduce the stuck state: the fan spun after a reset. Run each "
                "trial once per boot with --boot-trial NAME, with coolercontrold disabled."
            )

    for trial in trials:
        if trial.name not in (CONTROL, "control-again") or trial.verdict != STUCK:
            continue
        floor_step = trial.steps[1] if len(trial.steps) > 1 else None
        restarted = [step for step in trial.steps[1:] if step["verdict"] == SPUN]
        key = trial.name.replace("-", "_")
        if floor_step is not None and floor_step["verdict"] == SPUN:
            findings[f"{key}_any_upload_restarts"] = True
            log.info(
                f"[{trial.name}] A LOWER duty upload restarted the fan: any real upload "
                "clears the latch, the duty is irrelevant."
            )
        if restarted:
            findings[f"{key}_restarted_by"] = restarted[0]["label"]
            log.info(f"[{trial.name}] The fan restarted at: {restarted[0]['label']}")
            held = trial.steps[-1]
            if held is not restarted[0]:
                findings[f"{key}_holds_after_start"] = held["verdict"]
                log.info(
                    f"[{trial.name}] After starting, '{held['label']}': {held['verdict']}"
                )
        else:
            findings[f"{key}_restarted_by"] = None
            log.info(f"[{trial.name}] Nothing in the chain restarted the fan.")
        if dynamic_debug_on:
            silent = [s["label"] for s in trial.steps if s["uploads"] == 0]
            if silent:
                log.info(f"[{trial.name}] Steps that uploaded nothing: {silent}")

    if control is not None and control.verdict == STUCK:
        candidates = [
            trial.name
            for trial in trials
            if trial.name not in (CONTROL, "control-again", "curve-only")
            and trial.verdict == SPUN
        ]
        findings["workaround_candidates"] = candidates
        log.info(f"Workaround candidates (SPUN while control was STUCK): {candidates}")

    again = by_name.get("control-again")
    if control is not None and again is not None:
        findings["control_repeatable"] = control.verdict == again.verdict
        log.info(f"Control repeatable: {control.verdict} then {again.verdict}")
    return findings


def print_summary(trials, operations, dynamic_debug_on, boot):
    line_filler()
    log.info("SUMMARY")
    line_filler()
    if not dynamic_debug_on:
        log.info("Upload counts are unknown: dynamic debug was unavailable.")
    for trial in trials:
        log.info(f"{trial.name:<14} {str(trial.verdict):<10} arm {trial.arm_seconds}s")
        if trial.steps:
            log.info(f"    {describe_steps(trial, dynamic_debug_on)}")
    failures = [op for op in operations if not op.ok]
    if failures:
        thin_filler()
        log.info(f"{len(failures)} failed write(s):")
        for op in failures:
            log.info(
                f"  [{op.trial}] {op.label}: errno {op.errno} ({op.errno_name}) {op.error}"
            )
    thin_filler()
    return conclude(trials, dynamic_debug_on, boot)


# -------------------------------------------------------------------------------- main


def parse_args():
    parser = argparse.ArgumentParser(
        description="CoolerControl diagnostic for the RDNA3/4 zero RPM stopped-fan latch",
        formatter_class=argparse.RawTextHelpFormatter,
    )
    parser.add_argument(
        "-v", "--version", action="version", version=f"\n {__VERSION__}"
    )
    parser.add_argument(
        "-d", "--debug", action="store_true", help="enable debug output"
    )
    parser.add_argument(
        "--trial",
        action="append",
        choices=list(TRIALS),
        metavar="NAME",
        help="run only these trials (repeatable); 'control' always runs first.\n"
        f"choices: {', '.join(TRIALS)}",
    )
    parser.add_argument(
        "--boot-trial",
        choices=list(TRIALS),
        metavar="NAME",
        help="run ONE trial without resetting first, right after a reboot with\n"
        "coolercontrold and lactd disabled. Use when 'control' SPUN after a reset.",
    )
    parser.add_argument("--yes", action="store_true", help="skip the start prompt")
    parser.add_argument(
        "--observe",
        type=int,
        default=15,
        metavar="SECONDS",
        help="first observation of each trial (default 15)",
    )
    parser.add_argument(
        "--step",
        type=int,
        default=10,
        metavar="SECONDS",
        help="observation of each follow-up step from a stuck fan (default 10)",
    )
    parser.add_argument(
        "--arm-timeout",
        type=int,
        default=180,
        metavar="SECONDS",
        help="how long to wait for the fan to stop after a reset (default 180)",
    )
    parser.add_argument(
        "--gap",
        type=int,
        default=5,
        metavar="SECONDS",
        help="delay between the zero RPM and curve commits in the 'gap' trial (default 5)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="log intended writes without performing them",
    )
    parser.add_argument(
        "--device",
        default="",
        help="restrict to the GPU whose device path contains this string",
    )
    parser.add_argument(
        "--device-path",
        default="",
        metavar="PATH",
        help="use this device directory instead of searching /sys; pair with --dry-run,\n"
        "e.g. --dry-run --device-path scripts/rdna4_data/rx9070xt",
    )
    return parser.parse_args()


def selected_trials(args):
    if args.boot_trial:
        return [args.boot_trial]
    if not args.trial:
        return list(TRIALS)
    chosen = [name for name in TRIALS if name in args.trial]
    return chosen if CONTROL in chosen else [CONTROL, *chosen]


def main():
    args = parse_args()
    if args.debug:
        log.setLevel(logging.DEBUG)
    boot = args.boot_trial is not None

    line_filler()
    log.info(f"CoolerControl RDNA3/4 zero RPM latch diagnostic v{__VERSION__}")
    line_filler()
    verify_root(args.dry_run)
    if not args.dry_run and not verify_services(boot):
        sys.exit(1)

    if args.device_path:
        hwmon_path, device_path = use_device_path(args.device_path)
    else:
        hwmon_path, device_path = find_amdgpu_hwmon(args.device)
    if hwmon_path is None:
        log.error(
            "No amdgpu GPU with a PMFW fan curve (gpu_od/fan_ctrl) found. Exiting."
        )
        sys.exit(1)
    log.info(f"AMD GPU hwmon: {hwmon_path}")
    log.info(f"AMD GPU device: {device_path}")

    kernel_log = KernelLog()
    sysfs = Sysfs(kernel_log, args.dry_run)
    fan = FanCtrl(sysfs, device_path, hwmon_path)
    context = read_context(sysfs, hwmon_path, device_path)
    log.info(f"  kernel {context['kernel']}, uptime {context['uptime_seconds']}s")
    error = fan.probe()
    if error is not None:
        log.error(error)
        sys.exit(1)
    log.info(
        f"Curve: {fan.point_count} points, temp {fan.temp_min}-{fan.temp_max}C, "
        f"duty {fan.duty_min}-{fan.duty_max}%. Control duty {control_duty(fan)}%."
    )

    names = selected_trials(args)
    log.info(f"Trials: {', '.join(names)}")
    log.info(
        "Keep the GPU idle for the whole run: no games, video encoding, or other GPU load. "
        "The fan must be able to stop on its own between trials."
    )
    if not args.yes and not args.dry_run:
        input("Press Enter to start...")

    dynamic_debug = DynamicDebug(args.dry_run)
    dynamic_debug.enable()
    dynamic_debug_on = bool(dynamic_debug.enabled)
    trials = []
    start = time()
    try:
        for name in names:
            trial = run_trial(name, fan, args, boot)
            trials.append(trial)
            if name == CONTROL and not boot and trial.verdict != STUCK:
                log.warning(
                    f"'control' was {trial.verdict}, not STUCK. The other trials need the "
                    "stuck state, so they are skipped."
                )
                break
    except KeyboardInterrupt:
        log.warning("Interrupted.")
    finally:
        line_filler()
        log.info("Restoring firmware automatic fan control")
        sysfs.trial = "restore"
        fan.reset_to_auto()
        dynamic_debug.disable()
        findings = print_summary(trials, sysfs.operations, dynamic_debug_on, boot)
        report = {
            "version": __VERSION__,
            "context": context,
            "args": vars(args),
            "dynamic_debug": dynamic_debug.enabled,
            "findings": findings,
            "trials": [trial.as_dict() for trial in trials],
            "operations": [op.as_dict() for op in sysfs.operations],
        }
        Path(JSON_FILE).write_text(json.dumps(report, indent=2) + "\n")
        kernel_log.close()
        thin_filler()
        log.info(f"Completed in {time() - start:.1f}s")
        log.info(f"Output saved to {LOG_FILE} and {JSON_FILE}. Please send both.")
        log.info(
            "Fan control is back on firmware automatic. Re-enable CoolerControl with: "
            "sudo systemctl enable --now coolercontrold"
        )


if __name__ == "__main__":
    main()
