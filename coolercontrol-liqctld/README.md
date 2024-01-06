# coolercontrol-liqctld

is a CoolerControl daemon for interacting with `liquidctl` devices on a system level, and is
installed as the `coolercontrol-liqctld` application. Its main purpose is to wrap the underlying
`liquidctl` library providing an API interface that the main `coolercontrol` daemon interacts with.
It also enables parallel device communication and access to specific device properties.
