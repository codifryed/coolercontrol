#!/usr/bin/env python3
# ----------------------------------------------------------------------------------------------------------------------
#  CoolerControl - monitor and control your cooling and other devices
#  Copyright (c) 2024  Guy Boldon
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
import argparse
import glob
import logging
import os
import re
import sys
from pathlib import Path
from time import sleep, time

# ----------------------------------------------------------------------------------------------------------------------
#
# This is a script to help determine how RDNA4 sysfs settings will work for CoolerControl
#
# ----------------------------------------------------------------------------------------------------------------------

log_format = "%(asctime)-15s %(levelname)-8s %(name)s - %(message)s"
logging.basicConfig(
    level=logging.INFO,
    format=log_format,
    handlers=[logging.FileHandler("rdna4_test.log"), logging.StreamHandler()],
)
log = logging.getLogger("CoolerControl-RDNA3/4")

__VERSION__ = "0.0.1"


class RDNA4Test:

    def __init__(self):
        parser = argparse.ArgumentParser(
            description="A CoolerControl test script for RDNA3/4 sysfs fan control",
            exit_on_error=False,
            # formatter_class=argparse.RawTextHelpFormatter,
        )
        parser.add_argument(
            "-v", "--version", action="version", version=f"\n {__VERSION__}"
        )
        parser.add_argument(
            "-d", "--debug", action="store_true", help="enable debug output \n"
        )
        parser.add_argument(
            "-t",
            "--test",
            action="store_true",
            help="enable test mode for using local sysfs mocks \n",
        )
        self.args = parser.parse_args()
        if self.args.debug:
            log.setLevel(logging.DEBUG)
        self.verify_full_access()
        self.hwmon_path: Path = self.find_amdgpu_hwmon_path()
        self.device_path: Path = self.get_device_path()

        self.fan_curve_path: Path = self.get_fan_curve_path()
        if self.fan_curve_path.exists():
            log.info("fan_curve file exists")
        else:
            log.error(f"fan_curve file not found in {self.fan_curve_path}")
            sys.exit(1)

        self.zero_rpm_enable_path: Path = self.get_zero_rpm_enable_path()
        if self.zero_rpm_enable_path.exists():
            log.info("zero_rpm_enable file exists")
        else:
            log.error(f"zero_rpm_enable file not found in {self.zero_rpm_enable_path}")

        self.zer_rpm_stop_temp_path: Path = self.get_zero_rpm_stop_temp_path()
        if self.zer_rpm_stop_temp_path.exists():
            log.info("zero_rpm_stop_temperature file exists")
        else:
            log.error(
                f"zero_rpm_stop_temperature file not found in {self.zero_rpm_enable_path}"
            )

        self.temp_min: int = -1
        self.temp_max: int = -100
        self.duty_min: int = -1
        self.duty_max: int = -100
        self.fan_curve_size: int = 0

    @staticmethod
    def verify_full_access():
        if os.geteuid() != 0:
            log.error(
                "This test script should be run with sudo/root privileges "
                "to be able to properly change the fan speed."
            )
            sys.exit(1)

    def find_amdgpu_hwmon_path(self) -> Path:
        if self.args.test:
            return Path("rdna4_data") / "rx9070xt" / "hwmon" / "hwmon5"
        hwmon_path: Path | None = None
        for hwmon_name in glob.glob("/sys/class/hwmon/hwmon*/name"):
            if "amdgpu" in Path(hwmon_name).read_text():
                hwmon_path = Path(hwmon_name).parent
                log.info(f"Found AMDGPU hwmon sysfs at {hwmon_path}")
                break
        if hwmon_path is None:
            log.error("Could not find AMDGPU hwmon path. Exiting.")
            sys.exit(1)
        return hwmon_path

    def get_device_path(self) -> Path:
        if self.args.test:
            return Path("rdna4_data") / "rx9070xt"
        device_path = (self.hwmon_path / "device").resolve()
        log.info(f"Device path: {device_path}")
        return device_path

    def get_fan_curve_path(self) -> Path:
        return self.device_path / "gpu_od" / "fan_ctrl" / "fan_curve"

    def get_zero_rpm_enable_path(self) -> Path:
        return self.device_path / "gpu_od" / "fan_ctrl" / "fan_zero_rpm_enable"

    def get_zero_rpm_stop_temp_path(self) -> Path:
        return (
            self.device_path / "gpu_od" / "fan_ctrl" / "fan_zero_rpm_stop_temperature"
        )

    def read_sensors(self) -> None:
        self._log_thin_line_filler()
        log.info("READING SYSFS DATA:")
        self._log_thin_line_filler()
        self._print_pwm_fan_speed()
        self._print_fan_rpm()
        self._print_temps()
        self._print_zero_rpm_enable()
        self._print_zero_rpm_stop_temp()

        #  output the fan_curve contents
        fan_curve_points = self.get_fan_curve()
        log.info(f"Fan Curve Points: {fan_curve_points}")
        self.fan_curve_size = self._determine_fan_curve_size(fan_curve_points)
        self._determine_fan_curve_limits()
        log.info(
            f"Current Fan Curve Limits: "
            f"Temp({self.temp_min}-{self.temp_max}) "
            f"Duty({self.duty_min}-{self.duty_max})"
        )
        self._log_thin_line_filler()
        log.info(".")

    @staticmethod
    def _log_thin_line_filler():
        log.info(
            "--------------------------------------------------------------------------------"
        )

    def _print_pwm_fan_speed(self) -> None:
        pwm_file = self.hwmon_path / "pwm1"
        if pwm_file.exists():
            raw_pwm = int(pwm_file.read_text())
            log.info(f"PWM value(0-255): {raw_pwm}")
            pwm_writable = os.access(pwm_file, os.W_OK)
            log.info(f"PWM File writable: {pwm_writable}")
        else:
            log.warning("pwm1 file not found")

    def _print_fan_rpm(self):
        input_file = self.hwmon_path / "fan1_input"
        if input_file.exists():
            raw_input = int(input_file.read_text())
            log.info(f"RPM value: {raw_input}")
        else:
            log.warning("fan1_input file not found")

    def _print_temps(self):
        for temp_file in self.hwmon_path.glob("temp*_input"):
            temp = int(temp_file.read_text()) / 1000.0
            log.info(f"{temp_file.name}: {temp}C")

    def get_fan_curve(self) -> list[(int, int)]:
        fan_curve_points = []
        for line in self.fan_curve_path.read_text().splitlines():
            match = re.search(
                r"(?P<index>\d+):\s+(?P<temp>\d+)C\s+(?P<duty>\d+)%", line
            )
            if match is None:
                continue
            temp = int(match.group("temp"))
            duty = int(match.group("duty"))
            fan_curve_points.append((temp, duty))
        return fan_curve_points

    @staticmethod
    def _determine_fan_curve_size(fan_curve_points: list[(int, int)]) -> int:
        return len(fan_curve_points)

    def _determine_fan_curve_limits(self):
        for line in self.fan_curve_path.read_text().splitlines():
            temp_match = re.match(
                r"FAN_CURVE\(hotspot temp\):\s+(?P<temp_min>\d+)C\s+(?P<temp_max>\d+)C",
                line,
            )
            if temp_match is not None:
                self.temp_min = int(temp_match.group("temp_min"))
                self.temp_max = int(temp_match.group("temp_max"))
                continue
            duty_match = re.match(
                r"FAN_CURVE\(fan speed\):\s+(?P<duty_min>\d+)%\s+(?P<duty_max>\d+)%",
                line,
            )
            if duty_match is not None:
                self.duty_min = int(duty_match.group("duty_min"))
                self.duty_max = int(duty_match.group("duty_max"))
                continue
        if (
            self.temp_min < 0
            or self.temp_max < 0
            or self.duty_min < 0
            or self.duty_max < 0
        ):
            log.error(
                "Could not determine fan curve limits. This means the fan_curve is not changeable."
            )
            log.error(f"fan_curve contents: {self.fan_curve_path.read_text()}")
            sys.exit(1)

    def _print_zero_rpm_enable(self):
        if not self.zero_rpm_enable_path.exists():
            log.info("Zero RPM Enable does not exist")
            return
        try:
            log.info(
                f"Zero RPM Enable content:\n{self.zero_rpm_enable_path.read_text()}"
            )
        except Exception as e:
            log.warning(f"Zero RPM Enable content ERROR: {e}")

    def _print_zero_rpm_stop_temp(self):
        if not self.zer_rpm_stop_temp_path.exists():
            log.info("Zero RPM Stop Temp does not exist")
            return
        try:
            log.info(
                f"Zero RPM Stop Temp content:\n{self.zer_rpm_stop_temp_path.read_text()}"
            )
        except Exception as e:
            log.warning(f"Zero RPM Stop Temp content ERROR: {e}")

    def reset_fan_curve(self):
        if self.args.test:
            log.info("TEST Resetting fan curve")
            return
        try:
            log.info("Resetting fan curve")
            self.fan_curve_path.write_text("r\n")
        except Exception as e:
            log.error(f"Error resetting fan curve: {e}")

    def _set_fan_curve(self, new_fan_curve: list[(int, int)]):
        if len(new_fan_curve) != self.fan_curve_size:
            log.error(
                f"Invalid fan curve size: {len(new_fan_curve)}. "
                f"Must be the same size as the current fan curve size: {self.fan_curve_size}"
                f"New Fan Curve: {new_fan_curve}"
            )
            return
        start_time = time()
        for index, (temp, duty) in enumerate(new_fan_curve):
            # disable range validation for now:
            # if temp < self.temp_min or temp > self.temp_max:
            #     log.error(
            #         f"Invalid temp: {temp}. "
            #         f"Must be between allowed limits of {self.temp_min} and {self.temp_max}"
            #     )
            #     continue
            # if duty < self.duty_min or duty > self.duty_max:
            #     log.error(
            #         f"Invalid duty: {duty}. "
            #         f"Must be between allowed limits of {self.duty_min} and {self.duty_max}"
            #     )
            #     continue
            new_curve_point = f"{index} {temp} {duty}\n"
            if self.args.test:
                log.debug(f"TEST Setting fan curve point: {new_curve_point}")
                continue
            try:
                self.fan_curve_path.write_text(new_curve_point)
            except Exception as e:
                log.error(
                    f"Error setting fan curve point: {new_curve_point}; "
                    f"Error: {e};\n"
                )
        log.info(f"Fan Curve {new_fan_curve} Set in {time() - start_time:.3f} seconds")

    def commit_fan_curve_changes(self):
        if self.args.test:
            log.info("TEST Committing new fan curve")
            return
        try:
            self.fan_curve_path.write_text("c\n")
        except Exception as e:
            log.error(f"Error committing new fan curve: {e}")

    def _duty_not_within_limits(self, duty: int) -> bool:
        not_within_limits = duty < self.duty_min or duty > self.duty_max
        if not_within_limits:
            log.error(
                f"Invalid duty: {duty}. "
                f"Must be between allowed limits of {self.duty_min} and {self.duty_max}"
            )
        return not_within_limits

    def apply_flat_simple_fan_curve(self, duty: int) -> None:
        # validity check disabled to check driver reaction:
        # if self._duty_not_within_limits(duty):
        #     return
        new_fan_curve = []
        steps = [self.temp_min, self.temp_max]
        for _ in range(self.fan_curve_size - 2):
            steps.append(self.temp_max)
        for temp in steps:
            new_fan_curve.append((int(temp), int(duty)))
        self._set_fan_curve(new_fan_curve)

    @staticmethod
    def wait_for_fan_stabilization(seconds: int = 3) -> None:
        for _ in range(seconds):
            log.info(".")
            sleep(1)

    @staticmethod
    def _max_1_sec_wait(start_time: float) -> None:
        wait_time = 1.0 - (time() - start_time)  # test writing every second async
        if wait_time > 0:
            sleep(wait_time)

    def reset_zero_rpm(self):
        if not self.zero_rpm_enable_path.exists():
            log.info("Zero RPM Enable does not exist")
            return
        if self.args.test:
            log.info("TEST Resetting Zero RPM Enable")
            return
        try:
            log.info("Resetting Zero RPM Enable")
            self.zero_rpm_enable_path.write_text("r\n")
        except Exception as e:
            log.error(f"Error resetting Zero RPM Enable: {e}")

    def set_zero_rpm(self, enable: bool):
        if not self.zero_rpm_enable_path.exists():
            log.info("Zero RPM Enable does not exist")
            return
        hwmon_enable = int(enable)  # 0 or 1
        if self.args.test:
            log.debug(f"TEST Setting zero RPM Enable: {enable}")
            return
        try:
            self.zero_rpm_enable_path.write_text(f"{hwmon_enable}\n")
            log.info(f"Set Zero RPM Enable:{enable}")
        except Exception as e:
            log.error(f"Error setting zero RPM Enable: {hwmon_enable}; Error: {e}")

    def commit_zero_rpm_changes(self):
        if not self.zero_rpm_enable_path.exists():
            log.info("Zero RPM Enable does not exist")
            return
        if self.args.test:
            log.info("TEST Committing Zero RPM Enable")
            return
        try:
            self.zero_rpm_enable_path.write_text("c\n")
        except Exception as e:
            log.error(f"Error committing new Zero RPM Enable: {e}")

    def reset_zero_rpm_stop_temp(self):
        if not self.zer_rpm_stop_temp_path.exists():
            log.info("Zero RPM Stop Temp does not exist")
            return
        if self.args.test:
            log.info("TEST Resetting Zero RPM Stop Temp")
            return
        try:
            log.info("Resetting Zero RPM Stop Temp")
            self.zer_rpm_stop_temp_path.write_text("r\n")
        except Exception as e:
            log.error(f"Error resetting Zero RPM Stop Temp: {e}")

    def set_zero_rpm_stop_temp_lowest(self):
        if not self.zer_rpm_stop_temp_path.exists():
            log.info("Zero RPM Stop Temp does not exist")
            return
        lowest_temp = self.temp_min
        for line in self.zer_rpm_stop_temp_path.read_text().splitlines():
            temp_match = re.match(
                r"ZERO_RPM_STOP_TEMPERATURE:\s+(?P<temp_min>\d+)\s+(?P<temp_max>\d+)",
                line,
            )
            if temp_match is not None:
                lowest_temp = int(temp_match.group("temp_min"))
                break
        if self.args.test:
            log.debug(f"TEST Setting zero RPM Stop Temp: {lowest_temp}")
            return
        try:
            self.zer_rpm_stop_temp_path.write_text(f"{lowest_temp}\n")
            log.info(f"Set Zero RPM Stop Temp:{lowest_temp}")
        except Exception as e:
            log.error(f"Error setting zero RPM Stop Temp: {lowest_temp}; Error: {e}")

    def set_zero_rpm_stop_temp_highest(self):
        if not self.zer_rpm_stop_temp_path.exists():
            log.info("Zero RPM Stop Temp does not exist")
            return
        highest_temp = self.temp_max
        for line in self.zer_rpm_stop_temp_path.read_text().splitlines():
            temp_match = re.match(
                r"ZERO_RPM_STOP_TEMPERATURE:\s+(?P<temp_min>\d+)\s+(?P<temp_max>\d+)",
                line,
            )
            if temp_match is not None:
                highest_temp = int(temp_match.group("temp_max"))
                break
        if self.args.test:
            log.debug(f"TEST Setting zero RPM Stop Temp: {highest_temp}")
            return
        try:
            self.zer_rpm_stop_temp_path.write_text(f"{highest_temp}\n")
            log.info(f"Set Zero RPM Stop Temp:{highest_temp}")
        except Exception as e:
            log.error(f"Error setting zero RPM Stop Temp: {highest_temp}; Error: {e}")

    def commit_zero_rpm_stop_temp_changes(self):
        if not self.zer_rpm_stop_temp_path.exists():
            log.info("Zero RPM Stop Temp does not exist")
            return
        if self.args.test:
            log.info("TEST Committing Zero RPM Stop Temp Changes")
            return
        try:
            self.zero_rpm_enable_path.write_text("c\n")
        except Exception as e:
            log.error(f"Error committing new Zero RPM Stop Temp Changes: {e}")


