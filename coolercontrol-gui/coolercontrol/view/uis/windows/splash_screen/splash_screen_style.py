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

SPLASH_SCREEN_STYLE = '''
QFrame {{
    background-color: {_bg_color};
    color: {_color};
    border-radius: 14px;
}}

QProgressBar {{
    background-color: {_progress_bg_color};
    color: {_progress_color};
    border-style: none;
    border-radius: 10px;
    text-align: center;
}}
QProgressBar::chunk{{
    border-radius: 10px;
    background-color: qlineargradient(
        spread:pad, x1:0, y1:0.511364, x2:1, y2:0.523,
        stop:0 {_progress_from_color}, 
        stop:1 {_progress_to_color}
    );
}}

#label_title {{
    color: {_title_color};
}}

#label_description {{
    color: {_color};
}}

#label_loading {{
    color: {_color};
}}

#label_version {{
    color: {_color};
}}
'''
