#  Coolero - monitor and control your cooling and other devices
#  Copyright (c) 2021  Guy Boldon
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
from typing import Optional, List, Iterator, Any

import numpy as np
import numpy.typing as npt
from PySide6 import QtCore
from matplotlib.animation import TimedAnimation, Animation
from matplotlib.artist import Artist
from matplotlib.backend_bases import MouseEvent
from matplotlib.backends.backend_qt5agg import FigureCanvasQTAgg
from matplotlib.figure import Figure
from matplotlib.lines import Line2D
from matplotlib.text import Annotation
from numpy.linalg import LinAlgError

from models.device_status import DeviceStatus
from models.speed_profile import SpeedProfile
from models.temp_source import TempSource
from view_models.device_observers import DeviceObserver, DeviceSubject

_LOG = logging.getLogger(__name__)

LABEL_CPU_TEMP: str = 'cpu temp'
LABEL_GPU_TEMP: str = 'gpu temp'
LABEL_DEVICE_TEMP: str = 'device temp'
LABEL_DEVICE_DUTY: str = 'device duty'
LABEL_PROFILE_FIXED: str = 'profile fixed'
LABEL_PROFILE_CUSTOM: str = 'profile custom'
DRAW_INTERVAL_MS: int = 250


class SpeedControlCanvas(FigureCanvasQTAgg, TimedAnimation, DeviceObserver):
    """Class to plot and animate Speed control and status"""

    def __init__(self,
                 device: DeviceStatus,
                 channel_name: str,
                 width: int = 16,
                 height: int = 9,
                 dpi: int = 120,
                 bg_color: str = '#000000',
                 text_color: str = '#ffffff',
                 cpu_color: str = 'red',
                 gpu_color: str = 'yellow',
                 liquid_temp_color: str = 'green',
                 device_line_color: str = 'blue',
                 starting_temp_source: str = '',
                 starting_speed_profile: str = ''
                 ) -> None:
        self._bg_color = bg_color
        self._text_color = text_color
        self._device = device
        self._channel_name = channel_name
        self._device_line_color = device_line_color
        self._cpu_color = cpu_color
        self._gpu_color = gpu_color
        self._liquid_temp_color = liquid_temp_color
        self._devices_statuses: List[DeviceStatus] = []
        self._chosen_temp_source: str = starting_temp_source
        self._chosen_speed_profile: str = starting_speed_profile
        self._drawn_artists: List[Artist] = []  # used by the matplotlib implementation for blit animation
        self.x_limit: int = 101  # the temp limit

        # Setup
        self.fig = Figure(figsize=(width, height), dpi=dpi, layout='constrained', facecolor=bg_color,
                          edgecolor=text_color)
        self.axes = self.fig.add_subplot(111, facecolor=bg_color)
        self.axes.set_ylim(0, 101)  # duty % range
        self.axes.set_xlim(20, self.x_limit)  # temp C range

        # Grid
        self.axes.grid(True, linestyle='dotted', color=text_color, alpha=0.5)
        self.axes.margins(x=0, y=0.05)
        self.axes.tick_params(colors=text_color)
        self.axes.set_xticks(
            [20, 30, 40, 50, 60, 70, 80, 90, 100],
            ['20°', '30°', '40°', '50°', '60°', '70°', '80°', '90°', '100°'])
        self.axes.set_yticks(
            [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100],
            ['0%', '10%', '20%', '30%', '40%', '50%', '60%', '70%', '80%', '90%', '100%'])
        self.axes.spines['top'].set_edgecolor(text_color + '00')
        self.axes.spines['right'].set_edgecolor(text_color + '00')
        self.axes.spines[['bottom', 'left']].set_edgecolor(text_color)

        # Lines
        self.lines: List[Line2D] = []
        self.duty_text: Annotation = Annotation('', (0, 0))

        # interactive
        self._profile_points_x: List[int] = []  # degrees
        self._profile_points_y: List[int] = []  # duty percent
        self._active_point_index: Optional[int] = None
        self._is_fixed_line_active: bool = False
        self._epsilon_threshold_pixels: int = 20
        self._epsilon_threshold_axis: int = 5
        self._button_press_cid: Optional[int] = self.fig.canvas.mpl_connect('button_press_event',
                                                                            self._mouse_button_press)
        self._button_release_cid: Optional[int] = self.fig.canvas.mpl_connect('button_release_event',
                                                                              self._mouse_button_release)
        self._mouse_motion_cid: Optional[int] = self.fig.canvas.mpl_connect('motion_notify_event', self._mouse_motion)

        # Initialize
        self._initialize_device_channel_duty_line()
        FigureCanvasQTAgg.__init__(self, self.fig)
        TimedAnimation.__init__(self, self.fig, interval=DRAW_INTERVAL_MS, blit=True)
        _LOG.debug('Initialized %s Speed Graph Canvas', device.device_name_short)

    @QtCore.Slot()
    def chosen_temp_source(self, temp_source: str) -> None:
        temp_source_btn = self.sender()
        channel_btn_id = temp_source_btn.objectName()
        self._chosen_temp_source = temp_source
        _LOG.debug('Temp source chosen:  %s from %s', temp_source, channel_btn_id)
        self._initialize_chosen_temp_source_lines()

    @QtCore.Slot()
    def chosen_speed_profile(self, profile: str) -> None:
        if profile:
            profile_btn = self.sender()
            channel_btn_id = profile_btn.objectName()
            _LOG.debug('Speed profile chosen:   %s from %s', profile, channel_btn_id)
            self._chosen_speed_profile = profile
            for line in list(self.lines):  # list copy as we're modifying in place
                if line.get_label() in [LABEL_PROFILE_FIXED, LABEL_PROFILE_CUSTOM]:
                    self.axes.lines.remove(line)
                    self.lines.remove(line)
            if profile == SpeedProfile.CUSTOM:
                self._initialize_custom_profile_markers()
            elif profile == SpeedProfile.FIXED:
                self._initialize_fixed_profile_line()

    def _draw_frame(self, framedata: int) -> None:
        """Is used to draw every frame of the chart animation"""

        if self._chosen_temp_source == TempSource.CPU:
            self._set_cpu_data()
        elif self._chosen_temp_source == TempSource.GPU:
            self._set_gpu_data()
        elif self.device.lc_device_id is not None:  # Liquid or other device temp
            self._set_device_temp_data()
        self._set_device_duty_data()

        self._drawn_artists = list(self.lines)  # pylint: disable=attribute-defined-outside-init
        self._drawn_artists.append(self.duty_text)
        for artist in self._drawn_artists:
            artist.set_animated(True)

    def new_frame_seq(self) -> Iterator[int]:
        return iter(range(self.x_limit))

    def draw(self) -> None:
        try:
            super().draw()
        except LinAlgError:
            # These error happens due to the collapse and expand animation of the device column, so far not a big deal
            _LOG.debug("expected LinAlgError draw error from speed control graph")

    def _step(self, *args: Any) -> None:
        # helpful to handle unexpected exceptions:
        try:
            TimedAnimation._step(self, *args)
        except BaseException as ex:
            TimedAnimation._stop(self)
            _LOG.exception('Error animating speed control graph: %s', ex)

    def notify(self, observable: DeviceSubject) -> None:
        if not self._devices_statuses:
            self._devices_statuses = observable.device_statuses

    def _initialize_device_channel_duty_line(self) -> None:
        channel_duty = 0.0
        channel_rpm = 0
        if self._channel_name == 'pump':
            if self._device.status.pump_duty:
                channel_duty = self._device.status.pump_duty
            if self._device.status.pump_rpm:
                channel_rpm = self._device.status.pump_rpm
        elif self._channel_name == 'fan':
            if self._device.status.fan_duty:
                channel_duty = self._device.status.fan_duty
            if self._device.status.fan_rpm:
                channel_rpm = self._device.status.fan_rpm
        if channel_duty:
            # todo: some devices do not set a duty and needs to be calculated manually....
            channel_duty_line = self.axes.axhline(
                channel_duty, xmax=100, color=self._device_line_color, label=LABEL_DEVICE_DUTY,
                linestyle='dotted', linewidth=1
            )
            channel_duty_line.set_animated(True)
            self.lines.append(channel_duty_line)
        if channel_rpm:
            text_y_position = self._calc_text_position(channel_duty)
            text_rpm = f'{channel_rpm} rpm'
            self.duty_text = self.axes.annotate(
                text=text_rpm, xy=(100, text_y_position), ha='right', size=10, color=self._device_line_color,
            )
            self.duty_text.set_animated(True)
        _LOG.debug('initialized channel duty line')

    def _initialize_chosen_temp_source_lines(self) -> None:
        for line in list(self.lines):  # list copy as we're modifying in place
            if line.get_label() in [LABEL_CPU_TEMP, LABEL_GPU_TEMP, LABEL_DEVICE_TEMP]:
                self.axes.lines.remove(line)
                self.lines.remove(line)
        if self._chosen_temp_source == TempSource.CPU:
            self._initialize_cpu_line()
        elif self._chosen_temp_source == TempSource.GPU:
            self._initialize_gpu_line()
        elif self._device.status.liquid_temperature is not None or self._device.status.device_temperature is not None:
            self._initialize_device_temp_line()
        # self._redraw_whole_canvas()  # might be needed for annotations in the future

    def _initialize_cpu_line(self) -> None:
        cpu_temp = 0
        cpu = self._get_first_device_with_name('cpu')
        if cpu and cpu.status.device_temperature:
            cpu_temp = cpu.status.device_temperature
        cpu_line = self.axes.axvline(
            cpu_temp, ymin=0, ymax=100, color=self._cpu_color, label=LABEL_CPU_TEMP, linestyle='dotted', linewidth=1
        )
        cpu_line.set_animated(True)
        self.lines.append(cpu_line)
        _LOG.debug('initialized cpu line')

    def _initialize_gpu_line(self) -> None:
        gpu_temp = 0
        gpu = self._get_first_device_with_name('gpu')
        if gpu and gpu.status.device_temperature:
            gpu_temp = gpu.status.device_temperature
        gpu_line = self.axes.axvline(
            gpu_temp, ymin=0, ymax=100, color=self._gpu_color, label=LABEL_GPU_TEMP, linestyle='dotted', linewidth=1
        )
        gpu_line.set_animated(True)
        self.lines.append(gpu_line)
        _LOG.debug('initialized gpu line')

    def _initialize_device_temp_line(self) -> None:
        device_temp = 0
        if self._device.status.liquid_temperature:
            device_temp = self._device.status.liquid_temperature
        elif self._device.status.device_temperature:
            device_temp = self._device.status.device_temperature
        device_line = self.axes.axvline(
            device_temp, ymin=0, ymax=100, color=self._liquid_temp_color, label=LABEL_DEVICE_TEMP,
            linestyle='dotted', linewidth=1
        )
        device_line.set_animated(True)
        self.lines.append(device_line)
        _LOG.debug('initialized liquidctl lines')

    def _initialize_custom_profile_markers(self) -> None:
        self._profile_points_x = [20, 30, 40, 50, 60, 70, 80, 90, 100]  # degrees
        min_duty = self._device.device_info.channels[self._channel_name].speed_options.min_duty
        max_duty = self._device.device_info.channels[self._channel_name].speed_options.max_duty
        default_duty: List[int] = list(np.linspace(min_duty, max_duty, 9)) \
            if min_duty is not None and max_duty is not None \
            else [20, 40, 80, 100, 100, 100, 100, 100, 100]  # safe default
        self._profile_points_y = default_duty
        profile_line = Line2D(
            self._profile_points_x,
            self._profile_points_y,
            color=self._device_line_color, linestyle='solid', linewidth=2, marker='o', markersize=6,
            label=LABEL_PROFILE_CUSTOM
        )
        profile_line.set_animated(True)
        self.axes.add_line(profile_line)
        self.lines.append(profile_line)
        _LOG.debug('initialized custom profile line')

    def _initialize_fixed_profile_line(self) -> None:
        device_duty_line = self._get_line_by_label(LABEL_DEVICE_DUTY)
        current_duty: int = list(device_duty_line.get_ydata())[0] if device_duty_line else 30
        fixed_line = self.axes.axhline(
            current_duty, xmax=100, color=self._device_line_color, label=LABEL_PROFILE_FIXED,
            linestyle='solid', linewidth=2
        )
        fixed_line.set_animated(True)
        self.lines.append(fixed_line)
        _LOG.debug('initialized fixed profile line')

    def _set_cpu_data(self) -> None:
        cpu = self._get_first_device_with_name('cpu')
        if cpu and cpu.status.device_temperature:
            cpu_temp = int(cpu.status.device_temperature)
            self._get_line_by_label(LABEL_CPU_TEMP).set_xdata([cpu_temp])

    def _set_gpu_data(self) -> None:
        gpu = self._get_first_device_with_name('gpu')
        if gpu and gpu.status.device_temperature:
            gpu_temp = int(gpu.status.device_temperature)
            self._get_line_by_label(LABEL_GPU_TEMP).set_xdata([gpu_temp])

    def _set_device_temp_data(self) -> None:
        liquid_temp = 0
        if self._device.status.liquid_temperature:
            liquid_temp = int(self._device.status.liquid_temperature)
        elif self._device.status.device_temperature:
            liquid_temp = int(self._device.status.device_temperature)
        self._get_line_by_label(LABEL_DEVICE_TEMP).set_xdata([liquid_temp])

    def _set_device_duty_data(self) -> None:
        channel_duty = 0
        channel_rpm = 0
        if self._channel_name == 'pump':
            if self._device.status.pump_duty:
                channel_duty = self._device.status.pump_duty
            # todo: some devices need the duty calculated manually
            if self._device.status.pump_rpm:
                channel_rpm = self._device.status.pump_rpm
        elif self._channel_name == 'fan':
            if self._device.status.fan_duty:
                channel_duty = self._device.status.fan_duty
            if self._device.status.fan_rpm:
                channel_rpm = self._device.status.fan_rpm
        self._get_line_by_label(LABEL_DEVICE_DUTY).set_ydata([channel_duty])
        self.duty_text.set_y(self._calc_text_position(channel_duty))
        self.duty_text.set_text(f'{channel_rpm} rpm')

    def _get_first_device_with_name(self, device_name: str) -> Optional[DeviceStatus]:
        return next(
            (device for device in self._devices_statuses if device.device_name == device_name),
            None
        )

    @staticmethod
    def _calc_text_position(channel_duty: float) -> float:
        return channel_duty + 1 if channel_duty < 90 else channel_duty - 4

    def _get_line_by_label(self, label: str) -> Line2D:
        return next(line for line in self.lines if line.get_label() == label)

    def _redraw_whole_canvas(self) -> None:
        self._blit_cache.clear()
        self._init_draw()
        self.draw()

    def _mouse_button_press(self, event: MouseEvent) -> None:
        if event.inaxes is None or event.button != 1:
            return
        if self._chosen_speed_profile == SpeedProfile.CUSTOM:
            self._active_point_index = self._get_index_near_pointer(event)
        elif self._chosen_speed_profile == SpeedProfile.FIXED:
            self._is_fixed_line_active = self._is_button_clicked_near_line(event)

    def _mouse_button_release(self, event: MouseEvent) -> None:
        if event.button != 1:
            return
        if self._chosen_speed_profile == SpeedProfile.CUSTOM:
            self._active_point_index = None
        elif self._chosen_speed_profile == SpeedProfile.FIXED:
            self._is_fixed_line_active = False

    def _get_index_near_pointer(self, event: MouseEvent) -> Optional[int]:
        """get the index of the vertex under point if within epsilon tolerance"""

        trans_data = self.axes.transData
        x_points_reshaped = np.reshape(self._profile_points_x, (np.shape(self._profile_points_x)[0], 1))
        y_points_reshaped = np.reshape(self._profile_points_y, (np.shape(self._profile_points_y)[0], 1))
        xy_points_reshaped: npt.NDArray = np.append(  # type: ignore[no-untyped-call]
            x_points_reshaped, y_points_reshaped, 1
        )
        xy_points_transformed = trans_data.transform(xy_points_reshaped)
        x_points_transformed, y_points_transformed = xy_points_transformed[:, 0], xy_points_transformed[:, 1]
        distances_to_points: npt.NDArray = np.hypot(x_points_transformed - event.x, y_points_transformed - event.y)
        closest_nonzero_point_indices, = np.nonzero(distances_to_points == np.amin(distances_to_points))
        closest_point_index: int = closest_nonzero_point_indices[0]

        _LOG.debug('Closest point distance: %f', distances_to_points[closest_point_index])
        if distances_to_points[closest_point_index] >= self._epsilon_threshold_pixels:
            return None  # if the click was too far away

        _LOG.debug('Closest Point Index found: %d', closest_point_index)
        return closest_point_index

    def _is_button_clicked_near_line(self, event: MouseEvent) -> bool:
        current_duty: List[int] = list(self._get_line_by_label(LABEL_PROFILE_FIXED).get_ydata())
        distance_from_line: int = abs(event.ydata - current_duty[0])
        _LOG.debug('Distance from Fixed Profile Line: %s', distance_from_line)
        return distance_from_line < self._epsilon_threshold_axis

    def _mouse_motion(self, event: MouseEvent) -> None:
        if event.inaxes is None or event.button != 1:
            return
        if self._active_point_index is not None:
            self._profile_points_y[self._active_point_index] = int(event.ydata)
            for index in range(self._active_point_index + 1, len(self._profile_points_y)):
                if self._profile_points_y[index] < event.ydata:
                    self._profile_points_y[index] = int(event.ydata)
            for index in range(self._active_point_index):
                if self._profile_points_y[index] > event.ydata:
                    self._profile_points_y[index] = int(event.ydata)
            self._get_line_by_label(LABEL_PROFILE_CUSTOM).set_ydata(self._profile_points_y)
            Animation._step(self)
        elif self._is_fixed_line_active:
            self._get_line_by_label(LABEL_PROFILE_FIXED).set_ydata([int(event.ydata)])
            Animation._step(self)