def log_line_filler():
    log.info(
        "####################################################################################"
    )


def reset_all(test: RDNA4Test):
    log_line_filler()
    log.info("Resting fan curve settings to default settings")
    log_line_filler()
    test.reset_fan_curve()
    test.reset_zero_rpm()
    test.reset_zero_rpm_stop_temp()
    test.wait_for_fan_stabilization()
    test.read_sensors()


def static_curve_single_commit(test: RDNA4Test, duty: int):
    reset_all(test)
    log_line_filler()
    log.info(f"Applying flat simple {duty}% fan curve - single commit")
    log_line_filler()
    test.set_zero_rpm(False)
    test.apply_flat_simple_fan_curve(duty)
    test.commit_fan_curve_changes()
    test.wait_for_fan_stabilization()
    test.read_sensors()


def static_curve_batched_commits(test: RDNA4Test, duty: int):
    reset_all(test)
    log_line_filler()
    log.info(f"Applying flat simple {duty}% fan curve - batched commits")
    log_line_filler()
    test.set_zero_rpm(False)
    test.apply_flat_simple_fan_curve(duty)
    test.commit_fan_curve_changes()
    test.commit_zero_rpm_changes()
    test.wait_for_fan_stabilization()
    test.read_sensors()


def static_curve_separate_commits(test: RDNA4Test, duty: int):
    reset_all(test)
    log_line_filler()
    log.info(f"Applying flat simple {duty}% fan curve - separate commits")
    log_line_filler()
    test.set_zero_rpm(False)
    test.commit_zero_rpm_changes()
    test.apply_flat_simple_fan_curve(duty)
    test.commit_fan_curve_changes()
    test.wait_for_fan_stabilization()
    test.read_sensors()


