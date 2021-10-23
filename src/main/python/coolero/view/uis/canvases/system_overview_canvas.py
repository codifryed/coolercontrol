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

# NOTE: !!! using matplot 3.5.0b1
#  (hopefully a stable release is done soon as pyside6 is only supported in beta right now)

import logging
from datetime import datetime
from typing import Optional, List, Iterator, Any

from matplotlib.animation import TimedAnimation
from matplotlib.artist import Artist
from matplotlib.backends.backend_qt5agg import FigureCanvasQTAgg
from matplotlib.figure import Figure
from matplotlib.legend import Legend
from matplotlib.lines import Line2D

from models.device_status import DeviceStatus
from models.status import Status
from view_models.device_observers import DeviceObserver, DeviceSubject

_LOG = logging.getLogger(__name__)
CPU_TEMP: str = 'CPU Temp'
CPU_LOAD: str = 'CPU Load'
CPU_COLOR: str = 'red'
GPU_LOAD: str = 'GPU Load'
GPU_TEMP: str = 'GPU Temp'
GPU_COLOR: str = 'orange'
DEVICE_TEMP: str = ' Device Temp'
DEVICE_LIQUID_TEMP: str = ' Liquid Temp'
DEVICE_PUMP: str = ' Pump Duty'
DEVICE_FAN: str = ' Fan Duty'
DRAW_INTERVAL_MS: int = 500


