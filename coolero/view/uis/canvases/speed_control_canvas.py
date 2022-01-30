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
import warnings
from typing import Optional, List

import numpy as np
import numpy.typing as npt
from PySide6.QtCore import Slot
from matplotlib.animation import Animation, FuncAnimation
from matplotlib.artist import Artist
from matplotlib.backend_bases import MouseEvent, DrawEvent, MouseButton
from matplotlib.backends.backend_qt5agg import FigureCanvasQTAgg
from matplotlib.figure import Figure
from matplotlib.lines import Line2D
from matplotlib.text import Annotation
from numpy.linalg import LinAlgError

from models.device import Device, DeviceType
from models.speed_profile import SpeedProfile
from models.temp_source import TempSource
from repositories.cpu_repo import CPU_TEMP
from repositories.gpu_repo import GPU_TEMP
from services.utils import MathUtils
from settings import Settings, ProfileSetting
from view_models.device_subject import DeviceSubject
from view_models.observer import Observer
from view_models.subject import Subject

_LOG = logging.getLogger(__name__)

LABEL_CPU_TEMP: str = 'cpu temp'
LABEL_GPU_TEMP: str = 'gpu temp'
LABEL_DEVICE_TEMP: str = 'device temp'
LABEL_CHANNEL_DUTY: str = 'device duty'
LABEL_PROFILE_FIXED: str = 'profile fixed'
LABEL_PROFILE_CUSTOM: str = 'profile custom'
LABEL_COMPOSITE_TEMP: str = 'composite temp'
DRAW_INTERVAL_MS: int = 1000


