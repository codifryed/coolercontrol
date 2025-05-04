# coolercontrol-ui

The UI is designed to enhance the user experience of controlling cooling devices on Linux, which
until recently had only been achievable using terminal commands and manually editing configuration
files. These are valid methods, but become increasingly more complex as one begins to use more
advanced features like fan curves and using sensor outputs from various sources. In addition, the UI
offers ways to monitor cooling related data, so that the user can see and adjust to the effects of
their changes in real time.

This folder contains the UI assets served by the daemon for both the Web UI and the Desktop
Application.

The UI is a JavaScript SPA using the Vue framework. It communicates with the `coolercontrold` daemon
using a REST API. Cosmetic-specific features are handled completely by the UI, whereas core logic
and processes are handled by the daemon.

## Requirements

- make
- nodejs >= 18.0.0
- npm

## Installation

Since these assets are embedded in the daemon binary, this folder itself doesn't install anything.
See `coolercontrold` for the daemon which contains the Web UI, and is also where the desktop
application retrieves the web assets.

## Development

Development can mostly be done using `npm`. Note that the Qt Desktop application uses an older
chromium browser engine on older distros. Such as Chrome v90 for Qt 6.2.4 on Ubuntu 22.04 LTS. This
means one needs to test functions and feature for compatibility with those older engines.

Install NPM dependencies & Build:

```bash
make build
```

Test:

```bash
make test
```

Hot-Reload in your browser:

```bash
npm run dev
# or
make dev
```

## Held-back Dependencies

- `"pinia": "2.2.4"` greater than this breaks some functionality in the UI with reactive
  text/numbers.
- `"primevue": "4.1.1"` breaks the original Primevue tailwind implementation that we have.
- `"tailwindcss-primeui": "^0.4.0"` breaks our original Primevue tailwind implementation
- `"vue-tsc": "2.2.4"` breaks some tests.
- `"@types/node": "^20.17.30"` for max compat with older distros
- `"tailwindcss": "^3.4.17",` the upgrade to 4.x looks to be significant work

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

This will auto-format all files. Afterward, commit any changes:

```bash
# cd to repository root directory
cd ..
make ci-fmt
```
