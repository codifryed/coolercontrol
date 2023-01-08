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

SPEED_CONTROL_STYLE = '''
QGroupBox {{
    color: {_color};
    font-size: 14pt;
    border: 1px solid {_border_color};
    border-radius: {_radius}px;
    margin-top: 14px;
}}

QComboBox, QListView {{
    /*
    selection-color: {_active_color};
    border-radius: {_radius}px;
    border-style: solid;
    border: 1px solid {_border_color};
    selection-background-color: {_active_color};
    margin-top: 0px;
    */
    color: {_color};
    background-color: {_bg_color};
    selection-background-color: {_selection_bg_color};
    selection-color: white;
    border-radius: 0px;
}}
QComboBox:editable {{
    /*
    background-color: {_active_color};
    padding-top: 3px;
    padding-left: 4px;
    */
}}
QComboBox:!editable, QComboBox::drop-down:editable {{
    /*
    background-color: {_bg_color};
    selection-background-color: {_bg_color};
    */
    padding-top: 3px;
    padding-bottom: 3px;
    padding-left: 7px;
}}
QComboBox:on {{
    /*
    background-color: {_bg_color};
    padding-top: 3px;
    padding-left: 4px;
    */
}}
QComboBox::drop-down {{
    /*
    subcontrol-origin: padding;
    border-radius: {_radius}px;
    background-color: {_bg_color};
    subcontrol-position: top right;
    width: 15px;

    border-left-width: 1px;
    border-left-color: darkgray;
    border-top-right-radius: 3px;
    border-bottom-right-radius: 3px;
    */
}}
QComboBox::down-arrow {{
    /*
    height: 20px;
    top: 1px;
    left: 2px;
    */
}}
QComboBox QAbstractItemView {{
    /*
    background-color: {_bg_color};
    border-radius: {_radius}px;
    */
}}
'''
