# coolercontrol-liqctld

is a CoolerControl service daemon written in Python for interacting with `liquidctl` devices on a
system level, and is installed as the `coolercontrol-liqctld` application. It wraps the underlying
`liquidctl` library providing an API that the main `coolercontrol` daemon interacts with. It also
enables parallel device communication and access to specific device properties.

## Requirements

- make
- python >= 3.8
- pip

## Installation

**Option 1:**  
The easiest is to install this using one of the distro packages.

**Option 2:**  
As part of the main Make goal

```bash
cd .. && sudo make install-source
```

**Option 3:**  
Manually.  
(sudo is needed to install the dependencies on a system-level)  
_Note: Not all distros allow manually installing system-level dependencies using pip_

```bash
sudo make clean
sudo make install-pip-dependencies
sudo make install-source
```

Then install the systemd service file manually:

```bash
cd .. && sudo install -Dm644 packaging/systemd/coolercontrol-liqctld.service -t $(DESTDIR)/etc/systemd/system/
```

## Development

In general, this package receives few updates and doesn't need to be including in the normal
development cycle. The easiest way to develop is to install the package for your particular
distribution and then copy or install the local development files into the python system package
area, overwriting the package files.

```bash
# Install CoolerControl package like normal
sudo make clean
sudo make build-source
```

```bash
# Note: some distributions like Fedora use a non-standard location for package-installed Python
# system dependencies and require you to manually set the current location. Arch will not let you
# install system-level packages with pip at all and you'll have to overwrite the installed files
# manually.
sudo pip install . --upgrade --target /usr/lib/python3.13/site-packages/

# Otherwise this will do:
sudo make install-source
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
