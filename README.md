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
<br/>
<p>
CoolerControl is a feature-rich cooling device control and monitoring application for Linux.
</p>

<!-- trunk-ignore-begin(markdownlint/MD045): links with emojis -->

![](screenshots/coolercontrol.webm)

<!-- trunk-ignore-end(markdownlint/MD045): links with emojis -->

<!-- <img src="screenshots/coolercontrol-overview.png" alt="Screenshot" width="700"/> -->

</div>

<div align="center">
<h2>

<!-- trunk-ignore-begin(markdownlint/MD051): links with emojis -->

[Install](https://docs.coolercontrol.org/getting-started.html) ·
[Hardware Support](#🧰-hardware-support) · [Documentation](https://docs.coolercontrol.org) ·
[Discord](https://discord.gg/MbcgUFAfhV)

<!-- trunk-ignore-end(markdownlint/MD051): links with emojis -->

</h2>
</div>
<br/>

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
- Multiple `Dashboards` with filters to view your system's sensor data
- `Alerts` to notify you of unexpected changes to temperatures or fans
- Re-applies settings after waking from sleep
- External monitoring and GUI support
- Headless server support with an available Web UI
- Comprehensive REST API for extensions

## 📦 Packages

CoolerControl is made up of several sub-packages:

1. `coolercontrold` _(required)_ - The system service that handles controlling your hardware.
2. `coolercontrol-liqctld` _(optional)_ - Service integration for `liquidctl` device support (AIOs,
   USB fan hubs, etc).
3. `coolercontrol` _(optional)_ - the standalone Desktop Application. _(alternatively you can access
   the [Web UI](http://localhost:11987) in your browser)_

_\*Note: You can control the daemon using its
[config file](https://gitlab.com/coolercontrol/coolercontrol/-/wikis/config-files), but that is not
officially supported._

### 🛠️ [Installation Instructions](https://docs.coolercontrol.org/getting-started.html)

## 🧰 Hardware Support

CoolerControl depends on [Hwmon](https://docs.kernel.org/hwmon/) kernel drivers and 
[liquidctl](https://github.com/liquidctl/liquidctl) to access and control supported hardware. Note
that your hardware <ins>**is not guaranteed**</ins> to be supported, as this depends on open-source
drivers and contributors. The following are the steps you should take to **maximize** hardware
coverage:

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
- NVidia GPUs - Fan control has been tested working on most cards with the NVidia proprietary
  drivers. CoolerControl **automatically** uses `NVML` and the CLI tools `nvidia-settings` and
  `nvidia-smi` as a fallback.
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

## 🌐 [CoolerControl Website](https://docs.coolercontrol.org)

## ❔ Problem or Question

If you are experiencing an issue or have a feature request, please open up an
[issue in GitLab](https://gitlab.com/coolercontrol/coolercontrol/-/issues) and use one of the
provided templates. When submitting a bug
[daemon logs](https://gitlab.com/coolercontrol/coolercontrol/-/wikis/Log-Output-&-Debugging#to-capture-log-output-to-a-file)
are invaluable to determining the cause. If you have a general question, please join the
[Discord](https://discord.gg/MbcgUFAfhV) channel where community members can also help.

## 🚀 Contributing

:heart: CoolerControl is in need of help with the following areas:

- Packaging
- Website
- Spreading the word

If you have an idea or want to submit some changes, it's usually best to either
[submit an Issue](https://gitlab.com/coolercontrol/coolercontrol/-/issues/) first or get on
[Discord](https://discord.gg/MbcgUFAfhV) to discuss it. For general information please read the
[contributing guidelines](https://gitlab.com/coolercontrol/coolercontrol/-/blob/main/CONTRIBUTING.md).

## ⭐ Acknowledgements

- Major thanks is owed to the python API of [liquidctl](https://github.com/liquidctl/liquidctl)
- Thanks to all the many contributors of [HWMon](https://docs.kernel.org/hwmon/)
- A big inspiration is [GKraken](https://gitlab.com/leinardi/gkraken) written by Roberto Leinardi.

## 📝 License

This program is licensed under [GPLv3+](LICENSE)

## 🗒️ Related Projects

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
