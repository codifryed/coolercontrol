#!/usr/bin/python3

import argparse

import requests

DAEMON_ADDRESS = "http://localhost:11987"


# CLI Setup
####################################################################################################

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
    help="List all devices and their control channels",
)
parser.add_argument(
    "-m",
    "--match",
    type=str,
    help="Match the given string to the device to interact with",
)
parser.add_argument(
    "-c",
    "--channel",
    type=str,
    help="Match the given string to the device control channel to interact with",
)
args = parser.parse_args()

# create a session to hold on to the authorized session cookie
req = requests.Session()

# List all devices and their channels if requested
if args.list:
    response = req.get(f"{DAEMON_ADDRESS}/devices")
    response.raise_for_status()
    devices = response.json().get("devices")
    for device in devices:
        print(f"{device.get('name')}\n=========================")
        for channel_name in device.get("info").get("channels").keys():
            print(f"  {channel_name}")
        print()
    exit(0)

if args.match is None or args.channel is None:
    print("You must provide a device and channel match string")
    exit(1)

# login needed to change any setting
response = req.post(f"{DAEMON_ADDRESS}/login", auth=("CCAdmin", args.password))
response.raise_for_status()

all_devices = req.get(f"{DAEMON_ADDRESS}/devices").json().get("devices")

device_uid: str | None = None
channel_name: str = "none"
for device in all_devices:
    if args.match.casefold() in device.get("name").casefold():
        device_uid = device.get("uid")
        for channel_name in device.get("info").get("channels").keys():
            if args.channel.casefold() in channel_name.casefold():
                channel_name = channel_name
                break

if device_uid is None:
    print(f"No device found matching '{args.match}'")
    exit(1)

####################################################################################################
# From here you can adjust the script to do what you would like

# LCD Screen: This is one of the most complex requests.
print(f"Setting the LCD image for device '{args.match}' channel '{channel_name}'")
# image_path = "/home/user/pictures/image.png"
image_path = "/home/theguy/Pictures/Zgifs/error.gif"
image_settings = [
    ("mode", "image"),
    ("brightness", 100),
    ("orientation", 0),
    # adjust MIME content type per your image format:
    ("images[]", ("image.gif", open(image_path, "rb"), "image/gif")),
]
response = req.put(
    f"{DAEMON_ADDRESS}/devices/{device_uid}/settings/{channel_name}/lcd/images",
    files=image_settings,
)
response.raise_for_status()
