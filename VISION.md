# The CoolerControl Vision

This is a document to help clarify the project's scope and is subject to change in the future.

## Mission

To make quality cooling control on Linux accessible to everyone.

## Maintainability

CC is maintained by a small core group of developers who volunteer their free time. This means that
for the project to remain sustainable, it cannot support a bloated feature set nor features that
require excessive maintenance. This is taken into account for many things. For example, "Nice to
have" feature priority will be lower than more essential/impactful features.

## Device Support

For cooling device control, CC directly accesses the HWMon Linux Kernel subsystem and additionally
integrates with liquidctl drivers. Hardware drivers are made available by upstream contributors. CC
generally avoids interacting directly with hardware devices and is too large a scope for this
project.

## LCD and Lighting

CoolerControl holds the connection to the devices it monitors. USB access is serial, so the daemon
serializes every request to a device: sensor reads and any screen or lighting write pass through a
single queue per device. Any other program that regularly drives such a device has to go through
CoolerControl, or it competes with sensor polling.

This sets what belongs in the project. CoolerControl owns the serialized device connection and the
hardware commands: the screen or lighting write itself, brightness, orientation. It does not own
presentation. Rendering, layouts, effects, dashboards and telemetry displays belong in separate
programs that use the API and hand CoolerControl a finished result, as coolerdash does today.

The LCD and lighting controls in the UI, and the daemon's built-in LCD renderers, predate this
boundary, from a time when nothing else on Linux drove these devices and the daemon was far simpler.
They are on a deprecation path: they will not gain new features or devices, and new integrations
should not build on them. The API will continue to support external programs that drive screens and
lighting, and may become more generic so that new devices are easier to support from outside the
daemon.

## coolercontrold Daemon

This is the main daemon that handles sensor data collection and device controls. It is meant to run
24/7 in the background with minimum energy and load requirements. It can be used on headless
servers, as well as desktop systems. systemd support is offered by the project, and other init
systems are handled by community contributions. Additionally, the daemon offers an API for GUI and
CLI control, and the GUI assets are embedded into the daemon allowing UI access from any browser.
This is a key feature as configuration file controls are not officially supported due to the
complexities involved with CC's feature set. See the
[coolercontrold README](coolercontrold/README.md) for more detailed information.

## coolercontrol-liqctld Daemon

This is a service daemon that interfaces directly with the
[liquidctl](https://github.com/liquidctl/liquidctl) Python API and uses IPC to communicate with the
main coolercontrold daemon. Its scope is limited to providing device access and detailed information
to the coolercontrold daemon. See the
[liqctld README](coolercontrold/daemon/resources/liqctld/README.md) for more information.

## GUI

The GUI is designed to enhance the user experience of controlling cooling devices on Linux without
the need to use the terminal or CLI commands. See the
[coolercontrol-ui README](coolercontrol-ui/README.md) for more information.
