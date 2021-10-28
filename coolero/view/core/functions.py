#  Coolero - monitor and control your cooling and other devices
#  Copyright (c) 2021  Guy Boldon
#  All credit for basis of the user interface (GUI) goes to: Wanderson M.Pimenta
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

from settings import app_path


class Functions:

    @staticmethod
    def set_svg_icon(icon_name: str) -> str:
        return str(app_path.joinpath('resources/images/svg_icons').joinpath(icon_name))

    @staticmethod
    def set_svg_image(icon_name: str) -> str:
        return str(app_path.joinpath('resources/images/svg_images/').joinpath(icon_name))

    @staticmethod
    def set_image(image_name: str) -> str:
        return str(app_path.joinpath('resources/images/images/').joinpath(image_name))
