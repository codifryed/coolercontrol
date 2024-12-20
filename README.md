<!-- trunk-ignore(markdownlint/MD041): First line should be heading -->
<div align="center">
  <h1>
  <img alt="CoolerControl" src="https://gitlab.com/coolercontrol/coolercontrol/-/raw/main/packaging/metadata/org.coolercontrol.CoolerControl.png" width="150">
  <br>
  CoolerControl
  <br>
  </h1>

<!-- trunk-ignore-begin(markdownlint)-->

<a href="https://discord.gg/MbcgUFAfhV"><img src="https://img.shields.io/badge/_-discord-_?style=for-the-badge&label=chat&logo=discord&color=568af2&labelColor=2c313c&logoColor=dce1ec"></a>
<a href="https://gitlab.com/coolercontrol/coolercontrol/pipelines"><img src="https://img.shields.io/gitlab/v/release/30707566?sort=semver&logo=gitlab&style=for-the-badge&color=568af2&labelColor=2c313c&logoColor=dce1ec"></a>
<a href="https://gitlab.com/coolercontrol/coolercontrol/-/graphs/main/charts"><img src="https://img.shields.io/gitlab/last-commit/coolercontrol/coolercontrol?style=for-the-badge&logo=gitlab&color=568af2&labelColor=2c313c&logoColor=dce1ec"></a>

<img src="https://img.shields.io/badge/_-Linux-2c313c?style=for-the-badge&logo=linux&logoColor=dce1ec">
<img src="https://img.shields.io/badge/_-Rust-2c313c?style=for-the-badge&logo=rust">
<img src="https://img.shields.io/badge/_-Vue-2c313c?style=for-the-badge&logo=vue.js">
<img src="https://img.shields.io/badge/_-Python-2c313c?style=for-the-badge&logo=python">

<!-- trunk-ignore-end(markdownlint)-->

<br/>
<p>
CoolerControl is a feature-rich cooling device control and monitoring application for Linux.
</p>

<!-- ![Preview Video](screenshots/coolercontrol.webm) -->
<img src="screenshots/coolercontrol-overview.png" alt="Screenshot" width="700"/>

</div>

<h4 align="center">

