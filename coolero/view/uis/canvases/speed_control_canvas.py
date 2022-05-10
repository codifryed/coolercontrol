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
from math import dist
from typing import Optional, List, Dict

import numpy as np
from PySide6.QtCore import Slot, Qt
from matplotlib.animation import Animation, FuncAnimation
from matplotlib.artist import Artist
from matplotlib.backend_bases import MouseEvent, DrawEvent, MouseButton, KeyEvent
from matplotlib.backends.backend_qt5agg import FigureCanvasQTAgg
from matplotlib.figure import Figure
from matplotlib.lines import Line2D
from matplotlib.text import Annotation
from numpy import errstate
from numpy.linalg import LinAlgError

from coolero.models.device import Device, DeviceType
from coolero.models.init_status import InitStatus
from coolero.models.speed_profile import SpeedProfile
from coolero.models.temp_source import TempSource
from coolero.repositories.cpu_repo import CPU_TEMP
from coolero.services.utils import MathUtils
from coolero.settings import Settings, ProfileSetting
from coolero.view_models.device_subject import DeviceSubject
from coolero.view_models.observer import Observer
from coolero.view_models.subject import Subject

_LOG = logging.getLogger(__name__)

LABEL_CPU_TEMP: str = 'cpu temp'
LABEL_GPU_TEMP: str = 'gpu temp'
LABEL_DEVICE_TEMP: str = 'device temp'
LABEL_CHANNEL_DUTY: str = 'device duty'
LABEL_PROFILE_FIXED: str = 'profile fixed'
LABEL_PROFILE_CUSTOM: str = 'profile custom'
LABEL_PROFILE_CUSTOM_MARKER: str = 'profile custom marker'
LABEL_COMPOSITE_TEMP: str = 'composite temp'
DRAW_INTERVAL_MS: int = 1_000
_TEMP_MARGIN_MAX: int = 1
_TEMP_MARGIN_MIN: int = -1