def force_zero_rpm(test: RDNA4Test):
    reset_all(test)
    log_line_filler()
    log.info("Forcing Zero RPM on fan")
    log_line_filler()
    # using "safe" batch-style commits
    test.set_zero_rpm(True)
    test.set_zero_rpm_stop_temp_highest()
    test.commit_zero_rpm_changes()
    test.commit_zero_rpm_stop_temp_changes()
    test.wait_for_fan_stabilization()
    test.read_sensors()


def force_zero_rpm_with_curve(test: RDNA4Test):
    reset_all(test)
    log_line_filler()
    log.info("Forcing Zero RPM with 50% Static Curve present")
    log_line_filler()
    # using "safe" batch-style commits
    test.apply_flat_simple_fan_curve(50)
    test.set_zero_rpm(True)
    test.set_zero_rpm_stop_temp_highest()
    test.commit_fan_curve_changes()
    test.commit_zero_rpm_changes()
    test.commit_zero_rpm_stop_temp_changes()
    test.wait_for_fan_stabilization()
    test.read_sensors()


def force_zero_rpm_with_curve_separate_commits(test: RDNA4Test):
    reset_all(test)
    log_line_filler()
    log.info("Forcing Zero RPM with 50% Static Curve present and separate commits")
    log_line_filler()
    test.apply_flat_simple_fan_curve(50)
    test.commit_fan_curve_changes()
    test.set_zero_rpm(True)
    test.commit_zero_rpm_changes()
    test.set_zero_rpm_stop_temp_highest()
    test.commit_zero_rpm_stop_temp_changes()
    test.wait_for_fan_stabilization()
    test.read_sensors()


def main():
    log_line_filler()
    log.info(f"Starting RDNA3/4 test v{__VERSION__}")
    log_line_filler()
    test = RDNA4Test()
    test.read_sensors()

    static_curve_single_commit(test, 25)
    static_curve_single_commit(test, 15)
    static_curve_single_commit(test, 10)
    static_curve_single_commit(test, 0)

    static_curve_batched_commits(test, 25)
    static_curve_batched_commits(test, 15)
    static_curve_batched_commits(test, 10)
    static_curve_batched_commits(test, 0)

    static_curve_separate_commits(test, 25)
    static_curve_separate_commits(test, 15)

    force_zero_rpm(test)
    force_zero_rpm_with_curve(test)
    force_zero_rpm_with_curve_separate_commits(test)

    # Done
    reset_all(test)
    log_line_filler()
    log.info("Testing Complete")
    log_line_filler()
    log.info("Output saved to rdna4_test.log")
    log.info("Thank you for testing for CoolerControl RDNA3/4 support!")


if __name__ == "__main__":
    main()