[Install](#-installation) · [Hardware Support](#-hardware-support) ·
[Docs](https://gitlab.com/coolercontrol/coolercontrol/-/wikis/home) ·
[Discord](https://discord.gg/MbcgUFAfhV)

</h4>

## ✨ Features

- A highly configurable control GUI with dashboards
- A system daemon that runs in the background
- Control any device based on any temperature or combination of temperatures
- Auto detection of hwmon/sysfs and liquidctl devices
- Enhanced liquidctl device support (AIOs, USB Fan hubs, LCD screens, RGB lighting, etc)
- Fan control support for most NVidia and AMD GPUs
- Fully customizable speed `Profiles` like Fixed, Graph(Curve), and Mix that can be applied to
  multiple fans
- `Functions` to control how a Profile is applied with hysteresis, threshold, directional, and
  response time control
- System-wide cooling `Modes` to adjust all your devices at once
- Create your own `Custom Sensors` based on a File or on a combination of temperature sensors
- Re-applies settings after waking from sleep
- External monitoring and GUI support
- Headless server support with an available Web UI
- Comprehensive REST API for extensions

## 🛠️ Installation

CoolerControl is made up of several sub-packages:

1. `coolercontrold` _(required)_ - The system service that handles controlling your hardware.
2. `coolercontrol-liqctld` _(optional)_ - Service integration for `liquidctl` device support (AIOs,
   USB fan hubs, etc).
3. `coolercontrol` _(optional)_ - the standalone Desktop Application. _(alternatively you can access
   the [Web UI](http://localhost:11987) in your browser)_

_\*Note: You can control the daemon using its
[config file](https://gitlab.com/coolercontrol/coolercontrol/-/wikis/config-files), but that is not
officially supported._

### 📦 Packages

- [AppImage](#appimage)
- [AUR](#aur)
- [Ubuntu/Debian Based](#debian)
- [Fedora](#fedora)
- [OpenSuse Tumbleweed](#opensuse-tumbleweed)
- [Nix](#nix)
- [Gentoo](#gentoo)
- [From Source](#source)

## 🧰 Hardware Support

CoolerControl depends on [Hwmon](https://hwmon.wiki.kernel.org/projectinformation) kernel drivers
and [liquidctl](https://github.com/liquidctl/liquidctl) to access and control supported hardware.
Note that your hardware <ins>**is not guaranteed**</ins> to be supported, as this depends on
open-source drivers and contributors. The following are the steps you should take to **maximize**
hardware coverage:

- Install **`lm-sensors`** and run `sudo sensors-detect`. For more details see the
  [Arch Wiki](https://wiki.archlinux.org/index.php/Lm_sensors#Installation) and the
  [HWMon Support section](https://gitlab.com/coolercontrol/coolercontrol/-/wikis/HWMon-Support).
  Additionally, you can check out the official
  [lm-sensors repository](https://github.com/lm-sensors/lm-sensors/issues) for tips on manually
  loading unofficial kernel modules for hardware that isn't supported out-of-the-box yet.
- For newer motherboards and cards it's best to install the **latest available kernel** for your
  distribution which includes the latest Hwmon drivers and kernel modules.
- Check the [liquidctl hardware support list](https://github.com/liquidctl/liquidctl) for the state
  of support for USB devices like fan hubs and AIOs.
- NVidia GPUs - Fan control has been tested working on most cards. CoolerControl **automatically**
  uses NVML and the CLI tools `nvidia-settings` and `nvidia-smi` as backup.
- AMD GPUs - Older cards work out of the box. The 7000 series and above have different firmware
  controls and CoolerControl will work if you:
  - enable the fan control feature by setting the kernel boot option:
    `amdgpu.ppfeaturemask=0xffffffff`.
  - ⚠️ Be aware that regardless of the fan speed set, the fan will only turn on once a builtin
    (non-configurable) temperature threshold has been reached. In other words, the card needs to be
    under load for the fan settings to take effect. It has been reported that that the fans will
    generally start spining once the Junction Temperature reaches 60C and stop spinning at 50C.
- Laptops - ThinkPads, some ASUS, and some HP Laptops are known to have supported linux drivers,
  **but not all**. If your laptop has a hwmon kernel driver, then CoolerControl will use it
  automatically. Otherwise, fan control for your laptop is most likely not supported.
- In general, CoolerControl will detect supported devices and available capabilities
  **automatically**. If needed the GUI will also prompt you for any additional steps. There are some
  situations where the kernel drivers are not yet mature enough to offer full fan control
  functionality, in which case you will get **an error** when attempting to apply changes.

## AppImage

[![AppImageDownload](screenshots/download-appimage-banner.svg)](https://gitlab.com/coolercontrol/coolercontrol/-/releases/permalink/latest/downloads/packages/CoolerControlD-x86_64.AppImage)  
[**CoolerControlD Daemon**](https://gitlab.com/coolercontrol/coolercontrol/-/releases/permalink/latest/downloads/packages/CoolerControlD-x86_64.AppImage)

---

[![AppImageDownload](screenshots/download-appimage-banner.svg)](https://gitlab.com/coolercontrol/coolercontrol/-/releases/permalink/latest/downloads/packages/CoolerControl-x86_64.AppImage)  
[**CoolerControl Desktop App (Optional)**](https://gitlab.com/coolercontrol/coolercontrol/-/releases/permalink/latest/downloads/packages/CoolerControl-x86_64.AppImage)

There are two AppImages:  
`CoolerControlD` _(required)_ The system daemon that **requires sudo access**.  
`CoolerControl` _(optional)_ The standalone GUI Desktop Application. _(alternatively you can access
the [Web UI](http://localhost:11987) in your browser)_

Download them by using either the above links or goto the
[Releases](https://gitlab.com/coolercontrol/coolercontrol/-/releases) page to download specific
versions.

The AppImages are helpful if you want to try things out without installing anything. Note that it is
recommended to **install the systems packages** whenever possible, as many things are then handled
automatically.

Once downloaded:

```bash
chmod +x CoolerControlD-x86_64.AppImage
chmod +x CoolerControl-x86_64.AppImage
```

Start the daemon:

```bash
sudo ./CoolerControlD-x86_64.AppImage
```

Then run the either the desktop appimage or access the Web UI.

_\* :warning: on some systems you'll have to install `libfuse2` or `fuse` for appimages to work._

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

[![Arch Linux](https://img.shields.io/badge/Arch_Linux-1793D1?style=for-the-badge&logo=arch-linux&logoColor=white)](#aur)
[![Arch Linux](https://img.shields.io/badge/manjaro-35BF5C?style=for-the-badge&logo=manjaro&logoColor=white)](#aur)  
[![AUR](https://img.shields.io/aur/votes/coolercontrol.svg?style=for-the-badge)](https://aur.archlinux.org/packages/coolercontrol)

There are official binary and source packages in the AUR.

Use your installed AUR Helper, i.e.:

```bash
# binary package
yay -S coolercontrol-bin

# source package
yay -S coolercontrol
```

Then enable and start the systemd service:

```bash
sudo systemctl enable --now coolercontrold
```

## Debian

[![Linux](https://img.shields.io/badge/Debian-A81D33?style=for-the-badge&logo=debian&logoColor=white)](#debian)
[![Linux](https://img.shields.io/badge/Ubuntu-E95420?style=for-the-badge&logo=ubuntu&logoColor=white)](#debian)
[![Linux](https://img.shields.io/badge/Pop!_OS-48B9C7?style=for-the-badge&logo=Pop!_OS&logoColor=white)](#debian)
[![Linux](https://img.shields.io/badge/Linux_Mint-87CF3E?style=for-the-badge&logo=linux-mint&logoColor=white)](#debian)

Debain packages are supported for the following distros:

- \>= Debian Bookworm
- \>= Ubuntu 22.04 (Jammy)
- Most other distributions based on the above.
- Kali Linux does **not** support using the `coolercontrol-liqctld` package.

**Recommended** You can quickly setup the Cloudsmith repository automatically:  
_\*[Other Cloudsmith Options](#cloudsmith-package-options)_

Make sure `curl` is installed:

```bash
sudo apt install curl apt-transport-https
```

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
sudo systemctl enable --now coolercontrold
```

### Ubuntu 22.04 LTS (Optional)

The Ubuntu package `liquidctl` is outdated. Therefore, some devices might not show, such as the
`NZXT Smart Device V2`. To fix this, you can force upgrade the package:

> ⚠️ **Before proceeding, make sure to run the commands outlined [above](#debian).**

```bash
sudo systemctl stop coolercontrold
sudo pip install liquidctl --upgrade
sudo systemctl start coolercontrold
```

<!-- trunk-ignore-begin(markdownlint/MD036): Emphasis used instead of heading -->

_You might need to restart your computer for the changes to take effect_

<!-- trunk-ignore-end(markdownlint/MD036) -->

## Fedora

[![Linux](https://img.shields.io/badge/Fedora-294172?style=for-the-badge&logo=fedora&logoColor=white)](#fedora)
[![Linux](https://img.shields.io/badge/Nobara-black?style=for-the-badge)](#fedora)  
[![Linux](https://copr.fedorainfracloud.org/coprs/codifryed/CoolerControl/package/coolercontrol/status_image/last_build.png)](#fedora)

There is a Copr repository available for Fedora based distributions:

```bash
# make sure you have the necessary plugin:
sudo dnf install dnf-plugins-core
sudo dnf copr enable codifryed/CoolerControl
sudo dnf install coolercontrol
sudo systemctl enable --now coolercontrold
```

## OpenSuse Tumbleweed

[![Linux](https://img.shields.io/badge/Tumbleweed-0C322C?style=for-the-badge&logo=opensuse&logoColor=white)](#opensuse-tumbleweed)  
[![build result](https://build.opensuse.org/projects/home:codifryed/packages/coolercontrol/badge.svg?type=default)](https://build.opensuse.org/package/show/home:codifryed/coolercontrol)

Packaging is done on the Open Build Service for openSuse Tumbleweed and there are two easy ways to
install the packages:

1. You can use the
   [1-Click-Install](https://software.opensuse.org/ymp/home:codifryed/openSUSE_Tumbleweed/coolercontrol.ymp)
   method.
2. Or install from the command line:

```bash
# make sure opi is installed if it's not already:
sudo zypper install opi
opi coolercontrol
```

Then enable and start the systemd service:

```bash
sudo systemctl enable --now coolercontrold
```

## Nix

[![Linux](https://img.shields.io/badge/NixOS-5277C3?style=for-the-badge&logo=nixos&logoColor=white)](#nix)

The coolercontrol package is currently a part of the `nixpkgs-unstable` and `nixos-unstable`
channels.

For NixOS there are is a configuration option available, which should install the application and
enable the services:

```nix
programs.coolercontrol.enable = true;
```

And an option for NVidia graphic card owners that should default to on if you have the nvidia driver
in `services.xserver.videoDrivers`:

```nix
programs.coolercontrol.nvidiaSupport = true;
```

If installing using the Nix package manager on a non-NixOS distro, you'll need to do some things
manually. For example:

```bash
# Make sure your channel is up to date
nix-channel --update
nix-env -iA nixpkgs.coolercontrol
sudo systemctl enable --now ~/.nix-profile/lib/systemd/system/coolercontrold.service ~/.nix-profile/lib/systemd/system/coolercontrol-liqctld.service
```

:warning: On non-NixOS, this will enable the services for the currently installed version. You need
to disable the services and re-enable them after each update. `systemctl reenable` will not work.

## Gentoo

[![Linux](https://img.shields.io/badge/Gentoo-54487A?style=for-the-badge&logo=gentoo&logoColor=white)](#gentoo)

Gentoo users can use these unofficial portage overlays:

- https://gpo.zugaina.org/sys-apps/coolercontrold
- https://gpo.zugaina.org/sys-apps/coolercontrol-liqctld
- https://gpo.zugaina.org/sys-apps/coolercontrol

## Cloudsmith Package Options

[![Hosted By: Cloudsmith](https://img.shields.io/badge/OSS%20hosting%20by-cloudsmith-blue?logo=cloudsmith)](https://cloudsmith.com)

Package repositories for some distros is graciously provided by
[Cloudsmith](https://cloudsmith.com) - a fully hosted, cloud-native, universal package management
solution.

For special options, such as if you need to **force a specific distribution, release/version**, or
you want to do the steps manually, check out the
[CoolerControl repository on Cloudsmith](https://cloudsmith.io/~coolercontrol/repos/coolercontrol/setup/).
When using **a distribution that is based on another**, but not natively supported by Cloudsmith,
you can use the base-distribution repository. For example, if your distribution is based on Fedora:

```bash
curl -1sLf \
  'https://dl.cloudsmith.io/public/coolercontrol/coolercontrol/setup.rpm.sh' \
  | sudo -E distro=fedora codename=38 bash
```

#### Repository Alternative <!-- trunk-ignore(markdownlint/MD001): Emphasis used instead of heading -->

You can download the package files directly from the
[Releases Page](https://gitlab.com/coolercontrol/coolercontrol/-/releases) and install the packages
manually.

## Source

[![Linux](https://img.shields.io/badge/GIT-E44C30?style=for-the-badge&logo=git&logoColor=white)](#source-wip)

### Requirements

- git
- make
- cargo/rust >= 1.81.0
- python >= 3.8
- nodejs >= 18.0.0
- npm
- libdrm-dev

_Note:_ If you are running Arch Linux, installing from source requires special steps that the
official AUR package already does for you.

### System Packages

To optionally build the standalone GUI application you'll also need
[these Tauri development packages](https://v2.tauri.app/start/prerequisites/#linux).

### Setup Source

```bash
git clone https://gitlab.com/coolercontrol/coolercontrol.git
git checkout main
git pull
```

### Build and Install Everything

```bash
cd coolercontrol
make install-source -j3
# and watch it go.
```

That should install all the needed files onto your system.

Then start the daemons:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now coolercontrold
```

You should then be able to start the GUI like normal.

---

<br/>

# ❔ Problem or Question

If you are experiencing an issue or have a feature request, please open up an
[issue in GitLab](https://gitlab.com/coolercontrol/coolercontrol/-/issues) and use one of the
provided templates. When submitting a bug
[daemon logs](https://gitlab.com/coolercontrol/coolercontrol/-/wikis/Log-Output-&-Debugging#to-capture-log-output-to-a-file)
are invaluable to determining the cause. If you have a general question, please join the
[Discord](https://discord.gg/MbcgUFAfhV) channel where community members can also help.

# 🚀 Contributing

:heart: CoolerControl is in need of help with the following areas:

- Packaging
- Website
- Spreading the word

If you have an idea or want to submit some changes, it's usually best to either
[submit an Issue](https://gitlab.com/coolercontrol/coolercontrol/-/issues/) first or get on
[Discord](https://discord.gg/MbcgUFAfhV) to discuss it. For general information please read the
[contributing guidelines](https://gitlab.com/coolercontrol/coolercontrol/-/blob/main/CONTRIBUTING.md).

# ⭐ Acknowledgements

- Major thanks is owed to the python API of [liquidctl](https://github.com/liquidctl/liquidctl)
- Thanks to all the many contributors of [HWMon](https://hwmon.wiki.kernel.org/projectinformation)
- A big inspiration is [GKraken](https://gitlab.com/leinardi/gkraken) written by Roberto Leinardi.

# 📝 License

This program is licensed under [GPLv3+](LICENSE)

# 🗒️ Related Projects

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
