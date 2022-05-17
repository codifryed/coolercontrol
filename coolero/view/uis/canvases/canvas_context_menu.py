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
from typing import List, Callable, Optional

from matplotlib.artist import Artist
from matplotlib.axes import Axes
from matplotlib.backend_bases import MouseEvent, MouseButton
from matplotlib.figure import Figure
from matplotlib.patches import Rectangle, FancyBboxPatch
from matplotlib.text import Text
from matplotlib.transforms import IdentityTransform

from coolero.settings import Settings

_LOG = logging.getLogger(__name__)
_SPACER_SIZE: int = 5


class ItemProperties:
    def __init__(self,
                 fontsize=10,
                 text_color=Settings.theme['app_color']['text_foreground'],
                 text_color_deactivated=Settings.theme['app_color']['text_description'],
                 bg_color=Settings.theme['app_color']['bg_two'],
                 alpha=0.0):
        self.fontsize = fontsize
        self.text_color = text_color
        self.text_color_deactivated = text_color_deactivated
        self.bg_color = bg_color
        self.alpha = alpha


class MenuItem(Artist):
    pad_x: int = 5
    pad_y: int = 5

    def __init__(self,
                 axes: Axes,
                 text: str,
                 props: ItemProperties | None = None,
                 hover_props: ItemProperties | None = None,
                 callback: Optional[Callable] = None
                 ) -> None:
        super().__init__()
        self.axes = axes
        self.fig: Figure = axes.get_figure()
        self.set_figure(self.fig)
        self.set_zorder(50)  # always display the menu items above other artists

        self.text: str = text
        self.props = props if props is not None else ItemProperties()
        self.hover_props = hover_props if hover_props is not None else ItemProperties(
            text_color=Settings.theme['app_color']['text_active']
        )
        self.connect: Callable = callback if callback is not None else self.default_connect_func
        self.label: Text = axes.text(
            0, 0, text, transform=IdentityTransform(), size=self.props.fontsize, animated=True
        )
        self.label.set_animated(True)
        self.text_bbox = self.label.get_window_extent(self.fig.canvas.get_renderer())
        self.rect: Rectangle = Rectangle(
            (0, 0), 1, 1, animated=True  # Will be updated later.
        )
        self._hover: bool = False
        self.set_hover_props(self.hover)
        self.set_animated(True)
        self.set_visible(False)
        self._active: bool = True
        self._cid: int = self.fig.canvas.mpl_connect('button_release_event', self.check_select)

    @property
    def active(self) -> bool:
        return self._active

    @active.setter
    def active(self, is_active: bool) -> None:
        if is_active:
            if not self._active:
                self.label.set_color(self.props.text_color)
                self._cid = self.fig.canvas.mpl_connect('button_release_event', self.check_select)
        else:
            self.label.set_color(self.props.text_color_deactivated)
            self.fig.canvas.mpl_disconnect(self._cid)
        self._active = is_active

    @property
    def hover(self) -> bool:
        return self._hover

    @hover.setter
    def hover(self, is_hovering: bool) -> None:
        self.set_hover_props(is_hovering)
        self._hover = is_hovering

    @staticmethod
    def default_connect_func(self, event: MouseEvent) -> None:
        _LOG.warning('default menu item function called')

    def is_within(self, event: MouseEvent) -> bool:
        return self.rect.contains(event)[0]

    def check_select(self, event: MouseEvent) -> None:
        button_released_in_item = self.is_within(event)
        if not button_released_in_item:
            return
        if self.connect is not None:
            self.connect()

    def set_extent(self, x, y, w, h, depth):
        self.rect.set(x=x, y=y, width=w, height=h)
        self.label.set(position=(x + self.pad_x, y + depth + self.pad_y / 2))

    def draw(self, renderer):
        self.rect.draw(renderer)
        self.label.draw(renderer)

    def set_hover_props(self, is_hovered_over):
        props = self.hover_props if is_hovered_over else self.props
        self.label.set(color=props.text_color)
        self.rect.set(facecolor=props.bg_color, alpha=props.alpha)

    def check_hover(self, event: MouseEvent) -> bool:
        """ Update the hover status of event and return whether it was changed. """
        is_hovering = self.is_within(event)
        changed = ((is_hovering and self._active) != self._hover)
        if changed:
            self.hover = is_hovering
        return changed

    def set_visible(self, visible: bool) -> None:
        self.label.set_visible(visible)
        self.rect.set_visible(visible)
        super().set_visible(visible)

    def set_position(self, x: int, y: int, depth: int) -> None:
        self.rect.set_x(x)
        self.rect.set_y(y)
        self.label.set_position(((x + self.pad_x), (y + depth + self.pad_y / 2)))


