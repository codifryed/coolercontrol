[![GPLv3 License](https://img.shields.io/badge/License-GPL%20v3-yellow.svg)](https://opensource.org/licenses/)
[![pipeline status](https://gitlab.com/codifryed/coolero/badges/master/pipeline.svg)](https://gitlab.com/codifryed/coolero/pipelines)

# Coolero

is a program to monitor and control your cooling and other devices.  
It's fundamentally a GUI wrapper for some great cli libraries such
as [liquidctl](https://github.com/liquidctl/liquidctl) and others with a focus on cooling control under Linux.  
Written in [Python](https://www.python.org/) 3.9, it uses [PySide](https://wiki.qt.io/Qt_for_Python) for the UI,
and [Poetry](https://python-poetry.org/) for dependency management.

This project is currently in active development and is slowly working it's way towards an initial stable release.
Testers welcome!

## Screenshots

![Open Overview](screenshots/open-overview.png)
![Speed Channel](screenshots/speed-channel.png)
![Overview Customer Profile](screenshots/overview-custom-profile.png)

## Supported Devices:

| Name | Cooling | Lighting | Notes |
|------|---------|----------|-------|
| NZXT Kraken X53, X63, X73 | X |  | |

## Installation

### From Source:

Installing from source is currently the only supported method. Packaging and other methods are on their way.

#### Requirements:

* Linux
* [Python 3.9](https://www.python.org/)
* LibUSB 1.0 (libusb-1.0, libusb-1.0-0, or libusbx from your system package manager)
* [Poetry](https://python-poetry.org/) -
    * Make sure `python` is symlinked to your python3 installation
    * run `curl -sSL https://raw.githubusercontent.com/python-poetry/poetry/master/install-poetry.py | python -`

#### How:

* Clone the Repo `git clone git@gitlab.com:codifryed/coolero.git`
* Install the dependencies:
    ```bash
    cd coolero
    poetry install
    ```
* run it: `poetry run coolero`

## Debuging

`poetry run coolero --debug`
*this will produce a lot of debug output

## Credits

* A major inspiration and where this projects stems from is [GKraken](https://gitlab.com/leinardi/gkraken) written by
  Roberto Leinardi. This project started out as a rewrite of GKraken to be able to support a many more devices and
  configurations.
* UI based on [PyOneDark](https://github.com/Wanderson-Magalhaes/PyOneDark_Qt_Widgets_Modern_GUI) by Wanderson M.Pimenta

## License

This program is licensed under [GPLv3](COPYING.txt)  
also see [the copyright notice](COPYRIGHT.md)