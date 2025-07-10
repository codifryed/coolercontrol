#! /usr/bin/env python3

#  CoolerControl - monitor and control your cooling and other devices
#  Copyright (c) 2021-2025  Guy Boldon, Eren Simsek and contributors
#
#  This program is free software: you can redistribute it and/or modify
#  it under the terms of the GNU General Public License as published by
#  the Free Software Foundation, either version 3 of the License, or
#  (at your option) any later version.
#
#  This program is distributed in the hope that it will be useful,
#  but WITHOUT ANY WARRANTY; without even the implied warranty of
#  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
#  GNU General Public License for more details.
#
#  You should have received a copy of the GNU General Public License
#  along with this program.  If not, see <https://www.gnu.org/licenses/>.

import importlib.metadata
import importlib.util
import logging as log
import sys


def get_liquidctl_version() -> str:
    """
    Return the liquidctl version.
    This should be called after checking for the liquidctl package.
    """
    try:
        return importlib.metadata.version("liquidctl")
    except importlib.metadata.PackageNotFoundError:
        import liquidctl

        return getattr(liquidctl, "__version__", "unknown")


def main():
    """
    This script verifies that the necessary Python dependencies are installed.
    """
    root_logger = log.getLogger("root")
    root_logger.setLevel(log.INFO)
    log.info(f"Python Version detected: {sys.version}")
    importlib.util.find_spec("liquidctl")

    liquidctl_version = get_liquidctl_version()
    log.info(f"liquidctl version detected: {liquidctl_version}")


if __name__ == "__main__":
    main()
