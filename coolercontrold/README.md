# coolercontrold

is the main daemon containing the core logic for interfacing with devices, and installed as
"coolercontrold". It is meant to run in the background as a system daemon. It handles all device
communication and data management, additionally connecting to the liqctld daemon for liquidctl
supported devices. It has an API that services client programs like the coolercontrol-gui.
