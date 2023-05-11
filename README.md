<div align="center">
  <h1>
  <img alt="CoolerControl" src="https://gitlab.com/coolercontrol/coolercontrol/-/raw/main/packaging/metadata/org.coolercontrol.CoolerControl.png" width="200">
  <br>
  CoolerControl
  <br>
  <br>
  </h1>

[![Discord](https://img.shields.io/badge/_-online-_?label=&logo=discord&logoColor=ffffff&color=7389D8&labelColor=6A7EC2)](https://discord.gg/MbcgUFAfhV)
[![Linux](https://img.shields.io/badge/_-linux-blue?logo=linux&logoColor=fff)]()
[![Python](https://img.shields.io/badge/_-python-blue?logo=python&logoColor=fff)]()
[![Rust](https://img.shields.io/badge/_-rust-orange?logo=rust&logoColor=fff)]()
[![GPLv3 License](https://img.shields.io/badge/License-GPL%20v3-blue.svg?logo=gnu)](https://opensource.org/licenses/)
[![GitLab Release (latest by SemVer)](https://img.shields.io/gitlab/v/release/30707566?sort=semver&logo=gitlab)](https://gitlab.com/coolercontrol/coolercontrol/pipelines)

![Preview Video](screenshots/coolercontrol.webm)

</div>

<br>
<div align="center">

[Installation](#installation) -
[Wiki](https://gitlab.com/coolercontrol/coolercontrol/-/wikis/home) -
[Issues](#issues) -
[Contributing](#contributing) -
[Acknowledgements](#acknowledgements) -
[License](#license) -
[Related Projects](#related-projects)
</div>
<br>

## Cooling device control for Linux

CoolerControl features a GUI for viewing all your system's sensors and for creating custom fan and pump profiles based on any
available temperature sensor. Paired with this is a systemd service that controls all your devices in the background.

It's an extension of [liquidctl](https://github.com/liquidctl/liquidctl)
and [hwmon](https://hwmon.wiki.kernel.org) with a focus on controlling cooling devices such as AIO coolers and fans under Linux. Written
in [Python](https://www.python.org/) and [Rust](https://www.rust-lang.org/), it uses [PySide](https://wiki.qt.io/Qt_for_Python) for the UI.

*NOTE:* This project is still in the development phase and working towards its first stable release.

## Installation

1. [System Setup](#system-setup)
2. Install:
    - [AppImage](#appimage)
    - [AUR](#aur)
    - [Ubuntu/Debian](#debian)
    - [Fedora](#fedora)
    - [OpenSuse Tumbleweed](#opensuse-tumbleweed)
    - [Source (*work in progress*)](#source-wip)

## More Information

For a list of supported devices and more info on how to setup and configure the software check out
the [CoolerControl Wiki](https://gitlab.com/coolercontrol/coolercontrol/-/wikis/home).

## System Setup

Here are some steps to prepare your system for maximum usability with CoolerControl. (recommended)

- To have access to all available hwmon supported devices & controls it's recommended to have `lm-sensors` installed and to
  run `sudo sensors-detect`. For more details see the [Arch Wiki](https://wiki.archlinux.org/index.php/Lm_sensors#Installation) and
  the [HWMon Support section](https://gitlab.com/coolercontrol/coolercontrol/-/wikis/HWMon-Support)
- NVIDIA GPUs:
    - Fan control is currently supported as a single control for all fans on the card. If not already, make sure
      that `nvidia-settings` and `nvidia-smi` are installed on your machine. On some distributions this is done automatically with the
      driver, on others you need to install this manually.
- CoolerControl generally will detect supported devices and available capabilities automatically. The GUI will also prompt you for
  additional steps if necessary.

## AppImage

[![AppImageDownload](screenshots/download-appimage-banner.svg)](https://gitlab.com/coolercontrol/coolercontrol/-/releases/permalink/latest/downloads/packages/CoolerControlD-x86_64.AppImage)  [![AppImageDownload](screenshots/download-appimage-banner.svg)](https://gitlab.com/coolercontrol/coolercontrol/-/releases/permalink/latest/downloads/packages/CoolerControl-x86_64.AppImage)

Use both of the above links or goto the [Releases](https://gitlab.com/coolercontrol/coolercontrol/-/releases) page and download a specific
version.

There are two AppImages:  
`CoolerControlD` which runs as a daemon in the background and needs sudo access.  
`CoolerControl` which can be run to start the GUI and needs a desktop environment to run.

The AppImages are helpful if you want to try things out without installing anything. It is recommended to install the systems packages, as
it then installed as a systemd service which starts the daemon at boot and version updates are handled automatically. AppImage updates must
be handled manually.

The AppImages contain all the needed dependencies. Just make it executable and run it:

```bash
chmod +x CoolerControlD-x86_64.AppImage
chmod +x CoolerControl-x86_64.AppImage
# start daemon and put into the background
sudo ./CoolerControlD-x86_64.AppImage &
./CoolerControl-x86_64.AppImage
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

## AUR

[![Linux](https://img.shields.io/badge/_-Arch_Linux-blue?logo=arch-linux&logoColor=fff)](#aur)

Use your installed AUR Helper, i.e.:

```bash
yay -S coolercontrol
```

Then enable the systemd service:

```bash
sudo systemctl enable coolercontrold.service
sudo systemctl start coolercontrold.service
```

Finally start `coolerocontrol` like any normal desktop application, or from the commandline.

## Packages

[![Hosted By: Cloudsmith](https://img.shields.io/badge/OSS%20hosting%20by-cloudsmith-blue?logo=cloudsmith)](https://cloudsmith.com)

The system packages are compiled with the needed libraries and so should have very few system dependencies.
Package repository hosting is graciously provided by  [Cloudsmith](https://cloudsmith.com) - a fully hosted, cloud-native, universal package
management solution.

## Debian

[![Linux](https://img.shields.io/badge/_-ubuntu-orange?logo=ubuntu&logoColor=fff)](#ubuntu---debian)
[![Linux](https://img.shields.io/badge/_-debian-red?logo=debian&logoColor=fff)](#ubuntu---debian)

You can quickly setup the repository automatically (recommended):  
*[Other Package Repository Options](#package-repository-options)*

```bash
curl -1sLf \
  'https://dl.cloudsmith.io/public/coolercontrol/coolercontrol/setup.deb.sh' \
  | sudo -E bash
```

```bash
sudo apt update
sudo apt install coolercontrol
```

```bash
sudo systemctl enable coolercontrold
sudo systemctl start coolercontrold
```

If using **X11** you'll also need:

```bash
sudo apt install libxcb-cursor0
```

Finally start `coolerocontrol` like any normal desktop application, or from the commandline.

## Fedora

[![Linux](https://img.shields.io/badge/_-fedora-blue?logo=fedora&logoColor=fff)](#fedora)

You can quickly setup the repository automatically (recommended):  
*[Other Package Repository Options](#package-repository-options)*

```bash
curl -1sLf \
  'https://dl.cloudsmith.io/public/coolercontrol/coolercontrol/setup.rpm.sh' \
  | sudo -E bash
```

```bash
sudo dnf update
sudo dnf install coolercontrol
```

```bash
sudo systemctl enable coolercontrold
sudo systemctl start coolercontrold
```

If using **X11** you'll also need:

```bash
sudo dnf install xcb-util-cursor
```

Finally start `coolerocontrol` like any normal desktop application, or from the commandline.

## OpenSuse Tumbleweed

[![Linux](https://img.shields.io/badge/_-tumbleweed-green?logo=opensuse&logoColor=fff)](#opensuse-tumbleweed)

You can quickly setup the repository automatically (recommended):  
*[Other Package Repository Options](#package-repository-options)*

```bash
curl -1sLf \
  'https://dl.cloudsmith.io/public/coolercontrol/coolercontrol/setup.rpm.sh' \
  | sudo -E distro=opensuse codename=tumbleweed arch=x86_64 bash
```

```bash
sudo zypper ref
sudo zypper install coolercontrol
```

```bash
sudo systemctl enable coolercontrold
sudo systemctl start coolercontrold
```

Finally start `coolerocontrol` like any normal desktop application, or from the commandline.

### Package Repository Options

For other options, such as if you need to force a specific distribution, release/version, or you want to do the steps manually, check out
the [CoolerControl repository on Cloudsmith](https://cloudsmith.io/~coolercontrol/repos/coolercontrol/setup/).
If your particular distribution is not available from the repository,
please [submit an issue](https://gitlab.com/coolercontrol/coolercontrol/-/issues).

#### Repository Alternative

You can download a package file directly from the [Releases Page](https://gitlab.com/coolercontrol/coolercontrol/-/releases) and install the
package manually.

## Source (WIP)

<details>
<summary>Click to view</summary>

### Requirements

* Linux
* [Python 3.10](https://www.python.org/)
    * including the python3.10-dev package (may already be installed)
* Rust 1.66+

### System Packages

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

### [Poetry](https://python-poetry.org/)

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

### CoolerControl Files

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
      <!-- TODO: make install(compile all and copy to install dir) & install systemd files -->

</details>

---

<br>

# Issues

If you are experiencing an issue or have a feature request, please open up
an [issue in GitLab](https://gitlab.com/coolercontrol/coolercontrol/-/issues) and use one of the provided templates. When submitting a
bug [daemon logs](https://gitlab.com/coolercontrol/coolercontrol/-/wikis/Log-Output-&-Debugging#to-capture-log-output-to-a-file) are
invaluable to determining the cause. If you have a general question, please
join the discord channel where community members can also help.

Please remember that CoolerControl is not yet considered stable and breaking changes, although not often, can happen. The best thing to do
in those situations is to reapply your settings after an upgrade.

# Contributing

:heart: CoolerControl is in need of help with the following areas:

- Packaging
- Website
- Spreading the word

For general information please read
the [contributing guidelines](https://gitlab.com/coolercontrol/coolercontrol/-/blob/main/CONTRIBUTING.md).

# Acknowledgements

* Major thanks is owed to the python API of [liquidctl](https://github.com/liquidctl/liquidctl)
* Thanks to all the many contributors of [HWMon](https://hwmon.wiki.kernel.org/projectinformation)
* A big inspiration is [GKraken](https://gitlab.com/leinardi/gkraken) written by Roberto Leinardi.
* UI based on [PyOneDark](https://github.com/Wanderson-Magalhaes/PyOneDark_Qt_Widgets_Modern_GUI) by Wanderson M.Pimenta

# License

This program is licensed under [GPLv3](LICENSE)

# Related Projects

- [liquidctl](https://github.com/liquidctl/liquidctl)  
  Cross-platform tool and drivers for liquid coolers and other devices.


- [fan2go](https://github.com/markusressel/fan2go)  
  A daemon to control the fans of your computer.


- [thinkfan](https://github.com/vmatare/thinkfan)  
  A simple, lightweight fan control program. (ThinkPads)


- [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB)  
  Graphical interface to control many different types of RGB devices.


- [FanControl](https://github.com/Rem0o/FanControl.Releases)  
  A focused and highly customizable fan controlling software for Windows.
