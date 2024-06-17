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

log_format = "%(asctime)-15s %(levelname)-8s %(name)s - %(message)s"
logging.basicConfig(
    level=logging.INFO,
    format=log_format,
    handlers=[logging.FileHandler("rdna3_lact_test.log"), logging.StreamHandler()],
)
log = logging.getLogger("CoolerControl-RDNA3")

__VERSION__ = "0.0.1"


class RDNA3LACTTest:
    stabilization_wait_time: int = 5

    def __init__(self):
        parser = argparse.ArgumentParser(
            description="A CoolerControl test script for RDNA3 sysfs fan control",
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
        self.hwmon_path: Path = self.find_amdgpu_hwmon_path()
        self.device_path: Path = self.get_device_path()
        self.fan_curve_path: Path = self.get_fan_curve_path()
        if not self.fan_curve_path.exists():
            log.error(f"fan_curve file not found in {self.fan_curve_path}")
            sys.exit(1)
        self.temp_min: int = -1
        self.temp_max: int = -100
        self.duty_min: int = -1
        self.duty_max: int = -100
        self.fan_curve_size: int = 0

    @staticmethod
    def find_amdgpu_hwmon_path() -> Path:
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
        device_path = (self.hwmon_path / "device").resolve()
        log.info(f"Device path: {device_path}")
        return device_path

    def get_fan_curve_path(self) -> Path:
        if self.args.test:
            return Path("rdna3_data") / "rx7900xt" / "gpu_od" / "fan_ctrl" / "fan_curve"
        else:
            return self.device_path / "gpu_od" / "fan_ctrl" / "fan_curve"

    def read_sensors(self) -> None:
        log.info("--------------------------------------------------")
        log.info("READING SYSFS DATA:")
        log.info("--------------------------------------------------")
        self.print_pwm_fan_speed()
        self.print_fan_rpm()
        self.print_temps()

        #  output the fan_curve contents
        fan_curve_points = self.get_fan_curve()
        log.info(f"Fan Curve Points: {fan_curve_points}")
        self.fan_curve_size = self.determine_fan_curve_size(fan_curve_points)
        self.determine_fan_curve_limits()
        log.info(
            f"Fan Curve Limits: "
            f"Temp({self.temp_min}-{self.temp_max}) "
            f"Duty({self.duty_min}-{self.duty_max})"
        )
        log.info("--------------------------------------------------\n")

    def print_pwm_fan_speed(self) -> None:
        pwm_file = self.hwmon_path / "pwm1"
        if pwm_file.exists():
            raw_pwm = int(pwm_file.read_text())
            log.info(f"PWM value(0-255): {raw_pwm}")
            pwm_writable = os.access(pwm_file, os.W_OK)
            log.info(f"PWM File writable: {pwm_writable}")
        else:
            log.warning("pwm1 file not found")

    def print_fan_rpm(self):
        input_file = self.hwmon_path / "fan1_input"
        if input_file.exists():
            raw_input = int(input_file.read_text())
            log.info(f"RPM value: {raw_input}")
        else:
            log.warning("fan1_input file not found")

    def print_temps(self):
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
    def determine_fan_curve_size(fan_curve_points: list[(int, int)]) -> int:
        return len(fan_curve_points)

    def determine_fan_curve_limits(self):
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

    @classmethod
    def wait_for_fan_stabilization(cls, seconds: int | None = None) -> None:
        if seconds is None:
            seconds = cls.stabilization_wait_time
        for _ in range(seconds):
            log.info(".")
            sleep(1)

    @staticmethod
    def max_1_sec_wait(start_time: float) -> None:
        wait_time = 1.0 - (time() - start_time)  # test writing every second async
        if wait_time > 0:
            sleep(wait_time)


def main():
    log.info("##################################################")
    log.info(f"Starting RDNA3 LACT test v{__VERSION__}")
    log.info("##################################################")
    test = RDNA3LACTTest()
    test.read_sensors()

    log.info("##################################################")
    log.info("Please open up LACT and set a static fan speed to: 30%")
    log.info("Then press enter to continue")
    input()
    test.wait_for_fan_stabilization()
    test.read_sensors()

    log.info("##################################################")
    log.info("Please open up LACT and set a static fan speed to: 10%")
    log.info("Then press enter to continue")
    input()
    test.wait_for_fan_stabilization()
    test.read_sensors()

    log.info("##################################################")
    log.info("Please open up LACT and set a static fan speed to: 0%")
    log.info("Then press enter to continue")
    input()
    test.wait_for_fan_stabilization()
    test.read_sensors()

    log.info("Testing Complete")
    log.info("##################################################")
    log.info("Output saved to rdna3_lact_test.log")
    log.info("Thank you for testing for CoolerControl RDNA3 support")


if __name__ == "__main__":
    main()
