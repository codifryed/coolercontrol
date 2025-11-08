# Changelog

<!-- trunk-ignore-all(markdownlint/MD024): Multiple heading with same content -->

<!--
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Release notes are automatically generated from this file and git tags.
-->

## [3.0.2] - 2025-11-03

### Added

- firmware profile support for various HWMon drivers (!366)
- simulated zero-rpm mode for Nvidia GPUs that don't support 0% manual fan speed (#523)

### Changed

- minor liqctld improvements (!363)
- improved downstream packaging support (!364, !365, !369)
- improved UI reconnection logic (!368, #520)

### Fixed

- legacy Asetec/Kraken device support (#515)
- liqctld shutdown issue (!367)

## [3.0.1] - 2025-10-04

### Changed

- improved multiple-UI reconnection handling

### Fixed

- liqctld service failure affecting some liquidctl devices
- RPM warning about unversioned obsoletes

## [3.0.0] - 2025-10-03

### Removed

- The `coolercontrol-liqctld` package and service has been removed. The service has now been
  embedded in the `coolercontrold` daemon as a child process (!340)

### Added

- Overlay Profiles: Advanced offset controls on top of base Profiles (!347)
- Custom Sensor Offset and parent-child chaining (!346)
- Custom Profile temperature ranges (!344)
- Ability to import and export color schemes
- API changes to help 3rd party integrations
- Keyboard shortcuts for common navigation (!349)
- New device/main menu: drag and drop device and sensor sorting, pinned sensors, host name,
  additional submenu options, color control for device and entity titles, and improved usability and
  performance (!339)
- Firmware-controlled profile support for some AMDGPUs and liquidctl devices. (hwmon device support
  coming soon) (!354)
- Foundation for more advanced device and channel specific controls
- Min and Max CPU core frequency metrics
- Mix Profile difference mix function (!348)
- Ability to change liquidctl log level with environment variable

### Changed

- New Color Picker UI component
- Various logging improvements
- the `liqctld` service logs are now unified with the daemon's normal log output
- Nearly all dependencies upgraded, including several major version changes
- Website and documentation improvements
- Improved metric standards in the UI (!345)
- UI Performance and reloading improvements

### Fixed

- Issue with 24h time format
- Issue with minimum fan duty threshold (!338)
- Various minor bugs and UI issues

## [2.2.2] - 2025-07-18

### Added

- Support for devices that only have non-controllable fans (#465)
- aarch64/arm64 & musl build support (!327, #475)
- Support for raspberry cpus (!329)
- DockerHub builds (!331)
- Alert warmup duration (#466)
- Show all sensors with chart toggle (#480)
- Show running instance when starting desktop app (#453)

### Changed

- Updated dependencies (!336)

### Fixed

- liquidctl Coolit driver support (#467)
- Hover menu not displaying correctly (#468)
- ThinkPad full-speed mode not disengaging (#470)
- Daemon status translation key (!328)
- Duplicate welcome text (#477)
- Minor Alerts related issues (!333)

## [2.2.1] - 2025-06-13

### Added

- Improved security and quality checks for the GitLab CI Pipeline (!312)

### Changed

- Improved test coverage for main control engine (!315)
- Updated copyright year (!316)

### Fixed

- Disabled support for AsusRyujin liquidctl driver (#457)
- Removed no longer used title (!318)

### Security

- Vulnerability issue with dependency (!314)

## [2.2.0] - 2025-05-31

### Added

- Support for older Intel CPUs (#437)
- Support for liquidctl MPGCooler driver (#436)
- Support for liquidctl AsusRyujin driver (#441)
- Support for liquidctl Coolit Hydro GT driver (#442)
- Zero Temperature Drive Power State (#443)
- Project Vision document (!297)
- International Language support i18n (!296 @kuilei0926)
- Fan control and entity creation wizards (!305)

### Changed

- Dependencies updated (!308)
- Reduce log level for drivetemp slow devices (!287)
- Improved code documentation and log messages
- Various minor implementation improvements
- UI Refresh no longer needed for theme changes (!296 @kuilei0926)
- Load libdrm_amdgpu dynamically (!303)

### Fixed

- Added liquidctl initialization retries and max startup delay (#438)
- Badge links in docs (!304 @pallaswept)
- Daemon crash in some rare situations (!294)
- Minor OpenAPI documentation (!301 @caferen)
- Missing metrics for single Dashboards (!306)
- Long float values in Alert logs (!307)

## [2.1.0] - 2025-04-05

### Added

- New Icons including symbolic icon (!279)
- Zero RPM feature support for RDNA3/4 AMD GPU fan control - kernel version dependant (#386)
- Improved RDNA3/4 AMD GPU support, logs and docs (!280)
- Nvidia fan RPM support - upstream work (!270)
- Support for general HWMon device Power metrics (watts) (#422)
- Full-screen support for Web UI & Desktop app (!284)
- Full-Page feature for Dashboards (!284)
- Enhanced system information logs by default (!281)
- Link to Hardware Support docs in Device Settings (!281)

### Changed

- Continued log improvements for various situations (!278)
- Fallback icon logic and badge icon placement improvements (!279)
- Improved liqctld unix socket connection pool handling (!277)
- Improvments to Startup Tour (!281)
- Dependencies updated
- Build pipeline updates

### Fixed

- Issue when setting AseTek AIOs as either modern or legacy versions (#421)
- Handling for devices that do not have a writable pwmN_enable for fan control (!278)
- Issue on some systems with socket connection issues (#424)
- Issue where changing the Profile Type didn't count towards unsaved changes (!285)
- Minimum Profiles reqiured for Mix Profiles are now properly evaluated (!278)

## [2.0.1] - 2025-03-20

### Added

- Introduction Video in repo readme

### Changed

- Improved log messages and log levels
- Improved handling of invalid settings
- Updated URLs and screenshots in app metadata
- Force disable Qt debug logs by default

### Fixed

- Add missing desktop application Qt dependencies for 22.04 Ubuntu based distros (#408) (#409)
- Replaced unsupported JS functions for Qt 6.2.4 (#408) (#409)
- Model type for liquidctl speed profiles (#411)
- URL for hwmon kernel documentation (#414)
- Home icon for new Dashboards in main Tree Menu
- Bypass pwmN_enable setting for devices that don't support it (#407)

## [2.0.0] - 2025-03-15

### Added

- A new custom UI theme (#340)
- Dashboards (#296) (#337) (#356) (#395)
- Power usage display for CPUs and GPUs that support it (#355)
- Desktop notifications for important events and issues
- Alerts to be notified if temperatures or fan speeds exceed an expected range
- Application status badge with daemon health status
- Polling rate controls (#371)
- LCD loop over gifs folder feature (#76) (#364)

### Changed

- Many changes made for this major release for every component
- Modes UX and business logic has been reworked
- Disable (formerly Blacklist) devices and sensors more easily
- New Desktop Application based on Qt, improving usability and performance (#321) (#229) (#282)
- Various Daemon improvements (#237)
- All readme's have been refreshed

### Fixed

- Many small and larger bugs fixed, too many to list.

### Dependency

- Most all dependencies updated to latest versions

## [1.4.5] - 2024-12-14

### Added

- Support for AMD APU processors (#352)
- Support for virtual HWMon drivers (#376)

### Changed

- Use versioned NVML system library

### Fixed

- Infer standard http ports from the URL (#378)
- Incorrect WMClass name in desktop entry (#238)

### Dependency

- Most dependencies updated to latest versions

## [1.4.4] - 2024-11-02

### Added

- Debian Trixie package for Cloudsmith

### Fixed

- issue with sensor updates and chart changes from upstream dependency changes

## [1.4.3] - 2024-11-01

### Added

- Emergency mode for missing devices and sensors for active Profiles (#357)
- Liquidctl integration toggle (#367)

### Removed

- Some pre-1.0 deprecated configuration settings

### Fixed

- Several minor Modes-related issues
- Config saving issue (#359)
- Gnome + Wayland Desktop app / Tauri issue (#358)
- Profile deletion issues (#365,#363)

### Dependency

- All dependencies updated

## [1.4.2] - 2024-09-27

### Changed

- Improved the liquidctl and hwmon duplicate device filter logic
- Dependencies updated

### Fixed

- liquidctl asetek device handling caused crash of liqctld service (#348)
- Workaround for upstream issue causing checksum failure in networkless build setups (#349)

## [1.4.1] - 2024-09-21

### Added

- Show information popup about running lm-sensors (!223 @caferen)
- Option for the daemon to compress API responses (#318)
- Non-controllable HWMon fans are now viewable in the UI (!216 & !229)
- New Krakens to the duplicate device filter (!218)

### Changed

- Arch install instructions to include coolercontrol-bin AUR repository (!221 @caferen)
- Improved daemon logging and hwmon handling (!216)
- Daemon efficiency improvements (!219)
- Increased UI default timeout to help with some edge cases (#346)
- Allow doc_lazy_continuation lint (!226 @caferen)
- All dependencies updated

### Fixed

- Memory leak issue with the desktop application (#345)
- Display issue with custom sensor dialog when selection > 10 temp sources (#336)
- Crash when opening second instance of desktop application (#325)

## [1.4.0] - 2024-07-27

### Added

- AMD GPU RDNA 3 fan control (#265)
- NVML usage for Nvidia GPUs (replaces CLI tools) (#288)
- Proper AMD GPU device names from DRM drivers
- PCI ID lookup for hwmon devices
- Various testing scripts for testers
- Option to disable duplicate liquidctl/hwmon device filter
- Vendored build artifacts

### Changed

- Major Tauri upgrade - includes dependencies (#286)
- Chart rpm/mhz axis scaling limits removed
- Improved testing artifacts in merge pipelines
- Cleaned up some log messages
- Force application of speed setting when applying a Profile to an additional device channel
- Extend max sensor name length and overflow (#315)

### Fixed

- Top level icon under KDE Wayland (#291)
- Issue with GPU Frequency chart colors not persisting
- Handle hwmon fan rpm invalid value
- Issue with empty liquidctl device initialization response for some devices (#299)

## [1.3.0] - 2024-06-07

### Added

- Refresh button added for the Desktop Tauri App
- Logout option added along with some minor improvements to the login flow
- Webkit2gtk lib to Desktop App AppImage
- Additionally saving of window state when closing to tray
- Examples for users to see how to script their own device changes
- System and High contrast color themes options
- Frequency sensors for CPUs and AMD GPUs
- Zoned chart tooltips and zoom ability
- Scrolling ability for all number inputs
- Time range for time charts is now customizable and scrollable

### Changed

- UI theming has been significantly refactored for improved maintainability
- Moved temperature sensor names out of the status entity
- Status properties are now serialized dynamically to reduce overhead
- Improved some internal logic
- Documentation improvements
- All major and transient dependencies updated
- Moved chart line thickness setting into overview chart with dropdown selection
- Upon starting a second instance of the desktop app, the running instance's window will show

### Removed

- Deprecated settings options since 1.0
- Deprecated APIs since 1.0
- Deprecated Compose Repository since 1.0
- Other deprecated UI and daemon code since 1.0

### Fixed

- A Mode was not sometimes shown as active when it contained a default Profile
- Wrong line assigned to sensor in the overview chart for some liquidctl devices
- Firefox bookmark and history icon bug workaround

## [1.2.2] - 2024-04-13

### Fixed

- initial window size on KDE Wayland caused window decorations to be off-screen on some setups
- scheduler panic caused by upstream issue when DST ends

## [1.2.1] - 2024-04-02

### Fixed

- issue with swapping Mix Profiles on the same device channel

## [1.2.0] - 2024-04-01

### Added

- special handling for ipv4-only and ipv6-only setups for UI and daemon (#252)
- config file configuration for the daemon address and port (#249)
- configuration of the UIs connection to the daemon in the UI (#249)
- improved handling of network latency for remote setups (#249)
- system-wide Modes feature (#226)
- NixOS install info to the readme (#134)
- file based Custom Sensors (#157)
- Ubuntu 24.04 release package
- LMDE Bookworm release package
- Mix Profiles feature (#123 from @caferen)
- openrc service files for those that want them

### Changed

- improved RPM Axis scaling for Overview
- nearly all dependencies updated
- improved settings sidebar
- cleaned up logs
- some smaller UI improvements
- improved Custom Sensor adding and editing UX
- startup delay option for the Desktop App

### Fixed

- Fedora package dependency (#257)
- responsive UI issue with the Profile Editor in the Desktop App
- use default C locale for all shell commands (#260)
- add additional Xauth location (#259)

## [1.1.1] - 2024-02-16

### Added

- show current temps in temp source dropdown menus (#247)

### Changed

- unified enter key action and input text field focus for dialogs

### Fixed

- incorrect dependency in the debian package
- improved login handling (#251)
- rpm scaling issues
- manual duty control for non-duty reporting devices (#254)
- changing-xauth handling for NVidia devices (#250)

### Dependency Updates

- NPM:
- uplot to 1.6.30
- vue to 3.4.19
- element-plus to 2.5.5
- types/node to 20.11.19
- vitejs/plugin-vue to 5.0.4
- sass to 1.71.0
- vite to 5.1.3

## [1.1.0] - 2024-01-31

### Added

- improved documentation for Ubuntu 22.04/LTS installations (#241)
- setting to enable starting of desktop application in system tray (#230)
- sensors-type controls, rpms, and line thickness settings added for system overview graph (#231)
- system overview table option (#153)
- delta mix function for custom sensors (#246)
- improved security with authentication for device-controlling APIs

### Fixed

- UI issue with Corsair Command Pro (#240)
- outdated WM Class in desktop file (#238)
- uncaught exception in coolercontrol-liqctld for certain errors (#244)

### Dependency Updates

- Rust:
- env_logger to 0.10.2
- clap to 4.4.18
- chrono to 0.4.33
- regex to 1.10.3
- gifski to 1.14.1
- imgref to 1.10.1
- uuid to 1.7.0
- all tauri plugins to current git version
- NPM:
- axios to 1.6.7
- vue to 3.4.15
- element-lus to 2.5.3
- types/uuid to 9.0.8
- types/node to 20.11.7
- vue/test-utils to 2.4.4
- jsdom to 24.0.0
- sass to 1.70.0
- vite to 5.0.12
- vitest to 1.2.1

## [1.0.4] - 2024-01-15

### Changed

- nothing, but there was a hiccup in the committed release changes.

## [1.0.3] - 2024-01-15

### Fixed

- issue with Intel CPUs not being correctly parsed

## [1.0.2] - 2024-01-14

### Changed

- hwmon label handling is improved
- UI channel sorting is improved and follows hwmon order
- internal hwmon channel names are now proper identifiers

### Fixed

- issue where a specific liquidctl device option caused the UI menu to not display
- systemd issue where restart wasn't working as intended
- issue where certain hwmon devices returned an error when trying to control the fans

### Dependency Updates

- Rust:
- clap to 4.4.16
- anyhow to 1.0.79
- tokio to 1.35.1
- tokio-graceful-shutdown to 0.14.2
- async-trait to 0.1.77
- actix-web to 4.4.1
- actix-cors to 0.6.5
- serde to 1.0.195
- serde_json to 1.0.111
- sysinfo to 0.30.5
- psutil to 3.3.0
- nu-glob to 0.89.0
- yata to 0.6.3
- tiny-skia to 0.11.3
- image to 0.24.8
- tauri-build to 1.5.1
- tauri to 1.5.4
- all tauri plugins to c5e8cd31ec86ba7ddcd524f1377a5bf09229fb9e
- NPM:
- axios to 1.6.5
- uplot to 1.6.28
- vue to 3.4.13
- element-plus to 2.5.1
- types/node to 20.11.0
- vitejs/plugin-vue to 5.0.3
- jsdom to 23.2.0
- sass to 1.69.7
- vite to 5.0.11
- vitest to 1.2.0

## [1.0.1] - 2024-01-12

### Changed

- default profiles and functions are now more clearly non-selectable and non-editable

### Fixed

- issue with some systems not showing fans as controllable

## [1.0.0] - 2024-01-07

### Added

- re-usable profiles
- functions with advanced settings to apply to your profiles
- Custom Sensors
- devices can be easily blacklisted
- lots of new features in the new UI (too many to list)

### Changed

- The entire GUI has been rebuild from the ground up

### Fixed

- lots of smaller bugs (too many to list)
- the issues with the previous UI should be gone now, as it's been replaced

### Removed

- the Python GUI application

### Deprecated

- the previous settings still work, but are no longer supported and will be removed in the future

### Security

- inter-daemon communication is better protected

### Dependency Updates

- nearly everything has been updated

## [0.17.3] - 2023-12-15

### Added

- new UI preview: time format option for the graphs

### Fixed

- issue with the debian builds causing error on some systems
- new ui preview issues:
- delay read-ahead sometimes wasn't working as expected
- some font scaling issues
- improved profiled editor tooltip rendering
- refactored the safety latch so that fan curves are hit after a period of time, regardless of
  thresholds set
- other small issues

### Removed

- builds and packing for Fedora 37 > EOL

### Dependency Updates

- PySide to 6.6.1
- Nuitka to 1.9.5

## [0.17.2] - 2023-11-28

### Added

- ubuntu 23.10 package
- fedora 39 package
- preview of new UI available from Daemon (#220)
- improved image processor for images and gifs in the daemon
- new APIs for the new UI in the daemon
- LED sync, entity removal sync, and reset handling in the daemon
- backup configuration files CLI option for coolercontrold
- migrate to profiles CLI option for coolercontrold

### Changed

- improved installation documentation (#208, #218)
- improved hardware support documentation
- improved no-device handling for coolercontrol-liqctld
- improved and cleaned up output log messages for the coolercontrold systemd daemon
- various cleanups, refactorings and improvements for the coolercontrold daemon in preparation for
  the new UI
- updated CI images and pipelines to support the new UI
- improved configuration handling in the daemon
- more flexible LCD implementation to handle upcoming device screens with various sizes
- use our own error struct in the daemon API for proper error handling & messages
- added a minimum startup pause in the daemon to better handle devices that are time-sensitive at
  boot time
- refactoring of the daemon's profile engine, enabling a whole new suite of features and improving
  maintainability
- all statuses are now cleared upon waking from sleep for an improved UX
- temps are float64 values throughout the daemon enhancing maintainability and sensitivity
  especially for water coolers
- allowed more restarts of the systemd daemon for cases when blacklisting multiple devices in
  succession
- cleaned up new cargo build warnings
- copyrights updated

### Fixed

- handle coolercontrol-liqctld logging levels appropriately (#214)

### Dependency Updates

- Python:
- setproctitle to 1.3.3
- fastapi to 0.104.1
- uvicorn to 0.24.0
- orjson to 3.9.10
- nuitka to 1.9 (gui) and 1.8.6 (liqctld)
- pyside6 to 6.6.0
- matplotlib to 3.8.2
- numpy to 1.26.2
- Rust:
- env_logger to 0.10.1
- systemd-journal-logger to 2.1.1
- clap to 4.4.8
- tokio to 1.34.0
- tokio-graceful-shutdown to 0.14.1
- async-trait to 0.1.74
- reqwest to 0.11.22
- serde to 1.0.193
- serde_json to 1.0.108
- chrono to 0.4.31
- regex to 1.10.2
- const_format to 0.2.32
- nu-glob to 0.87.1
- sha2 to 0.10.8
- toml_edit to 0.21.0
- nix to 0.27.1
- yata to 0.6.2
- tiny-skia to 0.11.2
- gifski to 1.13.1
- uuid to 1.6.1

## [0.17.1] - 2023-09-13

### Added

- more xauthority locations for nvidia support (!102 from @pallaswept)
- support for Corsair iCUE Elite RGB H100i, H150i (#158)

### Changed

- improved systemd service handling for rpm and deb packaging (#145)
- various documentation improvements
- improved documentation for installation from source (#148)
- improved shell command and nvidia-startup resiliency (#156 with @pallaswept)

### Fixed

- issue with the corsair commander pro and internal fan profiles (#155)

### Dependency Updates

- Python:
- liquidctl to 1.13.0
- fastapi to 0.103.1
- uvicorn to 0.23.2
- orjson to 3.9.7
- nuitka to 1.8.1
- pyside6 to 6.5.2
- apscheduler to 3.10.4
- numpy to 1.25.2
- matplotlib to 3.7.3
- Rust:
- log to 0.4.20
- clap to 4.4.2
- anyhow to 1.0.75
- tokio to 1.32.0
- tokio-graceful-shutdown to 0.13.0
- async-trait to 0.1.73
- actix-web to 4.4.0
- reqwest to 0.11.20
- serde to 1.0.188
- serde_json to 1.0.106
- sysinfo to 0.29.10
- chrono to 0.4.30
- strum to 0.25.0
- regex to 1.9.5
- signal-hook to 0.3.17
- nu-glob to 0.83.0
- toml_edit to 0.19.15
- nix to 0.26.4
- uuid to 1.4.1

## [0.17.0] - 2023-07-16

### Added

- wiki pages for detailed documentation, instead of the ever-growing readme file
- packages for Ubuntu 23.04
- packages for Kali Rolling Linux (#136)
- max temperatures to composite device temperatures (part 1 of #123)

### Changed

- replaced the GUI window frame code for improved handling by all window managers (!87)
- adjusted some colors and small layout changes for the GUI
- Kraken Z LCD screen resets to default firmware screen on shutdown (!88)
- dynamic temp handling set to false/off by default (#138)
- various improvements to the documentation
- log level handling improvements for the daemon
- updated the application metadata
- updated the demo video

### Fixed

- reduced GUI segfaults when using the color picker
- poetry script name in readme (!92 from @trumank)
- issue with devices with extreme latency (#115)
- incorrect command spelling (!93 from @ChrisToxz1)
- issue with speed control graph and multiple cpu & gpu temps (#135)
- issue with multiple physical CPUs (#116 and #129)
- rust compile warnings introduced with 1.70.0

### Removed

- package support for Fedora 36 (EoL)

### Dependency Updates

- CI Pipelines:
- Rust to 1.71.0
- Poetry to 1.5.1
- Python to 3.11.4
- Python:
- pyside6 to 6.5.1.1
- matplotlib to 3.7.2
- numpy to 1.25.1
- requests to 2.31.0
- fastapi to 0.100.0
- uvicorn to 0.23.0
- orjson to 3.9.2
- nuitka to 1.7.5
- Rust:
- log to 0.4.19
- systemd-journal-logger to 1.0.0
- clap to 4.3.12
- anyhow to 1.0.71
- tokio to 1.29.1
- async-trait to 0.1.71
- reqwest to 0.11.18
- serde to 1.0.171
- serde_json to 1.0.102
- zbus to 3.14.1
- sysinfo to 0.29.4
- chrono to 0.4.26
- regex to 1.9.1
- signal-hook to 0.3.16
- const-format to 0.2.31
- nu-glob to 0.80.0
- sha2 to 0.10.7
- toml_edit to 0.19.14
- tiny-skia to 0.11.1
- uuid to 1.4.0

## [0.16.0] - 2023-04-23

### Added

- doc about ignoring a particular device
- Multiple Physical CPU support
- setting to show all CPU Core Temperatures if available (Intel)
- setting to allow ThinkPads to spin fans up to maximum possible speed (full-speed) when set to
  100%, with warning dialogs
- convenience dialog to enable ThinkPad fan control
- link in documentation to the default daemon config file for reference
- doc about potential breaking changes while using pre-stable/development release versions

### Changed

- made some improvements tot he CI pipelines
- improvements to sensor data gathering
- all CPU temperature sensors are now available, instead of automatically using a default sensor
  (affects previous settings)
- all AMD GPU temperature sensors are now available (affects previous settings)
- improved CPU temperature control in ThinkPads by showing all available kernel CPU temperature
  sensors, instead of the thinkpad_acpi smoothed CPU values
- improved logging
- LCD single temperature image tweaks
- enable building project with Python 3.11

### Fixed

- issue with LCD temp selection - displayed name
- issue when temperature name changes
- issue with Nvidia GPUs where the display id was not correct when retrieving and applying settings
- display issue with new kraken x53 hwmon driver
- issue with breaking change with PySide 6.5.0
- issue with incorrect GPU temperature when both AMD and Nvidia GPUs were present
- issue with uniquely identifying certain liquidctl devices

### Dependency Updates

- Python:
- nuitka to 1.5.6
- fastapi to 0.95.1
- uvicorn to 0.21.1
- orjson to 3.8.10
- pyside to 6.5.0
- apscheduler to 3.10.1
- matplotlib to 3.7.1
- Rust:
- clap to 4.2.2
- anyhow to 1.0.70
- tokio to 1.27.0
- async-trait to 0.1.68
- actix-web to 4.3.1
- reqwest to 0.11.16
- serde to 1.0.160
- serde_json to 1.0.96
- zbus to 3.11.1
- sysinfo to 0.28.4
- chrono to 0.4.24
- regex to 1.7.3
- nu-glob to 0.78.0
- toml_edit to 0.19.8
- uuid to 0.3.1

## [0.15.0] - 2023-03-14

### Changed

- improved performance and timing of status snapshots for all devices

### Added

- ability to disable a specific device from the daemon config file
- new custom LCD mode for the Kraken Z to display any single temperature

### Dependencies Added

- tiny-skia 0.8.3
- ril 0.9.0

## [0.14.6] - 2023-03-01

### Fixed

- NVIDIA card issue where nvidia-settings is not installed - required for fan control
- speed control graph bug when changing temp sources from dependency update

## [0.14.5] - 2023-02-27

### Changed

- disabled Matplotlib frame caching, which is on by default - no current need.

### Added

- ability to control all the fans on newer Nvidia cards - where more than one fan control is present
- OpenSuse Tumbleweed packaging

### Dependency Updates

- build images to Rust 1.67.1
- matplotlib to 3.7.0
- numpy to 1.24.2
- nuitka to 1.4.8
- fastapi to 0.92.0
- orjson to 3.8.6
- clap to 4.1.6
- anyhow to 1.0.69
- tokio to 1.25.0
- async-trait to 0.1.64
- serde_json to 1.0.93
- zbus to 3.10.0
- sysinfo to 0.28.0
- heck to 0.4.1
- signal-hook to 0.3.15
- nu-glob to 0.75.0
- toml_edit to 0.19.3
- uuid to 1.3.0

## [0.14.4] - 2023-02-14

### Changed

- connecting to each liquidctl device now happens automatically so that certain device properties
  are available at startup
- UI status updates begin at a more appropriate time
- enabled concurrent requests from the UI
- extended the UI request timeout to handle long-lasting requests

### Fixed

- issues with settings the LCD screen on Kraken Z models
- UI issue with AMD GPUs that have more than one temperature sensor
- issue where some devices like the Kraken 2 models where not properly detected
- UI issue when composite temperatures are off and there is a sync error
- a thread locking issue when applying settings that take a good amount of time

### Added

- automatic xauthority location injection for nvidia-settings
- helpful error messages when running as/not as root for the various applications
- checks for various edge cases for better stability
- automatic adjustment of update timing if there is a timing conflict between gui and daemon
- a caching layer for status updates that are blocked by long-running tasks, keeping status response
  times consistent

## [0.14.3] - 2023-02-09

### Changed

- improved the AUR installation documentation and FAQ

### Fixed

- issue with correctly communicating with Kraken Z devices

## [0.14.2] - 2023-02-07

### Changed

- removed openssl system dependency
- improved the debug logging documentation

### Fixed

- issue with coolercontrol-liqctld running as a systemd service
- main issue with using nvidia-settings from a systemd service

## [0.14.1] - 2023-02-06

### Changed

- restructured liqctld package to enable proper python installation

## [0.14.0] - 2023-02-05

### Changed

- project name changed to CoolerControl
- project split is three distinct applications
- GUI component is now a daemon client called coolercontrol
- improved liquidctl multi-device communication
- improved device identity uniqueness
- settings applied on boot instead of Desktop login
- two appimages are now needed, as one needs sudo permissions
- updated all dependencies to current working version
- improved some GUI options with more live-changes
- lots of little improvements throughout the project

### Fixed

- lots of smaller bugs found during the re-write

### Added

- systemd service for all packages
- system daemon called coolercontrold
- system daemon for liquidctl called coolercontrol-liqctld
- all configurations are saved to editable configuration files
- system packaging for deb and rpm for most distributions
- new AUR package that is not compatible with the deprecated coolero one

### Removed

- support for Flatpak - doesn't work for system level applications

### Deprecated

- the Coolero application

## [0.13.3] - 2022-12-11

### Changed

- updated PySide and shiboken to 6.4.1
- updated Numpy to 1.23.5
- updated development & other minor dependencies

### Fixed

- issue with AuraLed devices where initialization reset settings

## [0.13.2] - 2022-11-13

### Changed

- updated PySide and shiboken to 6.4.0.1
- updated matplotlib to 3.6.2
- updated nuitka to 1.2
- updated other minor dependencies
- improved log formatting

### Fixed

- missing pump controls for some Corsair Commander Core devices
- error from Speed Scheduler for device channels that have no duty
- issue in Speed Scheduler where float-based duties lead to unnecessarily applying settings for some
  devices

## [0.13.1] - 2022-10-20

### Changed

- updated liquidctl to 1.11.1
- updated other minor dependencies

### Fixed

- issue where certain devices were receiving a communication timeout - liquidctl issue

## [0.13.0] - 2022-10-16

### Added

- experimental support for Kraken Z LCD screens
- experimental support for aquacomputer devices from liquidctl
- experimental support for Corsair Commander Core XT from liquidctl
- a --debug-liquidctl option for liquidctl debugging

### Changed

- improved liquidctl debugging information
- Allow Coolero to keep running if liquidctl has an unexpected error
- parsing of new temperature sensor names for Corsair PSUs
- upgraded liquidctl to 1.11.0
- upgraded matplotlib to 3.6.1
- upgraded numpy to 1.23.4
- upgrade other minor dependencies

### Fixed

- issue with auto-wired testing VirtualBusDevice

## [0.12.9] - 2022-09-30

### Added

- ability to handle hwmon devices that only have manual mode, aka do not have a pwm_enable
  implementation

### Changed

- the internal scheduler now periodically checks the actual status, to help with instances where an
  external program/command has changed values
- improved logging for hwmon driver limitations
- update CI docker images
- upgraded minor dependencies
- upgraded matplotlib to 3.6.0
- upgraded PySide to 6.3.2
- upgraded Numpy to 1.23.3

### Fixed

- an issue with changed animation initilization with the matplotlib 3.6.0 upgrade

## [0.12.8] - 2022-08-26

### Changed

- updated dependencies

### Fixed

- issue with possible segfault after resuming from sleep

## [0.12.7] - 2022-08-13

### Changed

- updated matplotlib to 3.5.3
- updated other minor dependencies
- removed access to SUPPORTED_DEVICES in liquidctl
- made various documentation improvements

### Added

- experimental support for asus aura led motherboard controller
- CLI option to be able to skip device initialization for script users

## [0.12.6] - 2022-08-04

### Fixed

- added missing udev rules for access to newly supported devices

## [0.12.5] - 2022-08-02

### Fixed

- issue with log error and very fast desktop notifications

### Changed

- updated several dependencies

### Added

- experimental support for NZXT H1 V2 devices

## [0.12.4] - 2022-07-24

### Changed

- Hwmon Support is no longer considered experimental and read-only support is enabled by default
- updated minor dependencies
- readme updates

### Added

- liquidctl devices are reinitialized and settings are re-applied after resume from sleep/hibernate
- dialog window about enabling hwmon write access

## [0.12.3] - 2022-07-12

### Fixed

- Error when applying lighting settings
- Sizing issue for startup delay settings box

### Added

- Pump controls and duty for Corsair Hydro - Pro, Pro XT, and Platinum coolers

## [0.12.2] - 2022-07-10

### Changed

- updated minor dependencies
- updated Nuitka to 0.9.4
- updated Numpy to 1.23.1
- updated liquidctl to 1.10.0
- cleaned up info logs

### Added

- a startup delay settings option

## [0.12.1] - 2022-06-26

### Fixed

- Error when choosing no to close the application

### Changed

- updated Nuitka to 0.9
- updated Numpy to 1.23.0

### Added

- a pwm / dc toggle for hwmon fans that support it

## [0.12.0] - 2022-06-22

### Fixed

- UI error happening after long time of inactive animation
- socket error on connection close and shutdown

### Changed

- updated PySide to 6.3.1
- minor dependencies updated
- improved documentation

### Added

- enabled and updated the collapsable sidebar
- client side window decorations enabling improved wayland support and unified look across
  environments

## [0.11.1] - 2022-06-01

### Fixed

- issue that context menu would stay open when it shouldn't and could cause incorrect profiles to be
  applied

### Added

- copy and paste ability to the custom profile context menu

## [0.11.0] - 2022-05-30

### Fixed

- issue with hwmon devices being cut off on the info page

### Changed

- UI layout improvements
- system tray menu simplified
- improved cursor motion on custom profile graphs
- improved speed graph scaling of various temperature ranges
- refactored daemon implementation for future expansion
- updated minor dependencies
- updated pylint, psutil, nuitka, and numpy

### Added

- right and left movement of custom profile points
- context menu for adding, removing, editing, and resetting profile points
- show current temp and duty on hover
- displayed active area for grabbing points with the mouse
- keyboard support for moving highlighted custom profile points
- keyboard and mouse support for editing profile points and inputting point values
- systemd daemon support

## [0.10.3] - 2022-05-14

### Fixed

- issue when updating the AUR package when already running
- speed graph errors on devices with no duty and 0 rpm being reported
- background efficiency issue when starting minimized

## [0.10.2] - 2022-05-08

### Changed

- greatly reduced cpu usage when running in the system tray
- enhanced the Info page
- updated all links to use gitlab group coolero
- updated code of conduct
- upgraded pylint
- upgraded matplotlib to 1.5.2

### Added

- readme section for Adding Device Support
- CONTRIBUTING.md guidelines
- RELEASING.md guide

## [0.10.1] - 2022-05-02

### Fixed

- issue with Flatpak install on Wayland
- adjusted hwmon filter to always show laptop fans

## [0.10.0] - 2022-04-30

### Changed

- app window size and position are saved by default
- speed profiles are now applied on change - UX is now the same across all control types
- some performance and complexity improvements
- readme improvements
- settings page improvements
- pyside updated to 6.3.0
- setproctitle updated to 1.2.3
- other various dependency updates

### Added

- HWMon support (experimental) - this is a big change for coolero - see the readme
- option to disable dynamic temperature handling of cpu and gpu devices
- display notification when clearing scheduled profiles through None
- None and Default profile states are persisted like the others - they are now more of a usable
  option
- option --export-profiles to export last applied profiles in a usable format to the console -
  helpful for scripting
- composite temperatures like averages and deltas - and an associated option to enable them

### Fixed

- issue where more than one instance of the app could run at a time - using temporary lock file
- showing of random little windows during UI initialization
- edge case where device duty line was incorrect
- issue with flatpak debug log file

## [0.9.0] - 2022-04-06

### Changed

- liquidctl updated to 1.9.1
- mypy updated to 0.942
- pylint updated to 2.13.4
- nuitka updated to 0.7.7
- jeepney updated to 0.8.0
- improved cpu status algorithm efficiency
- due to liquidctl update some devices now read their status from hwmon

### Added

- copyright notice profiles for pycharm
- thinkpad - cpu temp is now read from correct hwmon device
- system overview legend now animates with transparency for improved readability
- fan control for Corsair Commander Core devices

### Fixed

- startup error where there was no gpu name for amd Radeon Graphics chips

## [0.8.5] - 2022-03-25

### Changed

- PySide updated to 6.2.4
- Nuitka updated to 0.7.6
- improved desktop notification experience for KDE

### Added

- log uncaught exceptions with the logger
- info about right click to zoom in system overview

### Fixed

- lighting issue due to recent refactoring
- legacy Kraken device recognition
- legacy Kraken lighting mode issues

## [0.8.4] - 2022-03-19

### Changed

- updated various dependencies
- improvements to the info page

### Added

- keyboard shortcut to reset custom profiles to default
- setting to use a brighter tray icon for visibility
- multiple gpu support for both the system overview and the speed control graphs

## [0.8.3] - 2022-03-07

### Changed

- improved value text positioning for all resolutions in control graphs
- adjust screenshots to standard metadata sizes

### Added

- display temperature value in control graphs
- start minimized setting
- minimize to tray setting

### Fixed

- animation artifacts are finally gone

## [0.8.2] - 2022-03-05

### Changed

- readme improvements
- updated liquidctl udev rules

### Added

- support for Corsair PSUs
- support for NZXT E-series PSUs
- support for Asetek Pro liquid coolers
- support for Hydro Platinum and Pro XT all-in-one liquid coolers
- experimental support for Corsair Commander Core and iCUE

### Fixed

- cpu temp for AMD FX processors
- lighting display issue when no lighting channels were present

## [0.8.1] - 2022-03-03

### Changed

- improved desktop notification so they're not so spamy
- improved wording in apply-udev-rules notice
- updated several dependencies
- when AMD and NVidia GPUs are present, prefer AMD

### Added

- AUR Package
- setting to disable desktop notification if desired
- support for Gigabyte RGB Fusion 2 devices

### Fixed

- issue when only fan controllers without any temp sources are connected
- show only composite temp sources when more than one temp source is available
- issue with cpu name when running a non-english local

## [0.8.0] - 2022-02-27

### Changed

- Breaking Change - new module structure, previously Saved Settings are unfortunately not supported
  and migration is not possible, meaning settings need to be re-applied after this update. This is
  needed to make installation in some situations possible. Preferred to do it now in the development
  version rather than later and foresee no need for such a change again in the future.
- improved appimage builds
- some dependency updates

### Added

- linux distro information in version output

### Fixed

- issue when applying lighting settings for the first time and previous Mode was None
- issue with manual scheduling and the respective threshold counter per setting

## [0.7.2] - 2022-02-23

### Changed

- project upgraded to python 3.10 for all builds

### Fixed

- scheduled jobs loop issue when resuming from sleep

## [0.7.1] - 2022-02-18

### Changed

- description, screenshot and demo updates
- improved background scheduling further
- minor dependency updates

### Added

- warning to update legacy firmware for Kraken2 devices
- desktop notifications for lighting changes

### Fixed

- simultaneous device communication issues
- applying settings at startup issue

## [0.7.0] - 2022-02-12

### Changed

- various readme improvements
- improved startup progress levels
- various code refactorings as things grow
- dependency updates
- small UI improvements

### Added

- lighting control for all currently supported devices
- lighting settings are saved
- handling of sync lighting channels
- system info to version and debug output

### Fixed

- pytest issues after upgrade
- same background scheduler is used for all device communication, reducing concurrency issues

## [0.6.5] - 2022-02-04

### Changed

- updated udev rules list
- upgraded major dependencies

### Added

- support for Aestek Devices (NZXT Legacy & EVGA Coolers)

### Fixed

- issue with dynamic buttons when there are more than 9 devices detected

## [0.6.4] - 2022-02-02

### Changed

- updated dependencies

### Added

- desktop notifications when applying settings to a device

## [0.6.3] - 2022-02-01

### Fixed

- store enough data to fill the new display
- version bump script for application version

## [0.6.2] - 2022-01-31

### Changed

- improved overview & control graph efficiency and responsiveness

### Added

- saving of set profiles
- apply shown profile with simple click
- zooming of system overview graph
- clearing last applied profile by applying the None profile
- setting for applying last applied profile at startup

### Fixed

- pipeline badge
- bug in control graph duty text with graph scaling
- issue with two SmartDevice2 devices

## [0.6.1] - 2022-01-29

### Changed

- use gitlab package registry for release packages
- values of 0% in graphs are now clearly visible
- improved temperature source names and internal flexibility
- use single scheduler for all device communication
- show number meanings on settings page

### Added

- liquid temps are now available as temp sources to other devices
- new temp source composition available, average of all temps.

### Fixed

- issue when NZXT SmartDevice2 fans were set to 0%
- internal and external profile speed setting is handled correctly

## [0.6.0] - 2022-01-25

### Changed

- improved AppImage building
- improved gitlab pipelines
- gitlab issue and MR templates
- removed the display of noise sensors for now

### Added

- New Logo and icons!
- basic application keyboard shortcuts
- smart device speed scheduling - much less usb traffic
- CLI option --add-udev-rule for manual application

### Fixed

- unclean shutdowns
- issue which didn't allow devices without temp probes to be speed controlled
- conflict with same temp source name with multiple devices
- issue with udev rules not being copied for AppImage and Flatpak installs

## [0.5.5] - 2022-01-21

### Changed

- update dependencies: mypy, types-psutil, nuitka, numpy, liquidctl
- set process name explicitly for easier performance profiling
- improved graph data handling and calculation efficiency

### Added

- support for the zenpower driver for cpu temps
- graph smoothing for cpu & gpu rapid fluctuations
- option to set the overview graph duration
- some small preparations for lighting control
- support for liquidctl SmartDevice2 driver
- support for liquidctl SmartDevice driver
- support for 'sync' channel for fans/pumps on devices that support it

### Fixed

- install from source package name
- catch StopIteration exceptions when looking for non-existent plot lines
- issue with getting mock statuses while testing

## [0.5.4] - 2022-01-10

### Changed

- updated app metadata for releases on flathub

## [0.5.3] - 2022-01-10

### Added

- new service for nvidia gpus that also includes the fan duty for cards that support it
- flatpak as official installation method

### Changed

- improved system tray features

## [0.5.2] - 2022-01-08

### Changed

- moved all metadata info into a single directory

### Fixed

- AppImage updating now correctly verifies image GPG signature

## [0.5.1] - 2022-01-07

### Fixed

- Issue with AppImage running on Wayland. Now runs on every distro tested so far.

## [0.5.0] - 2022-01-06

### Added

- different graph line colors for every temp and duty source
- test mocks and toggle
- device id included in legend when multiple versions of the same device are detected
- support for multiple temp sensors per device
- max temp for a device is now reflected in the UI
- speed profile markers now reflect how many are allowed per a device
- support for Corsair Commander Pro
- shaded region in speed control graph for min and max allowed duties

### Changed

- refactorings for better readability
- improved line picking and movement in speed control graphs
- improved speed control UI & UX
- logfiles for debugging are now put in /tmp
- cleaned up debug log output
- extended TempSource class for better interoperability between devices
- improved Status extraction runtime
- readme improvements
- updated dependencies
- improved AppImage update process

### Fixed

- handle shutdown exceptions
- kraken2 extraction issue
- issue with no liquidctl initialization status
- multiple devices and their channel buttons are now correctly displayed
- handling of detected unsupported devices
- incorrect error when scheduling speed
- issue with Kraken M22 because it has no cooling support
- display only reporting channels
- each channel now has its own observers for applying settings
- handle known and expected warnings from dynamic canvas resizing

## [0.4.1] - 2021-12-31

### Added

- support for multiple temperature probes per device
- duty calculation based on profile when the device doesn't provide it
- AppImage is now self-updatable and GPG signed
- Check for Updates setting for AppImages

### Changed

- updated readme with new screenshots and download banner
- updated dependencies

### Fixed

- issue when trying to set fixed speeds when using cpu or gpu as temp source

## [0.4.0] - 2021-12-26

### Added

- show/hide option
- exit confirmation with setting toggle
- different colors per device
- manual UI Scaling setting

### Changed

- updated dependencies, mypy, numpy, etc
- improved animations
- appimage improvements
- several small UI improvements
- left menu now open with logo

### Fixed

- issue with window size and position saving
- issue with system overview label
- issue with channel button toggle on column animation

## [0.3.2] - 2021-12-14

### Added

- support for Kraken X2 devices

### Changed

- updated dependencies, most notably matplotlib and PySide.
- appimage improvements

## [0.3.1] - 2021-12-09

### Added

- support for Kraken Z3 devices

### Changed

- dependency updates
- readme update

## [0.3.0] - 2021-12-04

### Added

- cpu and gpu speed profile feature
- speed profile feature for devices that don't support it natively
- light theme setting

### Changed

- added FAQ to readme
- some package building improvements
- various small improvements

### Fixed

- UI performance issue by disabling custom title bar

## [0.2.2] - 2021-11-30

### Added

- working flatpak package with temporary submodule
- working nuitka compilation
- info about package files in readme

### Changed

- improved build scripts

### Fixed

- issue with apscheduler and compilation

## [0.2.1] - 2021-11-20

### Added

- version bump script & make target
- working snap package
- working pyinstaller packaging

### Changed

- MatPlotLib updated to 3.5.0 stable release
- improved debug log output
- various small code improvements

### Fixed

- handle permissions error when getting cpu info

## [0.2.0] - 2021-11-14

### Added

- Feature toggles to be able to develop multiple features more easily
- Feature to allow users to choose which lines are displayed on the system overview graph

### Fixed

- Now handling errors in main UI startup logic - log and quit
- Issue with release pipeline and release description
- Remaining MyPy issues

### Changed

- updated readme badges

## [0.1.0] - 2021-11-13

### Added

- Working application for some main features & components - much more to come
- System overview graph for detected cpu, gpu and device information
- Setting option for saving the window size
- Info page with basic system information
- Dynamic pages for different devices
- Dynamic device buttons fan and pump channel controls
- Speed control UI - easily controllable graph for fixed and custom speed profiles
- Splash screen for initialization progress
- Initialization dialog window for adding udev rules
- Ability to automatically add liquidctl udev rules for the user upon confirmation
- Unified UI color theme
- Custom widgets and canvases
- Support for liquidctl Kraken X3 driver
- Poetry scripts for main actions
- Editorconfig for styling
