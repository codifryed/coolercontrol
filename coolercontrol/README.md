# Qt Desktop Application

The desktop application is written in C++ and uses the [Qt6](https://www.qt.io/product/qt6)
framework to create a native desktop application which renders the UI assets using QtWebEngine,
which is based on the [chromium](https://www.chromium.org/) browser engine.

## Package Requirements RPM

### Runtime RPM

- qt6-qtbase
- qt6-qtwebengine
- qt6-qtwebchannel

### Development RPM

- make automake gcc gcc-c++
- cmake
- qt6-qtbase-devel
- qt6-qtwebengine-devel
- qt6-qtwebchannel-devel

## Package Requirements DEB

### Runtime DEB

- qt6-base-dev (not 100% accurate - many smaller non-dev deps)
- libqt6webenginewidgets6
- libqt6webenginecore6-bin
- libxcb-cursor0 (for X11)

### Development DEB

- build-essential
- cmake
- qt6-base-dev
- qt6-webengine-dev
- qt6-webengine-dev-tools

## Installation

```bash
make build
sudo make install
```

**Alternatively:**  
One can use the dev-build and dev-install steps below.

## Development

Standard debugger is helpful for C++ development. Also, it's quite common to use an npm dev server
when testing Web & Qt changes. To use that properly, one needs to comment out the
`// url.setPort(DEFAULT_DAEMON_PORT);` line on line 226 of `main_window.cpp`. (subject to change in
the future)

Also note, that compilation is relatively quick, so testing with the release build is ok for most
things.

```bash
make
./build/coolercontrol
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
