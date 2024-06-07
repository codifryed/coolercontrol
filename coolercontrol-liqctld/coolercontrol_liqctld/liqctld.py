#  CoolerControl - monitor and control your cooling and other devices
#  Copyright (c) 2023  Guy Boldon
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

import argparse
import importlib.metadata
import logging
import os
import platform
import textwrap
from typing import Optional

import setproctitle
from coolercontrol_liqctld.server import Server


def add_log_level() -> None:
    debug_lc_lvl: int = 15

    def log_for_level(self, message, *args, **kwargs) -> None:
        if self.isEnabledFor(debug_lc_lvl):
            self._log(debug_lc_lvl, message, args, **kwargs)

    def log_to_root(message, *args, **kwargs) -> None:
        logging.log(debug_lc_lvl, message, *args, **kwargs)

    logging.addLevelName(debug_lc_lvl, "DEBUG_LC")
    setattr(logging, "DEBUG_LC", debug_lc_lvl)
    setattr(logging, "debug_lc", log_to_root)
    setattr(logging.getLoggerClass(), "debug_lc", log_for_level)


add_log_level()
log = logging.getLogger(__name__)
__version__: str = "1.3.0"


def main() -> None:
    setproctitle.setproctitle("coolercontrol-liqctld")
    env_log_level: Optional[str] = os.getenv("COOLERCONTROL_LOG")
    parser = argparse.ArgumentParser(
        description="A CoolerControl daemon service for liquidctl",
        exit_on_error=False,
        formatter_class=argparse.RawTextHelpFormatter,
    )
    parser.add_argument(
        "-v", "--version", action="version", version=f"\n {system_info()}"
    )
    parser.add_argument("--debug", action="store_true", help="enable debug output \n")
    parser.add_argument(
        "--debug-liquidctl", action="store_true", help="enable liquidctl debug output\n"
    )
    parser.add_argument(
        "-d",
        "--daemon",
        action="store_true",
        help="Starts liqctld in Systemd daemon mode",
    )
    args = parser.parse_args()
    # default & "INFO" levels:
    log_level = logging.INFO
    liquidctl_level = logging.WARNING
    uvicorn_level = logging.WARNING
    if args.debug:
        log_level = logging.DEBUG
        liquidctl_level = logging.DEBUG
        uvicorn_level = logging.DEBUG
    elif args.debug_liquidctl:
        log_level = logging.DEBUG_LC
        liquidctl_level = logging.DEBUG
        uvicorn_level = logging.WARNING
    elif env_log_level:
        if env_log_level.lower() == "debug":
            log_level = logging.DEBUG
            liquidctl_level = logging.DEBUG
            uvicorn_level = logging.DEBUG
        elif env_log_level.lower() == "debug_liquidctl":
            log_level = logging.DEBUG_LC
            liquidctl_level = logging.DEBUG
            uvicorn_level = logging.WARNING
        elif env_log_level.lower() == "warn":
            log_level = logging.WARNING
            liquidctl_level = logging.WARNING
            uvicorn_level = logging.WARNING
        elif env_log_level.lower() == "error":
            log_level = logging.ERROR
            liquidctl_level = logging.ERROR
            uvicorn_level = logging.ERROR

    is_systemd: bool = args.daemon and os.geteuid() == 0
    if is_systemd:
        log_format = "%(levelname)-8s %(name)s - %(message)s"
    else:
        log_format = "%(asctime)-15s %(levelname)-8s %(name)s - %(message)s"

    handler = logging.StreamHandler()
    handler.setFormatter(logging.Formatter(log_format))
    root_logger = logging.getLogger("root")
    root_logger.setLevel(log_level)
    root_logger.addHandler(handler)
    liquidctl_logger = logging.getLogger("liquidctl")
    liquidctl_logger.setLevel(liquidctl_level)

    log.info("Liquidctl daemon initializing")
    if log.isEnabledFor(logging.DEBUG):
        log.debug("DEBUG level enabled\n%s", system_info())
    elif log.isEnabledFor(logging.DEBUG_LC):
        log.debug_lc("Liquidctl DEBUG_LC level enabled\n%s", system_info())
    if os.geteuid() != 0:
        log.warning(
            "coolercontrol-liqctld should be run with root privileges in most cases"
        )

    server = Server(__version__, is_systemd, uvicorn_level)
    server.startup()


def system_info() -> str:
    sys_info: str = textwrap.dedent(
        f"""
            CoolerControl-Liqctld v{__version__}

            System:"""
    )
    if platform.system() == "Linux":
        sys_info += f'\n    {platform.freedesktop_os_release().get("PRETTY_NAME")}'  # type: ignore
    sys_info += textwrap.dedent(
        f"""
                {platform.platform()}

            Dependency versions:
                Python     {platform.python_version()}
                Liquidctl  {_get_package_version("liquidctl")}
            """
    )
    return sys_info


def _get_package_version(package_name: str) -> str:
    """This searches for package versions.
    First it checks the metadata, which is present for all packages.
    If the metadata isn't found, like with the compiled AppImage, it checks inside the package for __version__.
    If package doesn't exist then it either defaults to the last known version or "unknown"
    """
    try:
        return importlib.metadata.version(package_name)
    except importlib.metadata.PackageNotFoundError:
        if package_name == "liquidctl":
            import liquidctl

            return _get_version_attribute(liquidctl)
        else:
            return "unknown"


def _get_version_attribute(package_object: object) -> str:
    return getattr(package_object, "__version__", "unknown")


if __name__ == "__main__":
    main()