class SpeedControlCanvas(FigureCanvasQTAgg, FuncAnimation, Observer, Subject):
    """Class to plot and animate Speed control and status"""

    def __init__(self,
                 device: Device,
                 channel_name: str,
                 starting_temp_source: TempSource,
                 temp_sources: List[TempSource],
                 width: int = 16,
                 height: int = 9,
                 dpi: int = 120,
                 bg_color: str = Settings.theme['app_color']['bg_two'],
                 text_color: str = Settings.theme['app_color']['text_foreground'],
                 channel_duty_line_color_default: str = Settings.theme['app_color']['green'],
                 starting_speed_profile: SpeedProfile = SpeedProfile.NONE
                 ) -> None:
        self._observers: List[Observer] = []
        self._bg_color = bg_color
        self._text_color = text_color
        self._channel_duty_line_color = channel_duty_line_color_default
        self._devices: List[Device] = []
        self._drawn_artists: List[Artist] = []  # used by the matplotlib implementation for blit animation
        self.device = device
        self.channel_name = channel_name
        self._min_channel_duty = self.device.info.channels[self.channel_name].speed_options.min_duty
        self._max_channel_duty = self.device.info.channels[self.channel_name].speed_options.max_duty
        self.current_temp_source: TempSource = starting_temp_source
        self._temp_sources: List[TempSource] = temp_sources
        self.current_speed_profile: SpeedProfile = starting_speed_profile

        # Setup
        self.fig = Figure(figsize=(width, height), dpi=dpi, layout='constrained', facecolor=bg_color,
                          edgecolor=text_color)
        self.axes = self.fig.add_subplot(111, facecolor=bg_color)
        self.axes.set_ylim(-2, 105)  # duty % range
        self.axes.set_xlim(20, self.device.info.temp_max)  # temp C range

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
        self.axes.fill_between(
            np.arange(self.axes.get_xlim()[0], 102),
            self._min_channel_duty, -2,
            facecolor=Settings.theme['app_color']['red'], alpha=0.1
        )
        if self._max_channel_duty < 100:
            self.axes.fill_between(
                np.arange(self.axes.get_xlim()[0], 106),
                self._max_channel_duty, 105,
                facecolor=Settings.theme['app_color']['red'], alpha=0.1
            )

        # Lines
        self.lines: List[Line2D] = []
        self.duty_text: Annotation = Annotation('', (0, 0))

        # interactive
        self.profile_temps: List[int] = []  # degrees
        self.profile_duties: List[int] = []  # duty percent
        self.fixed_duty: int = 0
        self._current_chosen_temp: float = 0.0
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
        FuncAnimation.__init__(self, self.fig, func=self.draw_frame, interval=DRAW_INTERVAL_MS, blit=True)
        _LOG.debug('Initialized %s Speed Graph Canvas', device.name_short)

    @Slot()
    def chosen_temp_source(self, temp_source_name: str) -> None:
        temp_source_btn = self.sender()
        channel_btn_id = temp_source_btn.objectName()
        self.current_temp_source = next(ts for ts in self._temp_sources if ts.name == temp_source_name)
        _LOG.debug('Temp source chosen:  %s from %s', temp_source_name, channel_btn_id)
        self._initialize_chosen_temp_source_lines()
        self.event_source.interval = 100  # quick redraw after change

    @Slot()
    def chosen_speed_profile(self, profile: str) -> None:
        if profile:  # on profile list update .clear() sends an empty string
            profile_btn = self.sender()
            channel_btn_id = profile_btn.objectName()
            _LOG.debug('Speed profile chosen:   %s from %s', profile, channel_btn_id)
            self.current_speed_profile = profile
            for line in list(self.lines):  # list copy as we're modifying in place
                if line.get_label() in [LABEL_PROFILE_FIXED, LABEL_PROFILE_CUSTOM]:
                    self.axes.lines.remove(line)
                    self.lines.remove(line)
            if profile == SpeedProfile.CUSTOM:
                self._initialize_custom_profile_markers()
            elif profile == SpeedProfile.FIXED:
                self._initialize_fixed_profile_line()
            self.event_source.interval = 100  # quick redraw after change

    def draw_frame(self, frame: int) -> List[Artist]:
        """Is used to draw every frame of the chart animation"""

        if self.current_temp_source.device.type == DeviceType.CPU:
            self._set_cpu_data()
        elif self.current_temp_source.device.type == DeviceType.GPU:
            self._set_gpu_data()
        elif self.current_temp_source.device.type == DeviceType.LIQUIDCTL:
            self._set_device_temp_data()
        elif self.current_temp_source.device.type == DeviceType.COMPOSITE:
            self._set_composite_temp_data()
        self._set_device_duty_data()

        self._drawn_artists = list(self.lines)  # pylint: disable=attribute-defined-outside-init
        self._drawn_artists.append(self.duty_text)
        if frame > 0 and frame % 8 == 0:  # clear the blit cache of strange artifacts every so often
            self._redraw_whole_canvas()
        self.event_source.interval = DRAW_INTERVAL_MS  # return to normal speed after first frame
        return self._drawn_artists

    def draw(self) -> None:
        with np.errstate(divide='raise'):
            with warnings.catch_warnings():
                warnings.filterwarnings('error')
                try:
                    super().draw()
                except (LinAlgError, FloatingPointError) as err:
                    # happens due to the collapse and expand animation of the device column, so far not a big deal
                    _LOG.debug('Expected draw error from speed control graph when resizing: %s', err)
                except UserWarning:
                    # Expected error when dynamically changing the axes size
                    _LOG.debug('Expected UserWarning when dynamically resizing axes')

    def notify_me(self, subject: Subject) -> None:
        if isinstance(subject, DeviceSubject) and not self._devices:
            self._devices = subject.devices

    def subscribe(self, observer: Observer) -> None:
        self._observers.append(observer)

    def unsubscribe(self, observer: Observer) -> None:
        self._observers.remove(observer)

    def notify_observers(self) -> None:
        for observer in self._observers:
            observer.notify_me(self)

    def _end_redraw(self, event: DrawEvent) -> None:
        """We override this so that our animation is redrawn quickly after a plot resize"""
        super()._end_redraw(event)
        self.event_source.interval = 100

    def _initialize_device_channel_duty_line(self) -> None:
        channel_duty = self._min_channel_duty
        channel_rpm = None
        for channel_status in self.device.status.channels:
            if self.channel_name == channel_status.name:
                if channel_status.duty is not None:
                    channel_duty = channel_status.duty
                if channel_status.rpm is not None:
                    channel_rpm = channel_status.rpm
                break
        else:
            if self.channel_name == 'sync' and self.device.status.channels:
                channel_status = self.device.status.channels[0]
                if channel_status.duty is not None:
                    channel_duty = channel_status.duty
                if channel_status.rpm is not None:
                    channel_rpm = channel_status.rpm
        if channel_rpm is not None:
            # not all devices report a duty percent, but if there's at least rpm, we can at least display something.
            channel_duty_line = self.axes.axhline(
                channel_duty, xmax=100, color=self._channel_duty_line_color, label=LABEL_CHANNEL_DUTY,
                linestyle='dotted', linewidth=1
            )
            channel_duty_line.set_animated(True)
            self.lines.append(channel_duty_line)
            text_y_position = self._calc_text_position(channel_duty)
            text_x_position = self.current_temp_source.device.info.temp_max
            text_rpm = f'{channel_rpm} rpm'
            self.duty_text = self.axes.annotate(
                text=text_rpm, xy=(text_x_position, text_y_position), ha='right', size=10,
                color=self._channel_duty_line_color,
            )
            self.duty_text.set_animated(True)
        _LOG.debug('initialized channel duty line')

    def _initialize_chosen_temp_source_lines(self) -> None:
        for line in list(self.lines):  # list copy as we're modifying in place
            if line.get_label() in [LABEL_CPU_TEMP, LABEL_GPU_TEMP] \
                    or line.get_label().startswith(LABEL_DEVICE_TEMP) \
                    or line.get_label().startswith(LABEL_COMPOSITE_TEMP):
                self.axes.lines.remove(line)
                self.lines.remove(line)
        if self.current_temp_source.device.type == DeviceType.CPU:
            self._initialize_cpu_line()
        elif self.current_temp_source.device.type == DeviceType.GPU:
            self._initialize_gpu_line()
        elif self.current_temp_source.device.type == DeviceType.LIQUIDCTL \
                and self.current_temp_source.device.status.temps:
            self._initialize_device_temp_line()
        elif self.current_temp_source.device.type == DeviceType.COMPOSITE:
            self._initialize_composite_temp_lines()
        self._redraw_whole_canvas()

    def _initialize_cpu_line(self) -> None:
        cpu_temp = 0
        cpu = self._get_first_device_with_type(DeviceType.CPU)
        if cpu:
            if cpu.status.temps:
                cpu_temp = cpu.status.temps[0].temp
            cpu_line = self.axes.axvline(
                cpu_temp, ymin=0, ymax=100, color=cpu.color(CPU_TEMP), label=LABEL_CPU_TEMP,
                linestyle='solid', linewidth=1
            )
            cpu_line.set_animated(True)
            self.lines.append(cpu_line)
            self.axes.set_xlim(cpu.info.temp_min, cpu.info.temp_max + 1)
            _LOG.debug('initialized cpu line')

    def _initialize_gpu_line(self) -> None:
        gpu_temp = 0
        gpu = self._get_first_device_with_type(DeviceType.GPU)
        if gpu:
            if gpu.status.temps:
                gpu_temp = gpu.status.temps[0].temp
            gpu_line = self.axes.axvline(
                gpu_temp, ymin=0, ymax=100, color=gpu.color(GPU_TEMP), label=LABEL_GPU_TEMP,
                linestyle='solid', linewidth=1
            )
            gpu_line.set_animated(True)
            self.lines.append(gpu_line)
            self.axes.set_xlim(gpu.info.temp_min, gpu.info.temp_max + 1)
            _LOG.debug('initialized gpu line')

    def _initialize_device_temp_line(self) -> None:
        for index, temp_status in enumerate(self.current_temp_source.device.status.temps):
            if self.current_temp_source.name in [temp_status.frontend_name, temp_status.external_name]:
                device_line = self.axes.axvline(
                    temp_status.temp, ymin=0, ymax=100, color=self.current_temp_source.device.color(temp_status.name),
                    label=LABEL_DEVICE_TEMP + str(index),
                    linestyle='solid', linewidth=1
                )
                device_line.set_animated(True)
                self.lines.append(device_line)
                self.axes.set_xlim(
                    self.current_temp_source.device.info.temp_min,
                    self.current_temp_source.device.info.temp_max + 1
                )
        _LOG.debug('initialized device lines')

    def _initialize_composite_temp_lines(self) -> None:
        for index, temp_status in enumerate(self.current_temp_source.device.status.temps):
            if self.current_temp_source.name == temp_status.name:
                composite_line = self.axes.axvline(
                    temp_status.temp, ymin=0, ymax=100, color=self.current_temp_source.device.color(temp_status.name),
                    label=LABEL_COMPOSITE_TEMP + str(index),
                    linestyle='solid', linewidth=1
                )
                composite_line.set_animated(True)
                self.lines.append(composite_line)
                self.axes.set_xlim(
                    self.current_temp_source.device.info.temp_min,
                    self.current_temp_source.device.info.temp_max + 1
                )
        _LOG.debug('initialized composite lines')

    def _initialize_custom_profile_markers(self) -> None:
        saved_profiles: List[ProfileSetting] = Settings.get_temp_source_profiles(
            self.device.name, self.device.lc_device_id, self.channel_name, self.current_temp_source.name
        )
        for profile in saved_profiles:
            if profile.speed_profile == self.current_speed_profile and profile.profile_duties and profile.profile_temps:
                self.profile_temps = profile.profile_temps
                self.profile_duties = profile.profile_duties
                break
        else:
            self.profile_temps = MathUtils.convert_linespace_to_list(
                np.linspace(
                    self.current_temp_source.device.info.temp_min,
                    self.current_temp_source.device.info.temp_max,
                    self.current_temp_source.device.info.profile_max_length
                ))
            self.profile_duties = MathUtils.convert_linespace_to_list(
                np.linspace(
                    self._min_channel_duty, self._max_channel_duty,
                    self.current_temp_source.device.info.profile_max_length
                )
            )
        profile_line = Line2D(
            self.profile_temps,
            self.profile_duties,
            color=self._channel_duty_line_color, linestyle='solid', linewidth=2, marker='o', markersize=6,
            label=LABEL_PROFILE_CUSTOM
        )
        profile_line.set_animated(True)
        self.axes.add_line(profile_line)
        self.lines.append(profile_line)
        _LOG.debug('initialized custom profile line')

    def _initialize_fixed_profile_line(self) -> None:
        saved_profiles: List[ProfileSetting] = Settings.get_temp_source_profiles(
            self.device.name, self.device.lc_device_id, self.channel_name, self.current_temp_source.name
        )
        for profile in saved_profiles:
            if profile.speed_profile == SpeedProfile.FIXED and profile.fixed_duty is not None:
                self.fixed_duty = profile.fixed_duty
                break
        else:
            device_duty_line = self._get_line_by_label(LABEL_CHANNEL_DUTY)
            current_device_duty = int(list(device_duty_line.get_ydata())[0]) if device_duty_line else 0
            self.fixed_duty = current_device_duty or self._min_channel_duty
        fixed_line = self.axes.axhline(
            self.fixed_duty, xmax=100, color=self._channel_duty_line_color, label=LABEL_PROFILE_FIXED,
            linestyle='solid', linewidth=2
        )
        fixed_line.set_animated(True)
        self.lines.append(fixed_line)
        _LOG.debug('initialized fixed profile line')

    def _set_cpu_data(self) -> None:
        cpu = self._get_first_device_with_type(DeviceType.CPU)
        if cpu and cpu.status.temps:
            cpu_temp = int(round(cpu.status.temps[0].temp))
            self._current_chosen_temp = cpu_temp
            self._get_line_by_label(LABEL_CPU_TEMP).set_xdata([cpu_temp])

    def _set_gpu_data(self) -> None:
        gpu = self._get_first_device_with_type(DeviceType.GPU)
        if gpu and gpu.status.temps:
            gpu_temp = int(round(gpu.status.temps[0].temp))
            self._current_chosen_temp = gpu_temp
            self._get_line_by_label(LABEL_GPU_TEMP).set_xdata([gpu_temp])

    def _set_device_temp_data(self) -> None:
        if self.current_temp_source.device.status.temps:
            for index, temp_status in enumerate(self.current_temp_source.device.status.temps):
                if self.current_temp_source.name in [temp_status.frontend_name, temp_status.external_name]:
                    temp = int(round(temp_status.temp))
                    self._current_chosen_temp = temp
                    self._get_line_by_label(LABEL_DEVICE_TEMP + str(index)).set_xdata([temp])

    def _set_composite_temp_data(self) -> None:
        if self.current_temp_source.device.status.temps:
            for index, temp_status in enumerate(self.current_temp_source.device.status.temps):
                if self.current_temp_source.name == temp_status.name:
                    temp = int(round(temp_status.temp))
                    self._current_chosen_temp = temp
                    self._get_line_by_label(LABEL_COMPOSITE_TEMP + str(index)).set_xdata([temp])

    def _set_device_duty_data(self) -> None:
        channel_duty = 0
        channel_rpm = 0
        for channel_status in self.device.status.channels:
            if self.channel_name == channel_status.name:
                if channel_status.duty is not None:
                    channel_duty = channel_status.duty
                if channel_status.rpm is not None:
                    channel_rpm = channel_status.rpm
                break
        else:
            if self.channel_name == 'sync' and self.device.status.channels:
                channel_status = self.device.status.channels[0]
                if channel_status.duty is not None:
                    channel_duty = channel_status.duty
                if channel_status.rpm is not None:
                    channel_rpm = channel_status.rpm
        if not channel_duty and channel_rpm:
            # some devices do not have a duty and should to be calculated based on currently set profile
            if self.current_speed_profile == SpeedProfile.FIXED:
                channel_duty = self.fixed_duty
            elif self.current_speed_profile == SpeedProfile.CUSTOM:
                profile = MathUtils.convert_axis_to_profile(self.profile_temps, self.profile_duties)
                channel_duty = MathUtils.interpolate_profile(
                    MathUtils.normalize_profile(profile, 100, 100), self._current_chosen_temp
                )
            else:
                channel_duty = self._min_channel_duty
        self._get_line_by_label(LABEL_CHANNEL_DUTY).set_ydata([channel_duty])
        self.duty_text.set_x(self.current_temp_source.device.info.temp_max)
        self.duty_text.set_y(self._calc_text_position(channel_duty))
        self.duty_text.set_text(f'{channel_rpm} rpm')

    def _get_first_device_with_type(self, device_type: DeviceType) -> Optional[Device]:
        return next(
            iter(self._get_devices_with_type(device_type)),
            None
        )

    def _get_devices_with_type(self, device_type: DeviceType) -> List[Device]:
        return [device for device in self._devices if device.type == device_type]

    @staticmethod
    def _calc_text_position(channel_duty: float) -> float:
        return channel_duty + 1 if channel_duty < 90 else channel_duty - 4

    def _get_line_by_label(self, label: str) -> Line2D:
        try:
            return next(line for line in self.lines if line.get_label().startswith(label))
        except StopIteration:
            _LOG.error('No Initialized Plot Line found for label: %s', label)
            return Line2D([], [])

    def _redraw_whole_canvas(self) -> None:
        self._blit_cache.clear()
        self._init_draw()
        self.draw()

    def _mouse_button_press(self, event: MouseEvent) -> None:
        if event.inaxes is None or event.button != 1:
            return
        if self.current_speed_profile == SpeedProfile.CUSTOM:
            self._active_point_index = self._get_index_near_pointer(event)
            if self._active_point_index is not None \
                    and self._active_point_index + 1 == self.current_temp_source.device.info.profile_max_length:
                # the critical/highest temp is not changeable from 100%
                self._active_point_index = None
        elif self.current_speed_profile == SpeedProfile.FIXED:
            self._is_fixed_line_active = self._is_button_clicked_near_line(event)

    def _mouse_button_release(self, event: MouseEvent) -> None:
        if event.button != 1:
            return
        if self.current_speed_profile == SpeedProfile.CUSTOM:
            self._active_point_index = None
            self.notify_observers()
        elif self.current_speed_profile == SpeedProfile.FIXED:
            self._is_fixed_line_active = False
            self.notify_observers()

    def _get_index_near_pointer(self, event: MouseEvent) -> Optional[int]:
        """get the index of the vertex under point if within epsilon tolerance"""

        trans_data = self.axes.transData
        x_points_reshaped = np.reshape(self.profile_temps, (np.shape(self.profile_temps)[0], 1))
        y_points_reshaped = np.reshape(self.profile_duties, (np.shape(self.profile_duties)[0], 1))
        xy_points_reshaped: npt.NDArray = np.append(
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
        if event.inaxes is None or event.button != MouseButton.LEFT:
            return
        pointer_y_position: int = int(event.ydata)
        if pointer_y_position < self._min_channel_duty:
            pointer_y_position = self._min_channel_duty
        elif pointer_y_position > self._max_channel_duty:
            pointer_y_position = self._max_channel_duty
        if self._active_point_index is not None:
            self.profile_duties[self._active_point_index] = pointer_y_position
            for index in range(self._active_point_index + 1, len(self.profile_duties)):
                if self.profile_duties[index] < pointer_y_position:
                    self.profile_duties[index] = pointer_y_position
            for index in range(self._active_point_index):
                if self.profile_duties[index] > pointer_y_position:
                    self.profile_duties[index] = pointer_y_position
            self._get_line_by_label(LABEL_PROFILE_CUSTOM).set_ydata(self.profile_duties)
            Animation._step(self)
        elif self._is_fixed_line_active:
            self.fixed_duty = pointer_y_position
            self._get_line_by_label(LABEL_PROFILE_FIXED).set_ydata([pointer_y_position])
            Animation._step(self)
