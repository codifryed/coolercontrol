[![GPLv3 License](https://img.shields.io/badge/License-GPL%20v3-blue.svg?logo=gnu)](https://opensource.org/licenses/)
[![Gitlab pipeline status](https://img.shields.io/gitlab/pipeline-status/codifryed/coolero?branch=main&label=pipeline&logo=gitlab)](https://gitlab.com/codifryed/coolero/pipelines)
[![GitLab Release (latest by SemVer)](https://img.shields.io/gitlab/v/release/30707566?sort=semver&logo=gitlab)](https://gitlab.com/codifryed/coolero/pipelines)
[![Discord](https://img.shields.io/badge/_-online-_?label=&logo=discord&logoColor=ffffff&color=7389D8&labelColor=6A7EC2)](https://discord.gg/MbcgUFAfhV)
[![Linux](https://img.shields.io/badge/_-linux-blue?logo=linux&logoColor=fff)]()
[![Python](https://img.shields.io/badge/_-python-blue?logo=python&logoColor=fff)]()

# Coolero

is a program to monitor and control your cooling devices.  
It offers a easy-to-use user interface with various control features and also provides live thermal performance details.

It uses libraries like
[liquidctl](https://github.com/liquidctl/liquidctl) and others with a focus on cooling control under Linux.  
Written in [Python](https://www.python.org/) it uses [PySide](https://wiki.qt.io/Qt_for_Python) for the UI
and [Poetry](https://python-poetry.org/) for dependency management.

This project is currently in active development and slowly working it's way towards it's first major release.  
Testers welcome!

## Features

- System Overview Graph - choose what to focus on and see the effects of your configuration changes live.
- Supports multiple devices and multiple versions of the same device.
- Internal profile scheduling - create speed profiles based on CPU, GPU or other device sensors that don't natively
  support that feature.
- A modern custom UI
- Goal: Support most of the devices [liquidctl supports](https://github.com/liquidctl/liquidctl#supported-devices)
- Load last used profiles at startup**
- Other integrations, like thinkpad_acpi and lm_sensors to be able to control additional cooling devices**

_**In progess_

## Demo

![Demo](screenshots/coolero-demo.gif)

## Supported Devices:

Some devices are only partially supported or considered experimental,
see [liquidctl](https://github.com/liquidctl/liquidctl#supported-devices) for more specifics.
*More coming!*

| Name                                  | Cooling            | Lighting | Notes                           |
|---------------------------------------|--------------------|----------|---------------------------------|
| NZXT Kraken Z (Z53, Z63 or Z73)       | :heavy_check_mark: |          |                                 |
| NZXT Kraken X (X53, X63 or X73)       | :heavy_check_mark: |          |                                 |
| NZXT Kraken X (X42, X52, X62 and X72) | :heavy_check_mark: |          |                                 |
| NZXT Kraken M22                       |                    |          | <sup>Lighting only device</sup> |
| NZXT HUE 2, HUE 2 Ambient             |                    |          | <sup>Lighting only device</sup> |
| NZXT Smart Device V2                  | :heavy_check_mark: |          |                                 |
| NZXT RGB & Fan Controller             | :heavy_check_mark: |          |                                 |
| NZXT Smart Device                     | :heavy_check_mark: |          |                                 |
| NZXT Grid+ V3                         | :heavy_check_mark: |          |                                 |
| Corsair Commander Pro                 | :heavy_check_mark: |          |                                 |
| Corsair Obsidian 1000D                | :heavy_check_mark: |          |                                 |
| Corsair Lighting Node Core, Pro       |                    |          | <sup>Lighting only device</sup> |

_*Lighting is a WIP_

## Installation

Installation is currently supported by AppImage, Flatpak and from Source

### AppImage:

[![AppImageDownload](screenshots/download-appimage-banner.svg)](https://coolero.org/releases/latest/Coolero-x86_64.AppImage)  
Use the above link or goto the [Releases](https://gitlab.com/codifryed/coolero/-/releases) page and download the latest
AppImage.  
The AppImage contains all the needed dependencies. Just make it executable and run it:

```bash
chmod +x Coolero-x86_64.AppImage
./Coolero-x86_64.AppImage
```

It's recommended to turn on 'Check for updates' in Settings, which is disabled by default. Coolero will then ask if you
want to update it if a newer version is available.

<details>
<summary>Click for more info about AppImages</summary>

<a href="https://appimage.org/">AppImage Website</a><br>

For improved desktop integration:
<ul>
    <li><a href="https://github.com/TheAssassin/AppImageLauncher">AppImageLauncher</a></li>
    <li><a href="https://github.com/probonopd/go-appimage/blob/master/src/appimaged/README.md">appimaged</a></li>
</ul>
</details>

### Flatpak:

You can checkout the [Coolero page on Flathub](https://flathub.org/apps/details/org.coolero.Coolero)

or install with the command line:

```commandline
flatpak install org.coolero.Coolero
```

### Source:

<details>
<summary>Click to view</summary>

#### Requirements:

* Linux
* [Python 3.9](https://www.python.org/)
    * including the python3.9-dev package (may already be installed)
* System packages:
    * Ubuntu:
      ```sudo apt install libusb-1.0-0 curl python3-virtualenv python3.9-venv build-essential libgl1-mesa-dev```
    * Fedora:
      ```sudo dnf install libusbx curl python3-virtualenv mesa-libGL-devel && sudo dnf groupinstall "C Development Tools and Libraries"```
    * More specifically:
        * LibUSB 1.0 (libusb-1.0, libusb-1.0-0, or libusbx from your system package manager)
        * curl
        * python3-virtualenv  (or python3.9-virtualenv)
        * python3-venv  (or python3.9-venv)
        * Packages needed to build Qt applications:
            * build-essential
            * libgl1-mesa-dev
* [Poetry](https://python-poetry.org/) -
    * run `curl -sSL https://raw.githubusercontent.com/python-poetry/poetry/master/install-poetry.py | python3 -`
    * run `poetry --version` to make sure poetry works
    * if needed, add `$HOME/.local/bin` to your PATH to execute poetry easily - `export PATH=$HOME/.local/bin:$PATH`
    * if Python 3.9 is not your default python installation, then run `poetry env use python3.9` in the project
      directory to give poetry access

#### How:

* Clone the Repo `git clone git@gitlab.com:codifryed/coolero.git`
* Install the dependencies:
    ```bash
    cd coolero
    poetry install
    ```
* run it: `poetry run coolero`

</details>

## Debugging

*this will produce quite a bit of debug output to the command line and a rotating log file under /tmp

#### AppImage:

`./Coolero-x86_64.AppImage --debug`

#### Flatpak:

`flatpak run org.coolero.Coolero --debug`

#### From Source:

`poetry run coolero --debug`

## Acknowledgements

* Major thanks is owed to the python API of [liquidctl](https://github.com/liquidctl/liquidctl)
* A major influence is [GKraken](https://gitlab.com/leinardi/gkraken) written by Roberto Leinardi.
* UI based on [PyOneDark](https://github.com/Wanderson-Magalhaes/PyOneDark_Qt_Widgets_Modern_GUI) by Wanderson M.Pimenta

## License

This program is licensed under [GPLv3](COPYING.txt)  
also see [the copyright notice](COPYRIGHT.md)

## FAQ

- Should I use Liquid or CPU as a temperature source to control my pump/fans?
    - Quick answer: Liquid
    - The thermodynamics of liquid cooling are very different compared to the traditional method. Choose what works best
      for your situation.
