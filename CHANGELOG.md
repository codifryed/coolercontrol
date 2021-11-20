# Changelog

<!--
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).
Release notes are automatically generated from this file and git tags.
-->

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
