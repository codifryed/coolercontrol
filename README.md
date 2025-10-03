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
<a href="https://gitlab.com/coolercontrol/coolercontrol/-/releases"><img src="https://img.shields.io/gitlab/v/release/30707566?sort=semver&logo=gitlab&style=for-the-badge&color=568af2&labelColor=2c313c&logoColor=dce1ec"></a>
<a href="https://gitlab.com/coolercontrol/coolercontrol/-/commits"><img src="https://img.shields.io/gitlab/last-commit/coolercontrol/coolercontrol?style=for-the-badge&logo=gitlab&color=568af2&labelColor=2c313c&logoColor=dce1ec"></a>

<img src="https://img.shields.io/badge/_-Linux-2c313c?style=for-the-badge&logo=linux&logoColor=dce1ec">
<img src="https://img.shields.io/badge/_-Rust-2c313c?style=for-the-badge&logo=rust">
<img src="https://img.shields.io/badge/_-Vue-2c313c?style=for-the-badge&logo=vue.js">

<!-- trunk-ignore-end(markdownlint)-->

<br/>
<br/>
<p>
Powerful cooling control and monitoring for Linux 🐧
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
[Hardware Support](https://docs.coolercontrol.org/hardware-support.html) ·
[Documentation](https://docs.coolercontrol.org) · [Discord](https://discord.gg/MbcgUFAfhV)

<!-- trunk-ignore-end(markdownlint/MD051): links with emojis -->

</h2>
</div>
<br/>

## ✨ Features

- Highly configurable GUI with dashboards
- System daemon runs in the background
- Control devices based on any temperature or combinations of sensors
- Auto-detection of hwmon/sysfs and liquidctl devices
- Enhanced liquidctl device support (AIOs, USB fan hubs, LCD screens, RGB lighting, etc.)
- GPU fan control for most NVIDIA and AMD GPUs
- Fully customizable `Profiles` (Fixed, Graph, Mix, Overlay) that can be applied to multiple fans
- `Functions` add hysteresis, thresholds, directionality, and response-time control
- System-wide cooling `Modes` to adjust all devices at once
- `Custom Sensors` from files or combinations of existing sensors
- Multiple `Dashboards` with filters for sensor data
- `Alerts` for temperature/fan anomalies
- Reapplies settings after sleep
- External monitoring and GUI support
- Headless support with a built-in Web UI
- Comprehensive REST API for integrations

### 🛠️ Installation Instructions

[See the Getting Started page](https://docs.coolercontrol.org/getting-started.html)

## 🧰 Hardware Support

[See the Hardware Support page](https://docs.coolercontrol.org/hardware-support.html)

## ❔ Problem or Question

If you are experiencing an issue or have a feature request, please open an
[issue in GitLab](https://gitlab.com/coolercontrol/coolercontrol/-/issues) and use one of the
provided templates. When submitting a bug
[daemon logs](https://gitlab.com/coolercontrol/coolercontrol/-/wikis/Log-Output-&-Debugging#to-capture-log-output-to-a-file)
are invaluable for determining the cause. If you have a general question, please join the
[Discord](https://discord.gg/MbcgUFAfhV) channel where community members can also help.

## ❤️ Support CoolerControl

Made for Linux, used 24/7. CoolerControl started as the tool I needed for my own rigs and grew from
there. If you’d like to help, your support goes straight into new features, integrations,
maintenance, and a cup of coffee to power those late‑night coding sessions.

<div>
<!-- trunk-ignore-begin(markdownlint)-->
<a href="https://ko-fi.com/codifryed"><img src="https://img.shields.io/badge/Ko--fi-F16061?style=for-the-badge&logo=ko-fi&logoColor=white"></a>
<a href="https://github.com/sponsors/codifryed"><img src="https://img.shields.io/badge/sponsor-30363D?style=for-the-badge&logo=GitHub-Sponsors&logoColor=#EA4AAA"></a>
<!-- trunk-ignore-end(markdownlint)-->
</div>

## 🚀 Contributing

Contributions are welcome and if you have an idea or want to submit some changes, it's best to
either [submit an Issue](https://gitlab.com/coolercontrol/coolercontrol/-/issues/) or get on
[Discord](https://discord.gg/MbcgUFAfhV) to discuss it first. For general information, please read
the
[contributing guidelines](https://gitlab.com/coolercontrol/coolercontrol/-/blob/main/CONTRIBUTING.md).

## ⭐ Acknowledgements

- A big inspiration is [GKraken](https://gitlab.com/leinardi/gkraken) written by Roberto Leinardi.
- Major thanks to the Python API of [liquidctl](https://github.com/liquidctl/liquidctl)
- Thanks to the many contributors to [hwmon](https://docs.kernel.org/hwmon/)

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
