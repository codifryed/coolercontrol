[![GPLv3 License](https://img.shields.io/badge/License-GPL%20v3-blue.svg?logo=gnu)](https://opensource.org/licenses/)
[![Gitlab pipeline status](https://gitlab.com/codifryed/coolero/badges/main/pipeline.svg)](https://gitlab.com/codifryed/coolero/-/commits/main)
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

- System Overview Graph - choose what to focus on and see the effects of your configuration changes live and over time.
- Supports multiple devices and multiple versions of the same device.
- Internal profile scheduling - create speed profiles based on CPU, GPU or other device sensors that don't natively
  support that feature.
- Last set profiles are saved and applied at startup
- A modern custom UI
- _Goal:_ Support most of the devices [liquidctl supports](https://github.com/liquidctl/liquidctl#supported-devices)
- _In progress:_ Lighting control and other integrations, like thinkpad_acpi, and lm_sensors to be able to control
  additional cooling devices

## Demo

![Demo](screenshots/coolero-demo.gif)

## Current Supported Devices:

Some devices are only partially supported or considered experimental,
see [liquidctl](https://github.com/liquidctl/liquidctl#supported-devices) for more specifics.

| Name                                     | Cooling            | Lighting | Notes                           |
|------------------------------------------|--------------------|----------|---------------------------------|
| NZXT Kraken Z (Z53, Z63 or Z73)          | :heavy_check_mark: |          |                                 |
| NZXT Kraken X (X53, X63 or X73)          | :heavy_check_mark: |          |                                 |
| NZXT Kraken X (X42, X52, X62 and X72)    | :heavy_check_mark: |          |                                 |
| NZXT Kraken X31, X41, X61                | :heavy_check_mark: |          |                                 |
| NZXT Kraken X40, X60                     | :heavy_check_mark: |          | <sup>Experimental</sup>         |
| NZXT Kraken M22                          |                    |          | <sup>Lighting only device</sup> |
| NZXT HUE 2, HUE 2 Ambient                |                    |          | <sup>Lighting only device</sup> |
| NZXT Smart Device V2                     | :heavy_check_mark: |          |                                 |
| NZXT RGB & Fan Controller                | :heavy_check_mark: |          |                                 |
| NZXT Smart Device                        | :heavy_check_mark: |          |                                 |
| NZXT Grid+ V3                            | :heavy_check_mark: |          |                                 |
| EVGA CLC 120 (CL12), 240, 280, 360       | :heavy_check_mark: |          |                                 |
| Corsair Hydro v2 H80i, H100i, H115i      | :heavy_check_mark: |          |                                 |
| Corsair Hydro GT/GTX H80i, H100i, H110i  | :heavy_check_mark: |          | <sup>Experimental</sup>         |
| Corsair Commander Pro                    | :heavy_check_mark: |          |                                 |
| Corsair Obsidian 1000D                   | :heavy_check_mark: |          |                                 |
| Corsair Lighting Node Core, Pro          |                    |          | <sup>Lighting only device</sup> |

_*Lighting is a WIP_

## Installation

Installation is currently supported by AppImage, Flatpak and from Source

### AppImage:

[![AppImageDownload](screenshots/download-appimage-banner.svg)](https://gitlab.com/api/v4/projects/30707566/packages/generic/appimage/latest/Coolero-x86_64.AppImage)  
Use the above link or goto the [Releases](https://gitlab.com/codifryed/coolero/-/releases) page and download a specific
version.  
The AppImage contains all the needed dependencies. Just make it executable and run it:

```bash
chmod +x Coolero-x86_64.AppImage
./Coolero-x86_64.AppImage
```

It's recommended to turn on **Check for updates** in Settings, which is disabled by default. Coolero will then ask if
you want to update it if a newer version is available.

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

or install from the command line:

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

#### System packages:

* Ubuntu:
    ```bash
    sudo apt install libusb-1.0-0 curl python3-virtualenv python3.9-venv build-essential libgl1-mesa-dev
    ```
* Fedora:
    ```bash
    sudo dnf install libusbx curl python3-virtualenv mesa-libGL-devel && sudo dnf groupinstall "C Development Tools and Libraries"
    ```
* More specifically:
    * LibUSB 1.0 (libusb-1.0, libusb-1.0-0, or libusbx from your system package manager)
    * curl
    * python3-virtualenv  (or python3.9-virtualenv)
    * python3-venv  (or python3.9-venv)
    * Packages needed to build Qt applications:
        * build-essential
        * libgl1-mesa-dev

#### [Poetry](https://python-poetry.org/) -

* install:
    ```bash
    curl -sSL https://raw.githubusercontent.com/python-poetry/poetry/master/install-poetry.py | python3 -
    ```
* run: `poetry --version` to make sure poetry works
* if needed, add `$HOME/.local/bin` to your PATH to execute poetry easily:
    ```bash
    export PATH=$HOME/.local/bin:$PATH
    ```
* if Python 3.9 is not your default python installation, then run the following in the project directory to give poetry
  access:
    ```bash
    poetry env use python3.9
    ```

#### How:

* Clone the Repo:
    ```bash
    git clone git@gitlab.com:codifryed/coolero.git
    ```
* Install the dependencies from the newly created repo directory:
    ```bash
    poetry install
    ```
* run it:
    ```bash
    poetry run coolero
    ```

</details>

## Usage hints:

- Scroll or right click on the system overview to zoom the time frame
- clicking anywhere in the control graphs will apply the current settings. Changing any setting will apply it
  immediately.
- Check the settings page for some QoL options.

## Debugging

To help diagnose issues enabling debug output is invaluable. It will produce quite a bit of output from the different
internal systems to help determine what the cause might be. Output is sent to the command line (stdout) and to a
rotating log file under /tmp/coolero for convienence. Simply add the `--debug` option.

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
- My UDev rules are messed up, how do I apply them again?
    - run Coolero from the command line with `--add-udev-rules` to have them re-applied
- I have an issue with X, what do I do?
    - Please join the discord channel if it's something rather small, otherwise opening an Issue ticket in GitLab is the
      best way to get something fixed.
- How do I get Coolero to start automatically when I start my computer?
    - Each distro has their own way to do this, from a simple menu option 'Startup Applications' to writing your own
      script
