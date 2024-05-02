# Scripting Examples

CoolerControl has an extensive REST API that the UI uses to communicate with the Daemon.

Scripts and other programs can also use this API to extend or automate certain flows as the user
sees fit. In the future there may be more official CLI helpers and an official OpenAPI
specification, but until then this directory contains some basic scripts to help users get started
with writing their own.

## Python script examples

You need to have the Python3 `requests` library installed. It might already be installed but if not
then there are several ways to do this depending on your distribution.

1. Install the system package, which is often called: `python3-requests`
2. Install using pip: `python3 -m pip install requests`

List all devices, channels, and modes:

```bash
./cc.py -l
```

Set LCD screen image:

```bash
./cc.py -m kraken -c lcd --image /home/user/pictures/images.gif
```

View the `cc.py` script for examples and information.
