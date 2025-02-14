# Qt Desktop Application

The desktop application is written in C++ and uses the [Qt6](https://www.qt.io/product/qt6)
framework to create a native desktop application which renders the UI assets using QtWebEngine,
which is based on the [chromium](https://www.chromium.org/) browser engine.

## Runtime Requirements

_todo: verify these are all needed:_

- qt6-qtbase
- qt6-qtwebchannel
- qt6-qtwebengine
- qt6-qtwebsockets
- qt6-qtwebview

### Debian:
- libqt6webenginewidgets6
- qt6-base-dev
- libqt6webenginecore6-bin
- libxcb-cursor0 (for X11)

## Development Requirements

- make
- cmake
- qt6-qtbase-devel
- qt6-qtwebchannel-devel
- qt6-qtwebengine-devel
- qt6-qtwebsockets-devel
- qt6-qtwebview-devel

### Debian:
- build-essential
- cmake
- qt6-base-dev (already taken care of below actually)
- qt6-webengine-dev
- qt6-webengine-dev-tools

## Installation

_todo: not yes sure if we're going to use the web assets and need to create makefiles:_

```bash
make build
sudo make install
```

**Alternatively:**  
One can use the dev-build and dev-install steps below.

## Development

**One can start an npm dev server and point the desktop app to that address, instead of the daemon
standard address**

Standard debugger is helpfull for C++ development. Tauri also included the ability to hot-reload the
UI assets and Rust backend during development:

```bash
cargo install tauri-cli --version "^2.0.0" --locked
cargo tauri dev
```

**Alternatively:**  
You can also build the daemon and desktop release binaries:

```bash
cd .. && make dev-build
```

Install the build daemon and desktop binaries to the system:

```bash
cd .. && make dev-install
```

Run all tests for the UI assets, daemon, and desktop application:

```bash
cd .. && make dev-test
```

## Formatting

CoolerControl uses [Trunk.io](https://github.com/trunk-io) to format all files for the entire
repository.

This will check if there are formatting issues:

```bash
cd .. && make ci-check
```

This will auto-format all files:

```bash
cd .. && make ci-fmt
```
