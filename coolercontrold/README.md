# coolercontrold

is the main daemon containing the core logic for interfacing with devices, and is installed as
"coolercontrold". It is meant to run in the background as a system daemon. It handles all device
communication and data management and processing, additionally connecting to the
`coolercontrol-liqctld` daemon for liquidctl supported devices.

It has an REST API that services client programs, such as the Desktop Application and Web UI.
Additionally the Web UI is embedded in the daemon for access over a local browser without the need
for any additional package.

Note that the API is not meant to be exposed over a publicly available network interface. Doing so
is not only a security risk and will expose control of your hardware to attacks, but the server is
not meant to be used like a standard http server. It will prioritize handling devices over serving
http requests and is designed for minimal resource usage. There are some security measures
implemented, such as session cookie authentication required to be able to change settings, but is in
itself incomplete without additional measures. Additional measures, for example, could include an
HTTPS proxy, but largely depends on your network setup and specific requirements.

The OpenAPI specification for the daemon's REST API can be found either in:

- [GitLab Repo](https://gitlab.com/coolercontrol/coolercontrol/-/blob/main/openapi/openapi.json?ref_type=heads)
- [CoolerControl Website](https://coolercontrol.org/openapi/)

## Design

Built using asynchronous Rust for efficiency and flexibility. It uses a structured concurrency model
instead of the typical shared-worker model. This means the application is single threaded (!Send +
!Sync) with the exception of blocking file IO, and image processing and rendering for LCD screens.
Those processes use a dedicated thread pool for that work. Tokio is currently used as the runtime
due to its excellent integration with other libraries.

Loop and processing timings have been synchronized to maximize process sleep time and efficiency,
while still delivering a consistent data stream and cooling device control.

### Why Asynchronous?

Using asynchronous concurrency enables this application to handle often-unforeseeable device
communication delays and poll device statuses independently, without the additional overhead and
complexity needed for a multi-threaded shared-worker runtime. Since most of the load of the
application is sysfs IO, the single-threaded asynchronous concurrency model fits this well. As a
Linux only application, io_uring is a possible alternative to the standard epoll methods being used
currently, and is something we might take advantage of in the future.

## Requirements

- make
- cargo/rust >= 1.81.0
- libdrm-dev
- To build the web assets:
  - nodejs >= 18.0.0
  - npm

## Installation

To include the web assets, make sure to build the assets first:

```bash
cd ../coolercontrol-ui/ && make build
make build
sudo make install
```

**Alternatively:**  
One can use the dev-build and dev-install steps below.

## Development

When developing the daemon, one has to also consider that it embeds the web assets into the binary.
To this end, the best way to develop the application is to install one of the distribution packages,
then:

Clean & Build the UI assets, daemon and desktop release binaries:

```bash
cd .. && make dev-build
```

Install the build daemon and desktop binaries:

```bash
cd .. && make dev-install
```

Run all tests for the UI assets, daemon, and desktop application:

```bash
cd .. && make dev-test
```

**Alternatively:**  
As a Rust application, one can use the standard methods for building and testing:  
_Note: this does not handle the web assets and daemon performance in debug mode is very different
compared to release mode_

```bash
cargo build
cargo test
cargo build --locked --release
```

## Formatting

CoolerControl uses [Trunk.io](https://github.com/trunk-io) to format all files for the entire
repository. The first time you run this, it may take a while as it downloads all the tools and
formatters needed for the project.

This will check if there are formatting or linting issues:

```bash
# cd to repository root directory
cd ..
make ci-check
```

This will auto-format all files. Afterwards, commit any changes:

```bash
# cd to repository root directory
cd ..
make ci-fmt
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
