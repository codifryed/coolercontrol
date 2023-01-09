[![GPLv3 License](https://img.shields.io/badge/License-GPL%20v3-blue.svg?logo=gnu)](https://opensource.org/licenses/)
[![Gitlab pipeline status](https://gitlab.com/coolero/coolero/badges/main/pipeline.svg)](https://gitlab.com/coolero/coolero/-/commits/main)
[![GitLab Release (latest by SemVer)](https://img.shields.io/gitlab/v/release/30707566?sort=semver&logo=gitlab)](https://gitlab.com/coolero/coolero/pipelines)
[![Discord](https://img.shields.io/badge/_-online-_?label=&logo=discord&logoColor=ffffff&color=7389D8&labelColor=6A7EC2)](https://discord.gg/MbcgUFAfhV)
[![Linux](https://img.shields.io/badge/_-linux-blue?logo=linux&logoColor=fff)]()
[![Python](https://img.shields.io/badge/_-python-blue?logo=python&logoColor=fff)]()
[![Rust](https://img.shields.io/badge/_-rust-orange?logo=rust&logoColor=fff)]()
[![Contributor Covenant](https://img.shields.io/badge/Contributor%20Covenant-2.1-4baaaa.svg)](CODE_OF_CONDUCT.md)

# CoolerControl

is a program to monitor and control your cooling devices. It offers an easy-to-use user interface, a control daemon, and provides live
thermal performance details.

CoolerControl is a frontend and enhancement of [liquidctl](https://github.com/liquidctl/liquidctl)
and [hwmon](https://hwmon.wiki.kernel.org) with a focus on controlling cooling devices such as AIO coolers and fans under Linux.
Written in [Python](https://www.python.org/) and [Rust](https://www.rust-lang.org/) it uses [Poetry](https://python-poetry.org/)
and [Cargo](https://crates.io/) for dependency management and [PySide](https://wiki.qt.io/Qt_for_Python) for the UI.

This project is currently in active development and slowly working it's way towards it's first major release.

(Screenshots)

### Coolero

What happened to [Coolero](https://gitlab.com/coolercontrol/coolercontrol/-/tree/coolero)? Due to popular request the project name has been
changed. (Sorry about that) At the same time a new implementation has been introduced. Coolero was developed as a GUI first - a sandboxed
user-space application available as an AppImage or Flatpak that later added system-level helpers to extend functionality. CoolerControl is
primarily a SystemD service that runs as a background daemon first, available as a systems package and an AppImage, and still maintains the
convenience and control of the original GUI. In that sense, the UI is essentially the same, but the backend/engine of the application
has been completely rewritten. You can still use the Coolero packages if desired, but it is considered deprecated and no new features will
be added.

*__NOTE:__ _Your configuration settings from Coolero will unfortunately not transfer to CoolerControl. You'll need to input your desired
settings again. The reason for this is that the underlying implementation is so different, that there is no easy transfer method between
them. The best way is to use the Coolero export-profiles option: `coolero --export-profiles` and copy the settings that you want to
transfer. Then input them again in the CoolerControl GUI. Once this is complete you can create a backup of the toml configuration file
in `/etc/coolercontrol` that can be used and changed by you in the future._

This rewrite offers several enhancements over the previous implementation:

- Lots of little annoying bugs fixed
- Control settings are applied automatically on boot, not login
- Decoupled GUI - device control is done in the background and the GUI is used when you want to monitor performance
- Human-editable configuration files
- Can run on a headless server
- Improved multi-device communication
- System level packaging (deb, rpm, etc)
- Enables features that were previously difficult or impossible to do

## Contents

[[_TOC_]]

## Features

- System Overview Graph - choose what to focus on and see the effects of your configuration changes live and over time.
- Supports multiple devices and multiple versions of the same device.
- Internal fan profile scheduling - create fan curves based on any device temperature sensor.
- Last set profiles are automatically saved and applied on boot.
- Settings are re-applied after waking from sleep/hibernate.
- Fan curve profiles can be copied from one device to another.
- A modern custom UI.
- Supports most __liquidctl__ [supported devices](https://github.com/liquidctl/liquidctl#supported-devices).
- Supports usable __hwmon__ (lm-sensors)
  [supported devices](https://hwmon.wiki.kernel.org/device_support_status).

## Demo

![Demo](screenshots/coolercontrol-demo.gif)

## Current Supported Devices

_Note: Some devices are only partially supported or considered experimental_

| Name                                                                                                              | Notes                                                                                                                                                                                                                                                                                            |
|-------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| HWMon (lm-sensors, aka motherboard connected fans) [devices](https://hwmon.wiki.kernel.org/device_support_status) | <sup>[see doc](#hwmon-support)</sup>                                                                                                                                                                                                                                                             |
| NZXT Kraken Z (Z53, Z63 or Z73)                                                                                   | <sup>[experimental LCD support](https://github.com/liquidctl/liquidctl/blob/main/docs/kraken-x3-z3-guide.md)</sup>                                                                                                                                                                               |
| NZXT Kraken X (X53, X63 or X73)                                                                                   |                                                                                                                                                                                                                                                                                                  |
| NZXT Kraken X (X42, X52, X62 and X72)                                                                             |                                                                                                                                                                                                                                                                                                  |
| NZXT Kraken X31, X41, X61                                                                                         |                                                                                                                                                                                                                                                                                                  |
| NZXT Kraken X40, X60                                                                                              | <sup>[experimental](https://github.com/liquidctl/liquidctl/blob/main/docs/asetek-690lc-guide.md)</sup>                                                                                                                                                                                           |
| NZXT Kraken M22                                                                                                   | <sup>lighting only device</sup>                                                                                                                                                                                                                                                                  |
| NZXT HUE 2, HUE 2 Ambient                                                                                         | <sup>lighting only device</sup>                                                                                                                                                                                                                                                                  |
| NZXT Smart Device V2                                                                                              |                                                                                                                                                                                                                                                                                                  |
| NZXT H1 V2                                                                                                        | <sup>[experimental](https://github.com/liquidctl/liquidctl/blob/main/docs/nzxt-hue2-guide.md)</sup>                                                                                                                                                                                              |                                                                                                                               |
| NZXT RGB & Fan Controller                                                                                         |                                                                                                                                                                                                                                                                                                  |
| NZXT Smart Device                                                                                                 |                                                                                                                                                                                                                                                                                                  |
| NZXT Grid+ V3                                                                                                     |                                                                                                                                                                                                                                                                                                  |
| NZXT E500, E650, E850                                                                                             | <sup>[partial](https://github.com/liquidctl/liquidctl/blob/main/docs/nzxt-e-series-psu-guide.md)</sup>                                                                                                                                                                                           |
| Aquacomputer D5 Next                                                                                              | <sup>[experimental](https://github.com/liquidctl/liquidctl/blob/main/docs/aquacomputer-d5next-guide.md)</sup>                                                                                                                                                                                    |
| Aquacomputer Octo                                                                                                 | <sup>[experimental](https://github.com/liquidctl/liquidctl/blob/main/docs/aquacomputer-octo-guide.md)</sup>                                                                                                                                                                                      |
| Aquacomputer Quadro                                                                                               | <sup>[experimental](https://github.com/liquidctl/liquidctl/blob/main/docs/aquacomputer-quadro-guide.md)</sup>                                                                                                                                                                                    |
| Aquacomputer Farbwerk 360                                                                                         | <sup>[experimental](https://github.com/liquidctl/liquidctl/blob/main/docs/aquacomputer-farbwerk360-guide.md)</sup>                                                                                                                                                                               |
| Corsair Hydro GT/GTX H80i, H100i, H110i                                                                           | <sup>[experimental](https://github.com/liquidctl/liquidctl/blob/main/docs/asetek-690lc-guide.md)</sup>                                                                                                                                                                                           |
| Corsair Hydro v2 H80i, H100i, H115i                                                                               |                                                                                                                                                                                                                                                                                                  |
| Corsair Hydro Pro H100i, H115i, H150i                                                                             | <sup>pump speed is limited to 3 speeds and is set using duty % ranges</sup>                                                                                                                                                                                                                      |
| Corsair Hydro Platinum H100i, H100i SE, H115i                                                                     | <sup>pump speed is limited to 3 speeds and is set using duty % ranges</sup>                                                                                                                                                                                                                      |
| Corsair Hydro Pro XT H60i, H100i, H115i, H150i                                                                    | <sup>pump speed is limited to 3 speeds and is set using duty % ranges</sup>                                                                                                                                                                                                                      |
| Corsair iCUE Elite Capellix H100i, H115i, H150i                                                                   | <sup>[experimental](https://github.com/liquidctl/liquidctl/blob/main/docs/corsair-commander-core-guide.md)</sup>                                                                                                                                                                                 |
| Corsair Commander Pro                                                                                             |                                                                                                                                                                                                                                                                                                  |
| Corsair Commander Core                                                                                            | <sup>[experimental](https://github.com/liquidctl/liquidctl/blob/main/docs/corsair-commander-core-guide.md), has issues([1](https://github.com/liquidctl/liquidctl/issues/448), [2](https://github.com/liquidctl/liquidctl/pull/454), [3](https://github.com/liquidctl/liquidctl/pull/522))</sup> |
| Corsair Commander Core XT                                                                                         | <sup>[experimental](https://github.com/liquidctl/liquidctl/blob/main/docs/corsair-commander-core-guide.md), has issues([1](https://github.com/liquidctl/liquidctl/issues/448), [2](https://github.com/liquidctl/liquidctl/pull/454), [3](https://github.com/liquidctl/liquidctl/pull/522))</sup> |
| Corsair Obsidian 1000D                                                                                            |                                                                                                                                                                                                                                                                                                  |
| Corsair Lighting Node Core, Pro                                                                                   | <sup>lighting only device</sup>                                                                                                                                                                                                                                                                  |
| Corsair HX750i, HX850i, HX1000i, HX1200i                                                                          |                                                                                                                                                                                                                                                                                                  |
| Corsair RM650i, RM750i, RM850i, RM1000i                                                                           |                                                                                                                                                                                                                                                                                                  |
| EVGA CLC 120 (CL12), 240, 280, 360                                                                                |                                                                                                                                                                                                                                                                                                  |
| Gigabyte RGB Fusion 2.0                                                                                           | <sup>lighting only device</sup>                                                                                                                                                                                                                                                                  |
| ASUS Aura LED motherboards                                                                                        | <sup>lighting only device, [experimental](https://github.com/liquidctl/liquidctl/blob/main/docs/asus-aura-led-guide.md)</sup>                                                                                                                                                                    |

Your device isn't listed? See [Adding Device Support](#adding-device-support)

## Installation

Installation is currently supported by __Systems Package__, __AppImage__, the __AUR__, and from __Source__

### Systems Package

The system packages are compiled with the needed libraries and so should have very few system dependencies.

- Enable the service on boot: ```sudo systemctl enable coolercontrold@.service```
- Start the service: ```sudo systemctl start coolercontrold@.service```
- TODO...

1. Download package
2. install with package manager
3. start and enable systemd service
4. start coolercontrol

### AppImage

[![AppImageDownload](screenshots/download-appimage-banner.svg)](https://gitlab.com/api/v4/projects/30707566/packages/generic/appimage/latest/Coolercontrold-x86_64.AppImage)  
[![AppImageDownload](screenshots/download-appimage-banner.svg)](https://gitlab.com/api/v4/projects/30707566/packages/generic/appimage/latest/Coolercontrol-x86_64.AppImage)  
Use the above link or goto the [Releases](https://gitlab.com/coolercontrol/coolercontrol/-/releases) page and download a specific
version. There are two AppImages. `coolercontrold` which runs as a daemon in the background and needs sudo access, and `coolercontrol`
which can be run to start the GUI and needs a desktop environment to run. The AppImage is a good if you want to try things out without
installing anything. It is recommended to install the systems packages when applicable though, as it then installed as a systemd service
making it run automatically and is mostly hands-off.
The AppImages contain all the needed dependencies. Just make it executable and run it:

```bash
chmod +x CoolerControl-x86_64.AppImage
./CoolerControl-x86_64.AppImage
chmod +x Coolercontrold-x86_64.AppImage
./Coolercontrold-x86_64.AppImage
```

_Note: on some systems you'll have to install 'fuse' to make appimages work_

<details>
<summary>Click for more info about AppImages</summary>

<a href="https://appimage.org/">AppImage Website</a><br>

For improved desktop integration:
<ul>
    <li><a href="https://github.com/TheAssassin/AppImageLauncher">AppImageLauncher</a></li>
    <li><a href="https://github.com/probonopd/go-appimage/blob/master/src/appimaged/README.md">appimaged</a></li>
</ul>
</details>

### AUR

__Arch support is currently a WIP__
Use your installed AUR Helper, i.e.:

```commandline
yay -S coolercontrol
```

### Source

<details>
<summary>Click to view</summary>

#### Requirements

* Linux
* [Python 3.10](https://www.python.org/)
    * including the python3.10-dev package (may already be installed)
* Rust 1.66+

#### System Packages

* Ubuntu:
    ```bash
    sudo apt install libusb-1.0-0 curl python3-virtualenv python3.10-venv python3.10-dev build-essential libgl1-mesa-dev
    ```
* Fedora:
    ```bash
    sudo dnf install libusbx curl python3-virtualenv python3-devel mesa-libGL-devel && sudo dnf groupinstall "C Development Tools and Libraries"
    ```
* More specifically:
    * LibUSB 1.0 (libusb-1.0, libusb-1.0-0, or libusbx from your system package manager)
    * curl
    * python3-virtualenv  (or python3.10-virtualenv)
    * python3-venv  (or python3.10-venv)
    * Packages needed to build Qt applications:
        * build-essential
        * libgl1-mesa-dev

#### [Poetry](https://python-poetry.org/)

* install:
    ```bash
    curl -sSL https://raw.githubusercontent.com/python-poetry/poetry/master/install-poetry.py | python3 -
    ```
* run: `poetry --version` to make sure poetry works
* if needed, add `$HOME/.local/bin` to your PATH to execute poetry easily:
    ```bash
    export PATH=$HOME/.local/bin:$PATH
    ```
* if Python 3.10 is not your default python installation, then run the following in the project directory to give poetry
  access:
    ```bash
    poetry env use python3.10
    ```

#### CoolerControl Files

* The project is split into 3 main source directories:
    * coolercontrol-gui - The GUI written in Python
    * coolercontrol-liqctld - The liquidctl daemon written in Python
    * coolercontrold - The main service daemon written in Rust

* Clone the Repo:
    ```bash
    git clone git@gitlab.com:coolercontrol/coolercontrol.git
    ```
    * Install and run each service in order:
      ```bash 
      cd coolercontrol-liqctld
      poetry install
      poetry run liqctld  (todo: compile and run with sudo)
      ```
      ```bash
      cd coolercontrold
      cargo build --release
      sudo ./target/release/coolercontrold
      ```
      ```bash 
      cd coolercontrol-gui
      poetry install
      poetry run coolercontrol (todo: compile)
      ```
        - TODO: make install(compile all and copy to install dir) & install systemd files

</details>

## Post-Install Steps

- CoolerControl generally will detect supported devices and available capabilities automatically. The GUI will also prompt you for
  additional steps if necessary.
- To have access to all available hwmon supported devices & controls it's recommended to run `sensors-detect`. See
  the [Hwmon How To section](#How to).

## Usage Hints

- GUI
    - Scroll or right-click on the system overview to zoom the time frame.
    - Clicking anywhere in the control graphs will apply the current settings. Changing any setting will apply it immediately.
    - Check the info and settings pages in the GUI for some Quality of Life options.
- Configuration files:
    - daemon: `/etc/coolercontrol`
    - gui: `~/.config/coolercontrol`

## HWMon Support

Hwmon support comes with features that are similar to programs like [fancontrol](https://linux.die.net/man/8/fancontrol) and thinkfan. For
more info checkout the [HWMon wiki](https://hwmon.wiki.kernel.org/projectinformation). By default, all detected and usable fan/pump controls
are displayed.

### How To

- Optionally enable "Hwmon Temps" in the GUI to see all available and usable temp sensors
- **Highly Recommended:**
    - Install [lm-sensors](https://github.com/lm-sensors/lm-sensors) (lm_sensors) if not already installed. This is
      usually done through your distribution's package manager, i.e. apt, dnf, pacman.
        - run `sudo sensors-detect` at least once to make sure all available modules have been loaded.  
          *_In some rare cases your specified kernel module may need to be manually loaded_
    - restart coolercontrold: `systemctl restart coolercontrold@.service`

### Additional Info

- CoolerControl does not display all possible sensors and devices. It finds what is usable by the program and displays those.  
  The criteria are basically:
    - fans that are controllable
    - temperatures with reasonable values
    - devices that have sensors that meet those requirements.
- Setting a hwmon based speed profile to __'Default'__ will enable automatic mode for those fans - aka bios controlled.
- Some fans work in steps, like with the thinkpad, so the reported fan duty % will be the closest step to what one has set.
- Devices that are supported by Liquidctl will not be displayed additionally as Hwmon devices. This is because liquidctl offers many
  more features, such as lighting control, than what hwmon alone currently does. Also, liquidctl uses the hwmon interface by default if
  available.

### Known Issues

- The system overview graph will freak out if the sensor list is longer than the current window size can display. Please make the window
  larger and the graph will fix itself.

## CLI Arguments

- `-h, --help`: show available commands
- `-v, --version`: show program, system, and dependency version information
- `--debug`: turn on debug output to console, journalctl, and in the case of the GUI, a rotating log file
  under `/tmp/coolercontrol/coolercontrol.log`
- `--debug-liquidctl`: same as above but explicitly for liquidctl output _*daemon only_

## Debugging

To help diagnose issues enabling debug output is invaluable. It will produce a lot of output from the different internal systems to help
determine what the cause for a particular issue might be. Output is sent to the command line (stdout), for systemd service to the system
journal, journalctl, and in the case of the GUI, to a rotating temporary log file under `/tmp/coolercontrol/coolercontrol.log`. Simply add
the `--debug` option when starting the programs:

```bash
sudo systemctl restart coolercontrold@"--debug".service
coolercontrol --debug
```

#### AppImage

```
./Coolercontrold-x86_64.AppImage --debug
./Coolercontrol-x86_64.AppImage --debug
```

## Liquidctl Debugging

Liquidctl is an essential library for CoolerControl, so if you notice an issue related to liquidctl - reporting problems is an
easy and very valuable way to contribute to the project. Please check the existing [issues](https://github.com/liquidctl/liquidctl/issues)
and, if none matches your problem, use the appropriate template to create
a [new issue](https://github.com/liquidctl/liquidctl/issues/new/choose). When submitting an issue it's best to use the liquidctl CLI, or as
an alternative, use the coolercontrol `--debug-liquidctl` option for liquidctl debug
output:

```bash 
sudo systemctl restart coolercontrold@"--debug-liquidctl".service
```

## Adding Device Support

Support for new devices requires help from the community. CoolerControl is essentially a frontend for various "backend"
libraries. This means CoolerControl does not interact with the devices directly, but through the API of other systems or libraries. The two
currently supported backends are liquidctl and hwmon. Adding support for more devices generally means being supported in one of these
backends first. These are the steps to take to add support for your device in CoolerControl:

1. Is your device supported by liquidctl?
    - Go [here](https://github.com/liquidctl/liquidctl#supported-devices) and see if your device is listed.
        - Yes -> make a feature request for CoolerControl to add support for that device.
        - No -> continue

2. Is your device supported by hwmon?
    - Check [here](https://hwmon.wiki.kernel.org/device_support_status) to see if you can find your device and/or follow
      the [hwmon support guide](#hwmon-support) to see if you see your device is listed in the `sensor` command output.
        - Yes -> you should see the supported controls once you've enabled [HWMon support](#hwmon-support). If your
          device doesn't work as expected make a feature request to add or fix support for it.
        - No -> continue

3. Not supported by the above? There are still some options:
    1. See if another library does support communicating with the device and make a feature request to have CoolerControl integrate support
       for it.
    2. Support development of a driver for the device by contributing:
       see [liquidctl docs](https://github.com/liquidctl/liquidctl/tree/main/docs)
       or the [lm-sensors repo](https://github.com/lm-sensors/lm-sensors.git).
4. Once support has been added:
    - please report any bugs you notice using the device, real world device testing and feedback is invaluable.

## Acknowledgements

* Major thanks is owed to the python API of [liquidctl](https://github.com/liquidctl/liquidctl)
* Thanks to all the many contributors of [HWMon](https://hwmon.wiki.kernel.org/projectinformation)
* A big inspiration is [GKraken](https://gitlab.com/leinardi/gkraken) written by Roberto Leinardi.
* UI based on [PyOneDark](https://github.com/Wanderson-Magalhaes/PyOneDark_Qt_Widgets_Modern_GUI) by Wanderson M.Pimenta

## License

This program is licensed under [GPLv3](LICENSE)

## FAQ

- Should I use Liquid or CPU as a temperature source to control my pump/fans?
    - Quick answer: Liquid
    - The thermodynamics of liquid cooling are very different compared to the traditional method. Choose what works best for your situation.
- I have an issue with X, what do I do?
    - Please join the discord channel if it's something small, otherwise opening an Issue ticket in GitLab is the best way to get something
      fixed.
- Why should I use this program when I could do what CoolerControl does with a shell script?
    - Oh, you definitely can, and I would encourage you to do so. CoolerControl started out as a dynamic replacement for some
      of my own scripts with the added advantage of being able to visualize the data I was collecting.
- Can I request a feature, report a bug, or voice a concern?
    - Yes please! See [GitLab issues](https://gitlab.com/coolercontrol/coolercontrol/-/issues)
