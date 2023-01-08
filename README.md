# CoolerControl

Monitor and control your cooling devices.

This is a re-implementation of [Coolero](https://gitlab.com/coolero/coolero) with a focus on system level integration
with a control GUI written in both Rust and Python.

Some goals/features over Coolero:

- control settings applied on system start
- lower resource usage
- decoupled GUI
- remote control
- can run on a headless server
- full systemd integration
- improved multi-device communication efficiency
- human readable & alterable settings
- system level packaging (deb, rpm, etc)

This is a work in progress, as the main software "engine" of Coolero is re-envisioned and re-implemented. No major
changes to the UI are planned.

