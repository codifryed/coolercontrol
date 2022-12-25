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

import logging
from pathlib import Path

from coolercontrol.models.base_daemon import BaseDaemon
from coolercontrol.settings import Settings

_LOG = logging.getLogger(__name__)


class HwmonDaemonClient(BaseDaemon):
    """
    This class is used to speak the Daemon running in the background
    """
    _client_version: str = '1'

    def __init__(self, is_session_daemon: bool) -> None:
        super().__init__()
        self.is_session_daemon: bool = is_session_daemon
        if self.is_session_daemon:
            self._socket_dir: str = str(Settings.tmp_path.joinpath(self._socket_name))
        else:
            self._socket_dir = str(Settings.system_run_path.joinpath(self._socket_name))
        self._socket.connect(self._socket_dir)
        self.greet_daemon()

    def greet_daemon(self) -> None:
        self.send_kwargs(version=self._client_version)
        if response := self.recv_dict().get('response'):
            if response == 'version supported':
                _LOG.info('Client version supported by daemon and greeting exchanged successfully')
            else:
                _LOG.error('Client version not supported by daemon: %s', response)
                self.close_connection()
                raise ValueError('Client version not supported by daemon')
        else:
            _LOG.error('No response in client supported greeting')

    def apply_setting(self, path: Path, value: str) -> bool:
        return self.log_exceptions(
            lambda: self._apply_setting(path, value),
            default_return=False
        )

    def _apply_setting(self, path: Path, value: str) -> bool:
        self.send_kwargs(path=str(path), value=value)
        if response := self.recv_dict().get('response'):
            if response == 'setting success':
                return True
        return False

    def close_connection(self) -> None:
        """This will close the connection to the daemon"""
        self.log_exceptions(self._close_connection)

    def _close_connection(self) -> None:
        self.send_kwargs(cmd='close connection')
        if response := self.recv_dict().get('response'):
            if response == 'bye':
                _LOG.info('Daemon connection closed')
                self._socket.close()
                return
        _LOG.warning('Error trying to close the Daemon connection')
        self._socket.close()

    def shutdown(self) -> None:
        """This will shut the daemon down"""
        self.log_exceptions(self._shutdown)

    def _shutdown(self) -> None:
        self.send_kwargs(cmd='shutdown')
        if response := self.recv_dict().get('response'):
            if response == 'bye':
                _LOG.info('Daemon shutdown')
                self._socket.close()
                return
        _LOG.warning('Error trying to shut the Daemon down')
        self._socket.close()
