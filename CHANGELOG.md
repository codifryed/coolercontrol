# Changelog

<!--
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).
Release notes are automatically generated from this file and git tags.
-->

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
