#  Coolero - monitor and control your cooling and other devices
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
from typing import List

from matplotlib.artist import Artist
from matplotlib.axes import Axes
from matplotlib.backend_bases import MouseEvent, KeyEvent
from matplotlib.figure import Figure
from matplotlib.patches import FancyBboxPatch
from matplotlib.text import Text
from matplotlib.transforms import IdentityTransform

from coolero.settings import Settings, UserSettings
from coolero.view.uis.canvases.canvas_context_menu import ItemProperties, CanvasContextMenu

_LOG = logging.getLogger(__name__)


class CanvasInputBox:
    _pad_y: int = 5
    _pad_x: int = 5

    def __init__(self, axes: Axes) -> None:
        self.axes: Axes = axes
        self.fig: Figure = axes.get_figure()
        self._active: bool = False
        self.selected_xdata: int = 0
        self.selected_ydata: int = 0
        self.active_point_index: int | None = None
        self.current_profile_temps: List[int] = []
        self.current_profile_duties: List[int] = []
        self.current_max_temp: int = 100
        self.current_min_temp: int = 0
        self.current_max_duty: int = 100
        self.current_min_duty: int = 0
        self.props = ItemProperties()
        left, y0 = 100, 400  # starting coords

        self.temp_edit_box: Text = axes.text(
            left, y0, '100°', transform=IdentityTransform(), animated=True, color=self.props.text_color_deactivated,
            ha='right', parse_math=False, zorder=50
        )
        self.capture_keystrokes_temp: bool = False

        self.duty_edit_box: Text = axes.text(
            left, y0, '100%', transform=IdentityTransform(), animated=True, color=self.props.text_color_deactivated,
            ha='right', parse_math=False, zorder=50
        )
        self.capture_keystrokes_duty: bool = False

        text_bbox = self.duty_edit_box.get_window_extent(self.fig.canvas.get_renderer())
        self._ui_scaling_factor: float = Settings.user.value(UserSettings.UI_SCALE_FACTOR, defaultValue=1.0, type=float)
        self._text_height: int = text_bbox.height * self._ui_scaling_factor
        self._text_width: int = text_bbox.width * self._ui_scaling_factor
        self.width = (self._text_width + self._pad_x) * 2
        self.height = self._text_height + self._pad_y

        props_bg_box = ItemProperties(alpha=0.9)
        self.bg_box: FancyBboxPatch = FancyBboxPatch(
            (left, y0), self.width, self.height, animated=True, transform=IdentityTransform(),
            boxstyle='round,pad=3.0', fc=props_bg_box.bg_color, ec=props_bg_box.text_color, alpha=props_bg_box.alpha,
            zorder=49
        )
        self.axes.add_patch(self.bg_box)

        self.items: List[Artist] = [self.bg_box, self.temp_edit_box, self.duty_edit_box]
        self.active = False

    @property
    def active(self) -> bool:
        return self._active

    @active.setter
    def active(self, is_active: bool) -> None:
        if is_active:
            if self.active_point_index is not None and self.active_point_index < len(self.current_profile_temps):
                initial_temp: str = f'{self.current_profile_temps[self.active_point_index]}°'
                self.temp_edit_box.set_text(initial_temp)
                initial_duty: str = f'{self.current_profile_duties[self.active_point_index]}%'
                self.duty_edit_box.set_text(initial_duty)
                if self.active_point_index == 0:
                    self.capture_keystrokes_duty = True  # first point must stay at 0 degrees, therefor we start here
                else:
                    self.capture_keystrokes_temp = True  # start with temp editing active most of the time
                self._indicate_active_edit()
        else:
            self.stop_editing()
        for item in self.items:
            item.set_visible(is_active)
        self._active = is_active

    def contains(self, event: MouseEvent) -> bool:
        is_within, _ = self.bg_box.contains(event, 0)
        return is_within

    def set_position(self, context_menu: CanvasContextMenu) -> None:
        self.selected_xdata = context_menu.selected_xdata
        self.selected_ydata = context_menu.selected_ydata
        x, y = self.axes.transData.transform((self.selected_xdata, self.selected_ydata))
        x_axis = self.axes.transAxes.inverted().transform((x, y))[0]
        x_offset = 5 if x_axis < 0.8 else -self.width - 5
        y_offset = -5 if self.selected_ydata > 22 else self.height + 5
        x_pos = x + x_offset
        y_pos = y + y_offset - self.height
        self.bg_box.set_x(x_pos)
        self.bg_box.set_y(y_pos)
        self.temp_edit_box.set_position((
            x_pos + ((self.width / 2) - (self._pad_x * self._ui_scaling_factor)),
            y_pos + (self._pad_y * self._ui_scaling_factor)
        ))
        self.duty_edit_box.set_position((
            x_pos + (self.width - (self._pad_x * self._ui_scaling_factor)),
            y_pos + (self._pad_y * self._ui_scaling_factor)
        ))

    def click(self, event: MouseEvent) -> bool:
        if event.inaxes is None or not self.bg_box.contains(event)[0]:
            self.stop_editing()
            return False
        self._click_select_edit_box(event)
        return True

    def scroll(self, event: MouseEvent) -> None:
        if self.capture_keystrokes_temp:
            self._handle_text_editing(event.button, self.temp_edit_box, self.current_min_temp, self.current_max_temp)
        elif self.capture_keystrokes_duty:
            self._handle_text_editing(event.button, self.duty_edit_box, self.current_min_duty, self.current_max_duty)

    def keypress(self, event: KeyEvent) -> bool:
        key: str = event.key
        if key in {'enter', 'return'}:
            self.stop_editing()
            return self.apply_changes()
        elif key == 'tab' and self.active_point_index != 0:
            switch_active: bool = not self.capture_keystrokes_temp
            self.capture_keystrokes_temp = switch_active
            self.capture_keystrokes_duty = not switch_active
            self._indicate_active_edit()
            return False
        elif is_right := key == 'right' and self.active_point_index != 0:
            self.capture_keystrokes_temp = not is_right
            self.capture_keystrokes_duty = is_right
            self._indicate_active_edit()
            return False
        elif is_left := key == 'left' and self.active_point_index != 0:
            self.capture_keystrokes_temp = is_left
            self.capture_keystrokes_duty = not is_left
            self._indicate_active_edit()
            return False
        elif key == 'escape':
            self.stop_editing()
            self.active = False
            return False
        if self.capture_keystrokes_temp:
            self._handle_text_editing(key, self.temp_edit_box, self.current_min_temp, self.current_max_temp)
        elif self.capture_keystrokes_duty:
            self._handle_text_editing(key, self.duty_edit_box, self.current_min_duty, self.current_max_duty)

    @staticmethod
    def _handle_text_editing(key: str, text_box: Text, min_val: int, max_val: int) -> None:
        symbol: str = text_box.get_text()[-1]
        text: str = text_box.get_text()[:-1]  # without the symbol
        if len(key) == 1 and key.isdigit():
            if not text or int(text + key) <= max_val:
                text += key
            else:
                text = key  # reset text once over max
        elif key == 'up' and int(text) < max_val:
            text = str(int(text) + 1)
        elif key == 'down' and int(text) > min_val:
            text = str(int(text) - 1)
        elif key == 'backspace' and text != '':
            text = text[:-1]
        elif key == 'delete':
            text = ''
        else:
            return
        text += symbol
        text_box.set_text(text)

    def _click_select_edit_box(self, event: MouseEvent) -> None:
        if clicked_in_temp_box := self.temp_edit_box.contains(event)[0] and self.active_point_index != 0:
            self.capture_keystrokes_temp = clicked_in_temp_box
            self.capture_keystrokes_duty = not clicked_in_temp_box
        elif clicked_in_duty_box := self.duty_edit_box.contains(event)[0]:
            self.capture_keystrokes_temp = not clicked_in_duty_box
            self.capture_keystrokes_duty = clicked_in_duty_box
        else:
            return
        self._indicate_active_edit()

    def stop_editing(self) -> None:
        self.capture_keystrokes_temp = False
        self.capture_keystrokes_duty = False
        self._indicate_active_edit()

    def apply_changes(self) -> bool:
        temp_text: str = self.temp_edit_box.get_text()[:-1]  # remove symbol
        duty_text: str = self.duty_edit_box.get_text()[:-1]
        if not temp_text or not duty_text:
            return False
        captured_temp: int = int(temp_text)
        captured_duty: int = int(duty_text)
        current_temp: int = self.current_profile_temps[self.active_point_index]
        current_duty: int = self.current_profile_duties[self.active_point_index]

        self.active = False
        if current_temp != captured_temp or current_duty != captured_duty:
            self.current_profile_temps[self.active_point_index] = captured_temp
            self.current_profile_duties[self.active_point_index] = captured_duty
            return True
        return False

    def _indicate_active_edit(self) -> None:
        self.temp_edit_box.set_color(
            self.props.text_color
            if self.capture_keystrokes_temp else self.props.text_color_deactivated
        )
        self.duty_edit_box.set_color(
            self.props.text_color
            if self.capture_keystrokes_duty else self.props.text_color_deactivated
        )
