#!/usr/bin/env python3
"""
This script is an example of how to interact with the CoolerControl daemon via the REST API.
It can be uses as a reference for how to create your own scripts or a CLI tool.

Usage:
    cc.py -h | --help
    cc.py -l | --list
    cc.py -m <match> -c <channel> [--image <image>] [--password <password>] [--speed <speed>]
    cc.py -u <uid> -c <channel> [--image <image>] [--password <password>] [--speed <speed>]

Examples:
    Set a manual fan speed:
    ./cc.py -m kraken -c fan --speed 50

    Set a image on the LCD screen of a device:
    ./cc.py -m kraken -c lcd --image "/home/user/pictures/image.png"

    Set a manual fan speed by device UID:
    (This is useful if you have multiple devices with the same name)
    ./cc.py -u 123456-1234-0141234 -c fan --speed 50

    Activate a mode:
    ./cc.py --mode "Gaming"
"""

import argparse
import mimetypes
from pathlib import Path

import requests

# change this if the daemon is running on a different address:port
DAEMON_ADDRESS = "http://localhost:11987"


class CoolerControlCLI:
    def __init__(self):
        parser = argparse.ArgumentParser(description="Coolercontrol Scripting Helper")
        parser.add_argument(
            "-p",
            "--password",
            type=str,
            help="The password to use for authenticating with the daemon",
            default="coolAdmin",
        )
        parser.add_argument(
            "-l",
            "--list",
            action="store_true",
            help="List all devices with their UID, their control channels, and available modes",
        )
        parser.add_argument(
            "-m",
            "--match",
            type=str,
            help="Match the given string to the device to interact with",
        )
        parser.add_argument(
            "-u",
            "--device-uid",
            type=str,
            help="Match the given device unique identifier to the device to interact with",
        )
        parser.add_argument(
            "-c",
            "--channel",
            type=str,
            help="The device channel you want to control",
        )
        parser.add_argument(
            "-s",
            "--speed",
            type=str,
            help="A manual speed to set a fan channel to",
        )
        parser.add_argument(
            "-i",
            "--image",
            type=str,
            help="The absolute path to the image to set the LCD screen to",
        )
        parser.add_argument(
            "--mode",
            type=str,
            help="The CoolerControl Mode the activate",
        )
        self.args = parser.parse_args()
        self.req = requests.Session()  # Session to hold the login state
        self.device_uid: str | None = None
        self.channel_name: str | None = None

    def list_devices_and_modes(self):
        response = self.req.get(f"{DAEMON_ADDRESS}/devices")
        response.raise_for_status()
        for device in response.json().get("devices"):
            print(
                f"""{device.get('name')}
    {device.get("uid")}
================================================================"""
            )
            for channel_name in device.get("info").get("channels").keys():
                print(f"  {channel_name}")
            print()
        response = self.req.get(f"{DAEMON_ADDRESS}/modes")
        response.raise_for_status()
        print(
            """Modes:
================================================================"""
        )
        for mode in response.json().get("modes"):
            print(f"{mode.get('name')}")

    def login(self, password: str):
        response = self.req.post(f"{DAEMON_ADDRESS}/login", auth=("CCAdmin", password))
        response.raise_for_status()

    def set_mode(self, input_mode: str):
        modes = self.req.get(f"{DAEMON_ADDRESS}/modes").json().get("modes")
        selected_mode: str | None = None
        for mode in modes:
            if mode.get("name").casefold() == input_mode.casefold():
                selected_mode = mode
                break
        if selected_mode is None:
            print(f"No mode found matching '{input_mode}'")
            exit(1)
        response = self.req.post(
            f"{DAEMON_ADDRESS}/modes-active/{selected_mode.get('uid')}",
        )
        response.raise_for_status()

    def verify_device_and_channel(self):
        if (
            self.args.match is None and self.args.device_uid is None
        ) or self.args.channel is None:
            print("You must provide a device and channel match string")
            exit(1)

    def find_device_and_channel(self, match: str | None, uid: str | None, channel: str):
        all_devices = self.req.get(f"{DAEMON_ADDRESS}/devices").json().get("devices")
        dev: dict = None
        if match is not None:
            for device in all_devices:
                if match.casefold() in device.get("name").casefold():
                    dev = device
                    self.device_uid = device.get("uid")
        elif uid is not None:
            for device in all_devices:
                if uid == device.get("uid"):
                    dev = device
                    self.device_uid = device.get("uid")
        if self.device_uid is None:
            print(f"No device found matching '{match}' or '{uid}'")
            exit(1)
        for channel_key in dev.get("info").get("channels").keys():
            if channel.casefold() in channel_key.casefold():
                self.channel_name = channel_key
                break
        if self.channel_name is None:
            print(f"No channel found matching '{channel}'")
            exit(1)

    def set_speed(self, speed: int):
        response = self.req.put(
            f"{DAEMON_ADDRESS}/devices/{self.device_uid}/settings/{self.channel_name}/manual",
            json={"speed_fixed": speed},
        )
        response.raise_for_status()

    def set_lcd_image(self, image_path: str):
        """
        Set LCD Screen image
        This request is a bit more complex compared to the others
        """
        file_name: str = Path(image_path).name
        mime_content_type, _ = mimetypes.guess_type(file_name)
        image_settings = [
            ("mode", "image"),
            ("brightness", 100),
            ("orientation", 0),
            ("images[]", (file_name, open(image_path, "rb"), mime_content_type)),
        ]
        response = self.req.put(
            f"{DAEMON_ADDRESS}/devices/{self.device_uid}/settings/{self.channel_name}/lcd/images",
            files=image_settings,
        )
        response.raise_for_status()

    def run(self):
        if self.args.list:
            self.list_devices_and_modes()
            exit(0)
        self.login(self.args.password)
        if self.args.mode:
            self.set_mode(self.args.mode)
        else:
            self.verify_device_and_channel()
            self.find_device_and_channel(
                self.args.match, self.args.device_uid, self.args.channel
            )
            if self.args.speed is not None:
                self.set_speed(int(self.args.speed))
            elif self.args.image is not None:
                self.set_lcd_image(self.args.image)


if __name__ == "__main__":
    CoolerControlCLI().run()