class SystemOverviewCanvas(FigureCanvasQTAgg, TimedAnimation, DeviceObserver):
    """Class to plot and animate System Overview histogram"""

    _cpu_lines_initialized: bool = False
    _gpu_lines_initialized: bool = False
    _liquidctl_lines_initialized: bool = False

    def __init__(self,
                 width: int = 16,  # width/height ratio & inches for print
                 height: int = 9,
                 dpi: int = 120,
                 bg_color: str = '#000000',
                 text_color: str = '#ffffff'
                 ) -> None:
        self.bg_color = bg_color
        self.text_color = text_color
        self._devices_statuses: List[DeviceStatus] = list()
        self._drawn_artists: List[Artist] = []  # used by the matplotlib implementation for blit animation
        # todo: create button for 5, 10 and 15 size charts (quasi zoom)
        self.x_limit: int = 5 * 60  # the age, in seconds, of data to display

        # Setup
        self.fig = Figure(figsize=(width, height), dpi=dpi, layout='tight', facecolor=bg_color, edgecolor=text_color)
        self.axes = self.fig.add_subplot(111, facecolor=bg_color)
        self.axes.set_title('System Overview', color=text_color)
        # self.axes.set_xlabel('Age', color=text_color)
        # self.axes.set_ylabel('Temp [°C] / Load [%]', color=text_color)
        # self.axes.set_ylabel('°C / %', color=text_color)
        self.axes.set_ylim(0, 101)
        self.axes.set_xlim(self.x_limit, 0)  # could make this modifiable to scaling & zoom

        # Grid
        self.axes.grid(True, linestyle='dotted', color=text_color, alpha=0.5)
        self.axes.margins(x=0, y=0.05)
        self.axes.tick_params(colors=text_color)
        # todo: dynamically set by button above
        self.axes.set_xticks([30, 60, 120, 180, 240, 300], ['30s', '1m', '2m', '3m', '4m', '5m'])
        # self.axes.set_yticks([10, 20, 30, 40, 50, 60, 70, 80, 90, 100])
        self.axes.set_yticks(
            [10, 20, 30, 40, 50, 60, 70, 80, 90, 100],
            ['10°/%', '20°/%', '30°/%', '40°/%', '50°/%', '60°/%', '70°/%', '80°/%', '90°/%', '100°/%', ])
        self.axes.spines['top'].set_edgecolor(text_color + '00')
        self.axes.spines['right'].set_edgecolor(text_color + '00')
        self.axes.spines[['bottom', 'left']].set_edgecolor(text_color)

        # Lines
        self.lines: List[Line2D] = []

        # todo: create annotations for
        #  1. current actual value on the right
        #  2. peaks and troughs annotated with arrow thingy (perhaps only on cpu temp???)

        # Initialize
        FigureCanvasQTAgg.__init__(self, self.fig)
        TimedAnimation.__init__(self, self.fig, interval=DRAW_INTERVAL_MS, blit=True)

    def _draw_frame(self, framedata: int) -> None:
        """Is used to draw every frame of the chart animation"""

        # _LOG.debug("Statuses: %s", self._devices_statuses)
        now: datetime = datetime.now()
        self._set_cpu_data(now)
        self._set_gpu_data(now)
        self._set_liquidctl_device_data(now)
        self._drawn_artists = list(self.lines)
        for artist in self._drawn_artists:
            artist.set_animated(True)
        # todo: clear the blit cache every so often to clear out any drawing artifacts(happens every so often)

    def new_frame_seq(self) -> Iterator[int]:
        return iter(range(self.x_limit))

    def _step(self, *args: Any) -> None:
        # helpful to handle unexpected exceptions:
        try:
            TimedAnimation._step(self, *args)
        except BaseException as ex:
            TimedAnimation._stop(self)
            _LOG.error('Error animating system overview chart: ', ex)

    def notify(self, observable: DeviceSubject) -> None:
        if not self._devices_statuses:
            self._devices_statuses = observable.device_statuses

        if not self._cpu_lines_initialized and self._get_first_device_with_name('cpu'):
            self._initialize_cpu_lines()

        if not self._gpu_lines_initialized and self._get_first_device_with_name('gpu'):
            self._initialize_gpu_lines()

        if not self._liquidctl_lines_initialized:
            devices = self._get_liquidctl_devices()
            if devices:
                self._initialize_liquidctl_lines(devices)

    def init_legend(self, bg_color: str, text_color: str) -> Legend:
        legend = self.axes.legend(loc='upper left', facecolor=bg_color, edgecolor=text_color)
        for text in legend.get_texts():
            text.set_color(text_color)
        return legend

    def _set_cpu_data(self, now: datetime) -> None:
        cpu = self._get_first_device_with_name('cpu')
        if self._cpu_lines_initialized and cpu:
            cpu_history: List[Status] = cpu.status_history
            cpu_temps: List[float] = []
            cpu_loads: List[float] = []
            cpu_status_ages: List[int] = []
            for status in cpu_history[-self.x_limit:]:
                cpu_temps.append(status.device_temperature)
                cpu_loads.append(status.load_percent)
                cpu_status_ages.append(
                    (now - status.timestamp).seconds
                )

            self._get_line_by_label(CPU_TEMP).set_data(cpu_status_ages, cpu_temps)
            self._get_line_by_label(CPU_LOAD).set_data(cpu_status_ages, cpu_loads)

    def _set_gpu_data(self, now: datetime) -> None:
        gpu = self._get_first_device_with_name('gpu')
        if self._gpu_lines_initialized and gpu:
            gpu_history: List[Status] = gpu.status_history
            gpu_temps: List[float] = []
            gpu_loads: List[float] = []
            gpu_status_ages: List[int] = []
            for status in gpu_history[-self.x_limit:]:
                gpu_temps.append(status.device_temperature)
                gpu_loads.append(status.load_percent)
                gpu_status_ages.append(
                    (now - status.timestamp).seconds
                )
            self._get_line_by_label(GPU_TEMP).set_data(gpu_status_ages, gpu_temps)
            self._get_line_by_label(GPU_LOAD).set_data(gpu_status_ages, gpu_loads)

    def _set_liquidctl_device_data(self, now: datetime) -> None:
        if self._liquidctl_lines_initialized:
            for device in self._get_liquidctl_devices():
                device_temps: List[float] = []
                device_liquid_temps: List[float] = []
                device_pump: List[float] = []
                device_fan: List[float] = []
                device_status_ages: List[int] = []
                for status in device.status_history[-self.x_limit:]:
                    if status.device_temperature:
                        device_temps.append(status.device_temperature)
                    if status.liquid_temperature:
                        device_liquid_temps.append(status.liquid_temperature)
                    if status.pump_duty:
                        device_pump.append(status.pump_duty)
                    if status.fan_duty:
                        device_fan.append(status.fan_duty)
                    device_status_ages.append(
                        (now - status.timestamp).seconds
                    )
                if device_temps:
                    self._get_line_by_label(
                        device.device_name_short + DEVICE_TEMP
                    ).set_data(device_status_ages, device_temps)
                if device_liquid_temps:
                    self._get_line_by_label(
                        device.device_name_short + DEVICE_LIQUID_TEMP
                    ).set_data(device_status_ages, device_liquid_temps)
                if device_pump:
                    self._get_line_by_label(
                        device.device_name_short + DEVICE_PUMP
                    ).set_data(device_status_ages, device_pump)
                if device_fan:
                    self._get_line_by_label(
                        device.device_name_short + DEVICE_FAN
                    ).set_data(device_status_ages, device_fan)

    def _get_first_device_with_name(self, device_name: str) -> Optional[DeviceStatus]:
        return next(
            (device for device in self._devices_statuses if device.device_name == device_name),
            None
        )

    def _get_liquidctl_devices(self) -> List[DeviceStatus]:
        return [device_status for device_status in self._devices_statuses if device_status.lc_device]

    def _initialize_cpu_lines(self) -> None:
        lines_cpu = [
            Line2D([], [], color=CPU_COLOR, label=CPU_TEMP, linewidth=2),
            Line2D([], [], color=CPU_COLOR, label=CPU_LOAD, linestyle='dashed', linewidth=1)
        ]
        self.lines.extend(lines_cpu)
        for line in lines_cpu:
            self.axes.add_line(line)
        self._cpu_lines_initialized = True
        self._redraw_whole_canvas()
        _LOG.debug('initialized cpu lines')

    def _initialize_gpu_lines(self) -> None:
        lines_gpu = [
            Line2D([], [], color=GPU_COLOR, label=GPU_TEMP, linewidth=2),
            Line2D([], [], color=GPU_COLOR, label=GPU_LOAD, linestyle='dashed', linewidth=1)
        ]
        self.lines.extend(lines_gpu)
        for line in lines_gpu:
            self.axes.add_line(line)
        self._gpu_lines_initialized = True
        self._redraw_whole_canvas()
        _LOG.debug('initialized gpu lines')

    def _initialize_liquidctl_lines(self, devices: List[DeviceStatus]) -> None:
        for device in devices:
            lines_liquidctl = []
            if device.status.device_temperature:
                lines_liquidctl.append(
                    # todo: device line colors based on cycle of colors.
                    Line2D([], [], color='blue',
                           label=device.device_name_short + DEVICE_TEMP,
                           linewidth=2))
            if device.status.liquid_temperature:
                lines_liquidctl.append(
                    Line2D([], [], color='blue',
                           label=device.device_name_short + DEVICE_LIQUID_TEMP,
                           linewidth=2))
            if device.status.pump_duty:
                lines_liquidctl.append(
                    Line2D([], [], color='blue',
                           label=device.device_name_short + DEVICE_PUMP,
                           linestyle='dashed', linewidth=1))
            if device.status.fan_duty:
                lines_liquidctl.append(
                    Line2D([], [], color='blue',
                           label=device.device_name_short + DEVICE_FAN,
                           linestyle='dashdot', linewidth=1))
            self.lines.extend(lines_liquidctl)
            for line in lines_liquidctl:
                self.axes.add_line(line)
        self._liquidctl_lines_initialized = True
        self._redraw_whole_canvas()
        _LOG.debug('initialized liquidctl lines')

    def _redraw_whole_canvas(self) -> None:
        self.legend = self.init_legend(self.bg_color, self.text_color)
        self._blit_cache.clear()
        self._init_draw()
        self.draw()

    def _get_line_by_label(self, label: str) -> Line2D:
        return next(line for line in self.lines if line.get_label() == label)
