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

SCROLL_AREA_STYLE = '''
/* ScrollBars */
QScrollArea {{
    border: none;
    background: {_bg_color};
}}
QScrollBar:horizontal {{
    border: none;
    background: {_scroll_bar_bg_color};
    height: 12px;
    margin: 0px 0px 0 0px;
    border-radius: 6px;
}}
QScrollBar::handle:horizontal {{
    background: {_context_color};
    min-width: 25px;
    border-radius: 6px
}}
QScrollBar::add-line:horizontal {{
    border: none;
    background: {_scroll_bar_btn_color};
    width: 0px;
    border-top-right-radius: 6px;
    border-bottom-right-radius: 6px;
    subcontrol-position: right;
    subcontrol-origin: margin;
}}
QScrollBar::sub-line:horizontal {{
    border: none;
    background: {_scroll_bar_btn_color};
    width: 0px;
    border-top-left-radius: 6px;
    border-bottom-left-radius: 6px;
    subcontrol-position: left;
    subcontrol-origin: margin;
}}
QScrollBar::up-arrow:horizontal, QScrollBar::down-arrow:horizontal
{{
    background: none;
}}
QScrollBar::add-page:horizontal, QScrollBar::sub-page:horizontal
{{
    background: none;
}}
QScrollBar:vertical {{
    border: none;
    background: {_scroll_bar_bg_color};
    width: 12px;
    margin: 0px 0 0px 0;
    border-radius: 6px;
}}
QScrollBar::handle:vertical {{	
    background: {_context_color};
    min-height: 25px;
    border-radius: 6px
}}
QScrollBar::add-line:vertical {{
    border: none;
    background: {_scroll_bar_btn_color};
    height: 0px;
    border-bottom-left-radius: 6px;
    border-bottom-right-radius: 6px;
    subcontrol-position: bottom;
    subcontrol-origin: margin;
}}
QScrollBar::sub-line:vertical {{
    border: none;
    background: {_scroll_bar_btn_color};
    height: 0px;
    border-top-left-radius: 6px;
    border-top-right-radius: 6px;
    subcontrol-position: top;
    subcontrol-origin: margin;
}}
QScrollBar::up-arrow:vertical, QScrollBar::down-arrow:vertical {{
    background: none;
}}
QScrollBar::add-page:vertical, QScrollBar::sub-page:vertical {{
    background: none;
}}
'''
