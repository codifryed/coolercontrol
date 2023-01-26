# liqctld

is a daemon for interacting with `liquidctl` on a system level, and is installed as the "coolercontrol-liqctld" program.
It is written in python and provides a simple API interface over liquidctl that the main coolercontrol daemon interacts with. Its
dependencies and a python interpreter are embedded in the executable, negating any system-level python dependencies.
