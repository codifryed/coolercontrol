#  CoolerControl - monitor and control your cooling and other devices
#  Copyright (c) 2022  Guy Boldon
#  |
#  This program is free software: you can redistribute it and/or modify
#  it under the terms of the GNU General Public License as published by
#  the Free Software Foundation, either version 3 of the License, or
#  (at your option) any later version.
#  |
#  This program is distributed in the hope that it will be useful,
#  but WITHOUT ANY WARRANTY; without even the implied warranty of
#  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
#  GNU General Public License for more details.
#  |
#  You should have received a copy of the GNU General Public License
#  along with this program.  If not, see <https://www.gnu.org/licenses/>.
# ----------------------------------------------------------------------------------------------------------------------

import fcntl
import logging
import os
import sys

from coolercontrol.settings import Settings

_LOG = logging.getLogger(__name__)


class ApplicationInstance:
    """
    Class that can be instantiated only once per machine.
    Works by creating a lock file.
    Based on the Tendo library implementation. Updated for our use case and current python version.
    """

    def __init__(self) -> None:
        self.initialized: bool = False
        self.lockfile: str = str(Settings.tmp_path.joinpath('coolercontrol.lock'))
        _LOG.debug('CoolerControl application instance lockfile: %s', self.lockfile)

        self.fp = open(self.lockfile, 'w')
        self.fp.flush()
        try:
            fcntl.lockf(self.fp, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except IOError:
            _LOG.critical('There appears to already be an instance of CoolerControl running. Exiting.')
            sys.exit(2)
        self.initialized = True

    def __del__(self) -> None:
        if not self.initialized:
            return
        try:
            fcntl.lockf(self.fp, fcntl.LOCK_UN)
            # os.close(self.fp)
            if os.path.isfile(self.lockfile):
                os.unlink(self.lockfile)
        except Exception as exc:
            if _LOG:
                _LOG.warning(exc)
            else:
                print(f'Unloggable error: {exc}')
            sys.exit(-1)
