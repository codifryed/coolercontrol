# liqctld

is a CoolerControl service daemon written in Python for interacting with `liquidctl` devices on a
system level, and is embedded in the `coolercontrold` daemon. It wraps the underlying `liquidctl`
library providing an API that the main daemon interacts with using IPC over a Unix domain socket.

Functionally, it enables parallel device communication and access to specific underlying device
properties that are otherwise not exposed. It tries to be as generic as possible, but specific
driver handling is often required due to differences in the underlying devices. Resiliency and
device delay handling are also taken into account.

## Testing

Changes to these files are embedded in the `coolercontrold` daemon at build time, and are largely
tested as part of that process.

## Requirements

Requirements for `liqctld` are checked before the service is started, and the daemon will start
without the required libraries if not found.

### Runtime

- A python3 interpreter >= 3.8
- `liquidctl`

### Build

- `python3-dev` is required for dynamic linking to the Python C API

## Installation

See the [coolercontrold](../../README.md) installation instructions.