class SpeedControlCanvas(FigureCanvasQTAgg, FuncAnimation, Observer, Subject):
    """Class to plot and animate Speed control and status"""

    def __init__(self,
                 device: Device,
                 channel_name: str,
                 starting_temp_source: TempSource,
                 temp_sources: List[TempSource],
                 init_status: InitStatus,
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
        self._init_status: InitStatus = init_status
        self.current_speed_profile: SpeedProfile = starting_speed_profile

        # Setup
        self.fig = Figure(figsize=(width, height), dpi=dpi, layout='constrained', facecolor=bg_color,
                          edgecolor=text_color)
        self.axes = self.fig.add_subplot(111, facecolor=bg_color)
        self.axes.set_ylim(-2, 105)  # duty % range
        self.axes.set_xlim(-1, self.device.info.temp_max)  # temp C range

        # Grid
        self.axes.grid(True, linestyle='dotted', color=text_color, alpha=0.5)
        self.axes.margins(x=0, y=0.05)
        self.axes.tick_params(colors=text_color)
        self.axes.set_xticks(
            [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100],
            ['0°', '10°', '20°', '30°', '40°', '50°', '60°', '70°', '80°', '90°', '100°'])
        self.axes.set_yticks(
            [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100],
            ['0%', '10%', '20%', '30%', '40%', '50%', '60%', '70%', '80%', '90%', '100%'])
        self.axes.spines['top'].set_edgecolor(bg_color)
        self.axes.spines['top'].set_animated(True)
        self.axes.spines['right'].set_edgecolor(bg_color)
        self.axes.spines['right'].set_animated(True)
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
        self._epsilon_threshold_pixels: int = 16
        self._epsilon_threshold_points: int = 9  # (120 dpi / 72 * 9 = 15 pixels)
        self._button_press_cid: Optional[int] = self.fig.canvas.mpl_connect('button_press_event',
                                                                            self._mouse_button_press)
        self._button_release_cid: Optional[int] = self.fig.canvas.mpl_connect('button_release_event',
                                                                              self._mouse_button_release)
        self._mouse_motion_cid: Optional[int] = self.fig.canvas.mpl_connect('motion_notify_event', self._mouse_motion)
        self._key_press_cid: Optional[int] = self.fig.canvas.mpl_connect('key_press_event', self._key_press)

        # Initialize
        self._initialize_device_channel_duty_line()
        self.temp_text: Annotation = self.axes.annotate(
            text='', xy=(30, 101), size=10, rotation='vertical', va='top', ha='left'
        )
        self.temp_text.set_animated(True)
        self.marker_text: Annotation = self.axes.annotate(
            text='', xy=(30, 30), xycoords='data',
            size=10, va='center', ha='center',
            color=self._channel_duty_line_color,
            xytext=(0, 25), textcoords='offset points',
            bbox={'boxstyle': 'round', 'fc': self._bg_color, 'ec': self._text_color, 'alpha': 0.9},
        )
        self.marker_text.set_animated(True)
        self.marker_text.set_visible(False)
        self.fixed_text: Annotation = self.axes.annotate(
            text='', xy=(30, 30), xycoords='data',
            size=10, va='center', ha='center',
            color=self._channel_duty_line_color,
            xytext=(0, 25), textcoords='offset points',
            bbox={'boxstyle': 'round', 'fc': self._bg_color, 'ec': self._text_color, 'alpha': 0.9},
        )
        self.fixed_text.set_animated(True)
        self.fixed_text.set_visible(False)
        FigureCanvasQTAgg.__init__(self, self.fig)
        FuncAnimation.__init__(self, self.fig, func=self.draw_frame, interval=DRAW_INTERVAL_MS, blit=True)
        self.fig.canvas.setFocusPolicy(Qt.StrongFocus)
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
        if not profile:  # on profile list update .clear() sends an empty string
            return
        profile_btn = self.sender()
        channel_btn_id = profile_btn.objectName()
        _LOG.debug('Speed profile chosen:   %s from %s', profile, channel_btn_id)
        self.current_speed_profile = profile
        for line in list(self.lines):  # list copy as we're modifying in place
            if line.get_label() in [LABEL_PROFILE_FIXED, LABEL_PROFILE_CUSTOM, LABEL_PROFILE_CUSTOM_MARKER]:
                self.axes.lines.remove(line)
                self.lines.remove(line)
        if profile == SpeedProfile.CUSTOM:
            self._initialize_custom_profile_markers()
        elif profile == SpeedProfile.FIXED:
            self._initialize_fixed_profile_line()
        self.event_source.interval = 100  # quick redraw after change
        if self._init_status.complete:
            self.notify_observers()

    def draw_frame(self, frame: int) -> List[Artist]:
        """Is used to draw every frame of the chart animation"""

        if self.current_temp_source.device.type == DeviceType.CPU:
            self._set_cpu_data()
        elif self.current_temp_source.device.type == DeviceType.GPU:
            self._set_gpu_data()
        elif self.current_temp_source.device.type in [DeviceType.LIQUIDCTL, DeviceType.HWMON]:
            self._set_device_temp_data()
        elif self.current_temp_source.device.type == DeviceType.COMPOSITE:
            self._set_composite_temp_data()
        self._set_device_duty_data()

        self._drawn_artists = list(self.lines)  # pylint: disable=attribute-defined-outside-init
        self._drawn_artists.append(self.duty_text)
        self._drawn_artists.append(self.temp_text)
        self._drawn_artists.append(self.marker_text)
        self._drawn_artists.append(self.fixed_text)
        self._drawn_artists.append(self.axes.spines['top'])
        self._drawn_artists.append(self.axes.spines['right'])
        self.event_source.interval = DRAW_INTERVAL_MS  # return to normal speed after first frame
        return self._drawn_artists

    def draw(self) -> None:
        with errstate(divide='raise'):
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
            text_x_position = 50  # setting to absolute minimum at startup fixed strange bug when scaling later
            text_rpm = f'{channel_rpm} rpm'
            self.duty_text = self.axes.annotate(
                text=text_rpm, xy=(text_x_position, channel_duty), ha='right', va='bottom', size=10,
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
        elif self.current_temp_source.device.type in [DeviceType.LIQUIDCTL, DeviceType.HWMON] \
                and self.current_temp_source.device.status.temps:
            self._initialize_device_temp_line()
        elif self.current_temp_source.device.type == DeviceType.COMPOSITE:
            self._initialize_composite_temp_lines()
        self._redraw_whole_canvas()

    def _initialize_cpu_line(self) -> None:
        cpu_temp = 0
        if cpu := self._get_first_device_with_type(DeviceType.CPU):
            if cpu.status.temps:
                cpu_temp = cpu.status.temps[0].temp
            cpu_line = self.axes.axvline(
                cpu_temp, ymin=0, ymax=100, color=cpu.color(CPU_TEMP), label=LABEL_CPU_TEMP,
                linestyle='solid', linewidth=1
            )
            cpu_line.set_animated(True)
            self.lines.append(cpu_line)
            self.axes.set_xlim(cpu.info.temp_min + _TEMP_MARGIN_MIN, cpu.info.temp_max + _TEMP_MARGIN_MAX)
            self._set_temp_text_position(cpu_temp)
            self.temp_text.set_color(cpu.color(CPU_TEMP))
            self.temp_text.set_text(f'{cpu_temp}°')
            _LOG.debug('initialized cpu line')

    def _initialize_gpu_line(self) -> None:
        if self.current_temp_source.device.status.temps:
            gpu = self.current_temp_source.device
            gpu_temp_status = gpu.status.temps[0]
            gpu_temp = gpu_temp_status.temp
            gpu_line = self.axes.axvline(
                gpu_temp, ymin=0, ymax=100, color=gpu.color(gpu_temp_status.name),
                label=LABEL_GPU_TEMP,
                linestyle='solid', linewidth=1
            )
            gpu_line.set_animated(True)
            self.lines.append(gpu_line)
            self.axes.set_xlim(gpu.info.temp_min + _TEMP_MARGIN_MIN, gpu.info.temp_max + _TEMP_MARGIN_MAX)
            self._set_temp_text_position(gpu_temp)
            self.temp_text.set_color(gpu.color(gpu_temp_status.name))
            self.temp_text.set_text(f'{gpu_temp}°')
        _LOG.debug('initialized gpu lines')

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
                    self.current_temp_source.device.info.temp_min + _TEMP_MARGIN_MIN,
                    self.current_temp_source.device.info.temp_max + _TEMP_MARGIN_MAX
                )
                self._set_temp_text_position(temp_status.temp)
                self.temp_text.set_color(self.current_temp_source.device.color(temp_status.name))
                self.temp_text.set_text(f'{temp_status.temp}°')
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
                    self.current_temp_source.device.info.temp_min + _TEMP_MARGIN_MIN,
                    self.current_temp_source.device.info.temp_max + _TEMP_MARGIN_MAX
                )
                self._set_temp_text_position(temp_status.temp)
                self.temp_text.set_color(self.current_temp_source.device.color(temp_status.name))
                self.temp_text.set_text(f'{temp_status.temp}°')
        _LOG.debug('initialized composite lines')

    def _initialize_custom_profile_markers(self) -> None:
        saved_profiles: List[ProfileSetting] = Settings.get_temp_source_profiles(
            self.device.name, self.device.type_id, self.channel_name, self.current_temp_source.name
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
            label=LABEL_PROFILE_CUSTOM,
            pickradius=self._epsilon_threshold_points  # used to determine if the mouse cursor is close to this line
        )
        profile_line.set_animated(True)
        profile_hover_marker = Line2D(
            [0], [0], color=self._channel_duty_line_color, linestyle=None,
            marker='o', markersize=self._epsilon_threshold_pixels,
            label=LABEL_PROFILE_CUSTOM_MARKER
        )
        profile_hover_marker.set_visible(False)
        profile_hover_marker.set_animated(True)
        self.axes.add_line(profile_line)
        self.axes.add_line(profile_hover_marker)
        self.lines.append(profile_line)
        self.lines.append(profile_hover_marker)
        _LOG.debug('initialized custom profile line')

    def _initialize_fixed_profile_line(self) -> None:
        saved_profiles: List[ProfileSetting] = Settings.get_temp_source_profiles(
            self.device.name, self.device.type_id, self.channel_name, self.current_temp_source.name
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
            linestyle='solid', linewidth=2,
            pickradius=self._epsilon_threshold_points + 5  # used to determine if the mouse cursor is close to this line
        )
        fixed_line.set_animated(True)
        self.lines.append(fixed_line)
        _LOG.debug('initialized fixed profile line')

    def _set_cpu_data(self) -> None:
        cpu = self._get_first_device_with_type(DeviceType.CPU)
        if cpu and cpu.status.temps:
            cpu_temp: float = round(cpu.status.temps[0].temp, 1)
            self._current_chosen_temp = cpu_temp
            self._get_line_by_label(LABEL_CPU_TEMP).set_xdata([cpu_temp])
            self._set_temp_text_position(cpu_temp)
            self.temp_text.set_text(f'{cpu_temp}°')

    def _set_gpu_data(self) -> None:
        if self.current_temp_source.device.status.temps:
            gpu_temp: float = round(self.current_temp_source.device.status.temps[0].temp, 1)
            self._current_chosen_temp = gpu_temp
            self._get_line_by_label(LABEL_GPU_TEMP).set_xdata([gpu_temp])
            self._set_temp_text_position(gpu_temp)
            self.temp_text.set_text(f'{gpu_temp}°')

    def _set_device_temp_data(self) -> None:
        if self.current_temp_source.device.status.temps:
            for index, temp_status in enumerate(self.current_temp_source.device.status.temps):
                if self.current_temp_source.name in [temp_status.frontend_name, temp_status.external_name]:
                    temp: float = round(temp_status.temp, 1)
                    self._current_chosen_temp = temp
                    self._get_line_by_label(LABEL_DEVICE_TEMP + str(index)).set_xdata([temp])
                    self._set_temp_text_position(temp)
                    self.temp_text.set_text(f'{temp}°')

    def _set_composite_temp_data(self) -> None:
        if self.current_temp_source.device.status.temps:
            for index, temp_status in enumerate(self.current_temp_source.device.status.temps):
                if self.current_temp_source.name == temp_status.name:
                    temp: float = round(temp_status.temp, 1)
                    self._current_chosen_temp = temp
                    self._get_line_by_label(LABEL_COMPOSITE_TEMP + str(index)).set_xdata([temp])
                    self._set_temp_text_position(temp)
                    self.temp_text.set_text(f'{temp}°')

    def _set_device_duty_data(self) -> None:
        channel_duty = None
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
        if channel_duty is None and channel_rpm:
            # some devices do not have a duty and should to be calculated based on currently set profile
            if self.current_speed_profile == SpeedProfile.FIXED:
                channel_duty = self.fixed_duty
            elif self.current_speed_profile == SpeedProfile.CUSTOM:
                profile = MathUtils.convert_axis_to_profile(self.profile_temps, self.profile_duties)
                channel_duty = MathUtils.interpol_profile(
                    MathUtils.norm_profile(profile, 100, 100), self._current_chosen_temp
                )
            else:
                channel_duty = self._min_channel_duty
        self._get_line_by_label(LABEL_CHANNEL_DUTY).set_ydata([channel_duty])
        self._set_duty_text_position(channel_duty)
        self.duty_text.set_text(f'{channel_rpm} rpm')

    def _get_first_device_with_type(self, device_type: DeviceType) -> Optional[Device]:
        return next(
            iter(self._get_devices_with_type(device_type)),
            None
        )

    def _get_devices_with_type(self, device_type: DeviceType) -> List[Device]:
        return [device for device in self._devices if device.type == device_type]

    def _set_duty_text_position(self, channel_duty: float) -> None:
        self.duty_text.set_x(self.current_temp_source.device.info.temp_max)
        if channel_duty < 90:
            self.duty_text.set_verticalalignment('bottom')
            self.duty_text.set_y(channel_duty)
        else:
            self.duty_text.set_verticalalignment('top')
            self.duty_text.set_y(channel_duty - 0.4)

    def _set_temp_text_position(self, temp: float) -> None:
        """the offset calculation is required due to the changing x_limit values set by temp_max"""
        placement_swap_threshold: int = self.current_temp_source.device.info.temp_min + 5  # type: ignore
        if temp < placement_swap_threshold:
            x_limit_offset = 0.4 if self.current_temp_source.device.info.temp_max > 80 else 0.2  # type: ignore
            self.temp_text.set_horizontalalignment('left')
            self.temp_text.set_x(temp + x_limit_offset)
        else:
            self.temp_text.set_horizontalalignment('right')
            self.temp_text.set_x(temp)

    def _set_marker_text_and_position(self, x_temp: int, y_duty: int) -> None:
        self.marker_text.set_text(f'{x_temp}° {y_duty}%')
        y_offset = 25 if y_duty < 90 else -25
        x_temp_min_threshold = self.current_temp_source.device.info.temp_min + 5
        x_temp_max_threshold = self.current_temp_source.device.info.temp_max - 5
        if x_temp < x_temp_min_threshold:
            self.marker_text.set_horizontalalignment('left')
        elif x_temp > x_temp_max_threshold:
            self.marker_text.set_horizontalalignment('right')
        else:
            self.marker_text.set_horizontalalignment('center')
        self.marker_text.set_y(y_offset)  # text location in offset "points"
        self.marker_text.xy = (x_temp, y_duty)  # the focal point location in data points

    def _set_fixed_text_and_position(self, y_duty: int) -> None:
        self.fixed_text.set_text(f'{y_duty}%')
        y_offset = 25 if y_duty < 90 else -25
        x_temp_middle = self.current_temp_source.device.info.temp_min + round(
            (self.current_temp_source.device.info.temp_max - self.current_temp_source.device.info.temp_min) / 2
        )
        self.fixed_text.set_x(0)
        self.fixed_text.set_y(y_offset)  # text location in offset "points"
        self.fixed_text.xy = (x_temp_middle, y_duty)  # the focal point location in data points

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
        if event.inaxes is None or event.button != MouseButton.LEFT:
            return
        if self.current_speed_profile == SpeedProfile.CUSTOM:
            self._active_point_index = self._get_custom_profile_index_near_pointer(event)
            if self._active_point_index is not None \
                    and self._active_point_index + 1 == self.current_temp_source.device.info.profile_max_length:
                # the critical/highest temp is not changeable from 100%
                self._active_point_index = None
        elif self.current_speed_profile == SpeedProfile.FIXED:
            self._is_fixed_line_active = self._is_button_clicked_near_line(event)

    def _mouse_button_release(self, event: MouseEvent) -> None:
        if event.button != MouseButton.LEFT:
            return
        if self.current_speed_profile == SpeedProfile.CUSTOM:
            self._active_point_index = None
        elif self.current_speed_profile == SpeedProfile.FIXED:
            self._is_fixed_line_active = False
        self.notify_observers()

    def _get_custom_profile_index_near_pointer(self, event: MouseEvent) -> int | None:
        """get the index of the nearest index coordinate if within the epsilon tolerance"""
        contains, details = self._get_line_by_label(LABEL_PROFILE_CUSTOM).contains(event)
        # details['ind'] is a list of nearby indexes. Unfortunately the index distances are calculated per line
        #  'segment', instead of points on the line. This requires us to do some extra distance calculations.
        #  It's still very helpful to know if the mouse is over the line without lots of calculations.
        if not contains:
            return None
        index_of_nearby_line_segment = details['ind'][0]
        if index_of_nearby_line_segment + 1 == len(self.profile_duties):
            return index_of_nearby_line_segment  # last line segment works like a point
        indices_distances: Dict[int, float] = {
            index: dist(  # calculate pixel distance
                self.axes.transData.transform((self.profile_temps[index], self.profile_duties[index])),
                (event.x, event.y)
            )
            for index in [index_of_nearby_line_segment, index_of_nearby_line_segment + 1]  # line segment workaround
        }
        min_distance_index = min(indices_distances, key=indices_distances.get)
        return min_distance_index if indices_distances[min_distance_index] < self._epsilon_threshold_pixels else None

    def _is_button_clicked_near_line(self, event: MouseEvent) -> bool:
        contains, _ = self._get_line_by_label(LABEL_PROFILE_FIXED).contains(event)
        return contains is not None and contains

    def _mouse_motion(self, event: MouseEvent) -> None:
        if event.inaxes is None:
            if event.button != MouseButton.LEFT:  # clear any leftover hover text from fast mouse movement
                if self.marker_text.get_visible():
                    self._get_line_by_label(LABEL_PROFILE_CUSTOM_MARKER).set_visible(False)
                    self.marker_text.set_visible(False)
                elif self.fixed_text.get_visible():
                    self.fixed_text.set_visible(False)
            return
        if event.button == MouseButton.LEFT:
            self._mouse_motion_move_line(event)
        elif event.button is None:  # Hovering
            if self.current_speed_profile == SpeedProfile.CUSTOM:
                hover_active_point_index = self._get_custom_profile_index_near_pointer(event)
                if hover_active_point_index is not None:
                    active_x = self.profile_temps[hover_active_point_index]
                    active_y = self.profile_duties[hover_active_point_index]
                    self._get_line_by_label(LABEL_PROFILE_CUSTOM_MARKER).set_data(active_x, active_y)
                    self._get_line_by_label(LABEL_PROFILE_CUSTOM_MARKER).set_visible(True)
                    self._set_marker_text_and_position(active_x, active_y)
                    self.marker_text.set_visible(True)
                    Animation._step(self)
                    return
                if self._get_line_by_label(LABEL_PROFILE_CUSTOM_MARKER).get_visible():
                    self._get_line_by_label(LABEL_PROFILE_CUSTOM_MARKER).set_visible(False)
                    self.marker_text.set_visible(False)
                    Animation._step(self)
            elif self.current_speed_profile == SpeedProfile.FIXED:
                contains, _ = self._get_line_by_label(LABEL_PROFILE_FIXED).contains(event)
                if contains:
                    if not self.fixed_text.get_visible():
                        self._set_fixed_text_and_position(self.fixed_duty)
                        self.fixed_text.set_visible(True)
                        Animation._step(self)
                    return
                if self.fixed_text.get_visible():
                    self.fixed_text.set_visible(False)
                    Animation._step(self)

    def _mouse_motion_move_line(self, event):
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
            self._get_line_by_label(LABEL_PROFILE_CUSTOM_MARKER).set_ydata(
                [self.profile_duties[self._active_point_index]]
            )
            self._set_marker_text_and_position(
                self.profile_temps[self._active_point_index], self.profile_duties[self._active_point_index]
            )
            Animation._step(self)
        elif self._is_fixed_line_active:
            self.fixed_duty = pointer_y_position
            self._get_line_by_label(LABEL_PROFILE_FIXED).set_ydata([pointer_y_position])
            self._set_fixed_text_and_position(self.fixed_duty)
            Animation._step(self)

    def _key_press(self, event: KeyEvent) -> None:
        if event.key in ['ctrl+r', 'ctrl+R', 'f5'] and self.current_speed_profile == SpeedProfile.CUSTOM:
            self.profile_duties = MathUtils.convert_linespace_to_list(
                np.linspace(
                    self._min_channel_duty, self._max_channel_duty,
                    self.current_temp_source.device.info.profile_max_length
                )
            )
            self._get_line_by_label(LABEL_PROFILE_CUSTOM).set_ydata(self.profile_duties)
            Animation._step(self)
            _LOG.debug('Custom Profile Reset')