class CanvasContextMenu:

    def __init__(self,
                 axes: Axes,
                 callback_add_point: Optional[Callable] = None,
                 callback_remove_point: Optional[Callable] = None,
                 callback_reset_points: Optional[Callable] = None,
                 ) -> None:
        self.selected_xdata: int = 0
        self.selected_ydata: int = 0
        self._active: bool = False
        self.on_line: bool = False
        self.active_point_index: int | None = None
        self.current_profile_temps: List[int] = []
        self.axes: Axes = axes
        self.maximum_points_set: bool = True
        self.minimum_points_set: bool = False
        self.item_add_point = MenuItem(axes, 'add', callback=callback_add_point)
        self.item_remove_point = MenuItem(axes, 'remove', callback=callback_remove_point)
        space_props = ItemProperties(fontsize=0)
        self.item_spacer = MenuItem(axes, '', props=space_props, hover_props=space_props)
        self.item_reset_points = MenuItem(axes, 'reset', callback=callback_reset_points)
        self.menu_items: List[MenuItem] = [
            self.item_add_point, self.item_remove_point, self.item_spacer, self.item_reset_points
        ]
        max_height: int = max(item.text_bbox.height for item in self.menu_items)
        self.depth: int = max(-item.text_bbox.y0 for item in self.menu_items)
        max_width: int = max(item.text_bbox.width for item in self.menu_items)

        self.item_width: int = max_width + 2 * MenuItem.pad_x
        self.item_height: int = max_height + MenuItem.pad_y

        left, y0 = 100, 400  # starting coords
        self.total_menu_items_height: int = (self.item_height * (len(self.menu_items) - 1)) + _SPACER_SIZE
        for item in self.menu_items:
            item_height = self.item_height if item.text != '' else _SPACER_SIZE
            bottom = y0 - item_height
            item.set_extent(left, bottom, self.item_width, item_height, self.depth)
            y0 -= item_height
        props = ItemProperties(alpha=0.9)
        self.bg_box: FancyBboxPatch = FancyBboxPatch(
            (left, y0), self.item_width, self.total_menu_items_height, animated=True, transform=IdentityTransform(),
            boxstyle='round,pad=3.0', fc=props.bg_color, ec=props.text_color, alpha=props.alpha, zorder=49
        )
        self.axes.add_patch(self.bg_box)
        self.active = False  # visibility needed to render the correct size

    @property
    def active(self) -> bool:
        return self._active

    @active.setter
    def active(self, is_active: bool) -> None:
        if is_active:
            self.item_add_point.active = (
                    self.on_line
                    and self.active_point_index is None
                    and not self.maximum_points_set
                    and self.selected_xdata not in self.current_profile_temps
            )
            self.item_remove_point.active = (
                    self.active_point_index is not None
                    and not self.minimum_points_set
                    and self.active_point_index != 0
                    and self.active_point_index != len(self.current_profile_temps) - 1
            )
        else:
            for item in self.menu_items:
                item.set_hover_props(False)
        self.bg_box.set_visible(is_active)
        for item in self.menu_items:
            item.set_visible(is_active)
        self._active = is_active

    def contains(self, event: MouseEvent) -> bool:
        return any(item.is_within(event) for item in self.menu_items)

    def set_position(self, event: MouseEvent) -> None:
        if event.inaxes is None:
            return
        self.selected_xdata, self.selected_ydata = round(event.xdata), round(event.ydata)
        # offset based on position in graph, so that it's always completely displayed
        x_axis = self.axes.transAxes.inverted().transform((event.x, event.y))[0]
        y_offset = -5 if event.ydata > 22 else self.total_menu_items_height + 5
        x_offset = 5 if x_axis < 0.8 else -self.item_width - 5
        left = event.x + x_offset
        y0 = event.y + y_offset
        self.bg_box.set_x(left)
        for item in self.menu_items:
            item_height = self.item_height if item.text != '' else _SPACER_SIZE
            bottom = y0 - item_height
            item.set_position(left, bottom, self.depth)
            y0 -= item_height
        self.bg_box.set_y(y0)
