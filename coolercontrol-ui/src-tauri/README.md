# Tauri Desktop Application

The desktop application is written in Rust and uses the [Tauri](https://v2.tauri.app/) framework to
create a native GTK application which renders the UI assets using the system's
[WebKit](https://webkit.org/) browser engine.

## Requirements:

- make
- cargo/rust >= 1.81.0
- nodejs >= 18.0.0
- npm
- [Tauri development packages](https://v2.tauri.app/start/prerequisites/#linux).

## Installation

One needs to include the web assets, so make sure to build the assets first:

```bash
cd ../coolercontrol-ui/ && make build
make build
sudo make install
```

**Alternatively:**  
One can use the dev-build and dev-install steps below.

## Development

Tauri also included the ability to hot-reload the UI assets and Rust backend during development:

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

**Alternatively:**  
One can call rustfmt manually:

```bash
cargo fmt
```

We also use clippy pedantic rules for improved linting, but don't strongly enforce it:

```bash
cargo clippy -- -W clippy::pedantic -W clippy::cargo
```
