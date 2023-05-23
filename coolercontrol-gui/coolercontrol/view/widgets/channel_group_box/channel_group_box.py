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

from PySide6.QtWidgets import QGroupBox, QSizePolicy


class ChannelGroupBox(QGroupBox):
    _style: str = '''
    QGroupBox {{
        color: {_color};
        background: {_bg_color};
        border-top: 2px solid qlineargradient(spread:pad, x1:0.75, y1:0, x2:1, y2:0, 
                                               stop:0 {_boarder_color}, stop:1 {_bg_color} );
        border-left: 2px solid qlineargradient(spread:reflect, x1:0, y1:0.15, x2:0, y2:1, 
                                               stop:0 {_boarder_color}, stop:0.6 {_bg_color} );
        padding: 10px;
        margin-top: 16px; 
    }}
    QGroupBox::title {{
        subcontrol-origin: margin;
        left: 2px;
        padding: 0px 4px 0px 4px;
    }}
    '''

    def __init__(self,
                 title: str = '',
                 radius: int = 14,
                 color: str = '#FFF',
                 bg_color: str = '#444',
                 title_bg_color: str = '#000',
                 boarder_color: str = '#FFF',
                 ) -> None:
        super().__init__()
        self.setStyleSheet(self._style.format(
            _radius=radius,
            _color=color,
            _bg_color=bg_color,
            _title_bg_color=title_bg_color,
            _boarder_color=boarder_color,
        ))
        self.setTitle(title)
        self.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Preferred)
