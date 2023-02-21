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

import logging
from collections import defaultdict
from dataclasses import dataclass, field
from datetime import datetime
from operator import attrgetter

from matplotlib.animation import FuncAnimation
from matplotlib.artist import Artist
from matplotlib.backend_bases import PickEvent, DrawEvent, MouseEvent, MouseButton
from matplotlib.backends.backend_qt5agg import FigureCanvasQTAgg
from matplotlib.figure import Figure
from matplotlib.legend import Legend
from matplotlib.lines import Line2D
from matplotlib.text import Text

from coolercontrol.models.device import Device, DeviceType
from coolercontrol.models.status import Status
from coolercontrol.repositories.daemon_repo import MAX_UPDATE_TIMESTAMP_VARIATION, DaemonRepo
from coolercontrol.services.settings_observer import SettingsObserver
from coolercontrol.settings import Settings
from coolercontrol.view_models.device_observer import DeviceObserver
from coolercontrol.view_models.device_subject import DeviceSubject

log = logging.getLogger(__name__)
DRAW_INTERVAL_MS: int = 1_000


class SystemOverviewCanvas(FigureCanvasQTAgg, FuncAnimation, DeviceObserver):
    """Class to plot and animate System Overview histogram"""

    _cpu_lines_initialized: bool = False
    _gpu_lines_initialized: bool = False
    _liquidctl_lines_initialized: bool = False
    _hwmon_lines_initialized: bool = False
    _composite_lines_initialized: bool = False

    def __init__(self,
                 width: int = 16,  # width/height ratio & inches for print
                 height: int = 9,
                 dpi: int = 120,
                 bg_color: str = Settings.theme['app_color']['bg_one'],
                 text_color: str = Settings.theme['app_color']['text_foreground'],
                 title_color: str = Settings.theme["app_color"]["text_title"]
                 ) -> None:
        self._bg_color = bg_color
        self._text_color = text_color
        self._settings_observer = SettingsObserver()
        self._settings_observer.connect_clear_graph_history(self.clear_cached_graph_data)
        self._devices: list[Device] = []
        self._cpu_data: dict[Device, DeviceData] = {}
        self._gpu_data: dict[Device, DeviceData] = {}
        self._lc_devices_data: dict[Device, DeviceData] = {}
        self._hwmon_devices_data: dict[Device, DeviceData] = {}
        self._composite_data: DeviceData | None = None
        self.x_limit: int = 60  # the age, in seconds, of data to display:

        # Setup
        self.fig = Figure(figsize=(width, height), dpi=dpi, layout='tight', facecolor=bg_color, edgecolor=text_color)
        self.axes = self.fig.add_subplot(111, facecolor=bg_color)
        self.legend: Legend | None = None
        self.axes.set_ylim(-1, 101)
        self.axes.set_xlim(self.x_limit, 0)  # could make this modifiable to scaling & zoom
        self._drawn_artists: list[Artist] = []  # used by the matplotlib implementation for blit animation

        # Grid
        self.axes.grid(True, linestyle='dotted', color=text_color, alpha=0.5)
        self.axes.margins(x=0, y=0.05)
        self.axes.tick_params(colors=text_color)
        self.axes.set_yticks(
            [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100],
            ['0°/%', '10°/%', '20°/%', '30°/%', '40°/%', '50°/%', '60°/%', '70°/%', '80°/%', '90°/%', '100°/%', ])
        self.axes.set_xticks(
            [30, 60],
            ['30s', '1m'])
        self.axes.spines['top'].set_edgecolor(bg_color)
        self.axes.spines['right'].set_edgecolor(bg_color)
        self.axes.spines['right'].set_animated(True)
        self.axes.spines[['bottom', 'left']].set_edgecolor(text_color)

        # Lines
        self.lines: list[Line2D] = []
        self.legend_artists: dict[Artist, Line2D] = {}

        # Interactions
        self.fig.canvas.mpl_connect('pick_event', self._on_pick)
        self.fig.canvas.mpl_connect('button_press_event', self._on_mouse_click_scroll)
        self.fig.canvas.mpl_connect('scroll_event', self._on_mouse_click_scroll)
        self.zoom_level: int = 1

        # Initialize
        FigureCanvasQTAgg.__init__(self, self.fig)
        FuncAnimation.__init__(self, self.fig, func=self.draw_frame, interval=DRAW_INTERVAL_MS, blit=True, cache_frame_data=False)

    def draw_frame(self, frame: int) -> list[Artist]:
        """Is used to draw every frame of the chart animation"""
        self._set_cpu_data()
        self._set_gpu_data()
        self._set_lc_device_data()
        self._set_hwmon_device_data()
        self._set_composite_data()
        self.verify_data_lengths()
        self._drawn_artists = list(self.lines)  # pylint: disable=attribute-defined-outside-init
        self._drawn_artists.append(self.axes.spines['right'])
        if self.legend is not None:
            self._drawn_artists.append(self.legend)
        self.event_source.interval = DRAW_INTERVAL_MS  # return to normal speed after first frame
        return self._drawn_artists

    def notify_me(self, subject: DeviceSubject) -> None:  # type: ignore
        if self._devices:
            return
        self._devices = subject.devices
        if cpus := self._get_devices_with_type(DeviceType.CPU):
            self._initialize_cpu_lines(cpus)
        if gpus := self._get_devices_with_type(DeviceType.GPU):
            self._initialize_gpu_lines(gpus)
        if devices := self._get_devices_with_type(DeviceType.LIQUIDCTL):
            self._initialize_liquidctl_lines(devices)
        if hwmon_devices := self._get_devices_with_type(DeviceType.HWMON):
            self._initialize_hwmon_lines(hwmon_devices)
        if composite_device := self._get_first_device_with_type(DeviceType.COMPOSITE):
            self._initialize_composite_lines(composite_device)
        self.legend = self.init_legend(self._bg_color, self._text_color)
        self._redraw_canvas()

    def init_legend(self, bg_color: str, text_color: str) -> Legend:
        legend = self.axes.legend(loc='upper left', facecolor=bg_color, edgecolor=text_color, framealpha=0.9)
        legend.set_animated(True)
        for legend_line, legend_text, ax_line in zip(legend.get_lines(), legend.get_texts(), self.lines):
            is_visible: bool = Settings.is_overview_line_visible(legend_text.get_text())
            alpha: float = 1.0 if is_visible else 0.2
            legend_line.set_picker(True)
            legend_line.set_pickradius(7)
            legend_line.set_alpha(alpha)
            legend_text.set_color(text_color)
            legend_text.set_picker(True)
            legend_text.set_alpha(alpha)
            ax_line.set_visible(is_visible)
            self.legend_artists[legend_line] = ax_line
            self.legend_artists[legend_text] = ax_line
        return legend

    def redraw_workaround(self) -> None:
        """In some situations artifacts appear from hiding and showing the graph, in this case we manually clear"""
        self._redraw_canvas()

    def clear_cached_graph_data(self) -> None:
        for data in self._cpu_data.values():
            data.clear_cached_data()
        for data in self._gpu_data.values():
            data.clear_cached_data()
        for data in self._lc_devices_data.values():
            data.clear_cached_data()
        if self._composite_data is not None:
            self._composite_data.clear_cached_data()

    def _set_cpu_data(self) -> None:
        if not self._cpu_lines_initialized:
            return
        cpus = self._get_devices_with_type(DeviceType.CPU)
        for cpu in cpus:
            for name, temps in self._cpu_data[cpu].temps.items():
                self._get_line_by_label(
                    self._create_cpu_label(name, len(cpus), cpu.type_id)
                ).set_data(self._cpu_data[cpu].ages_seconds, temps)
            for name, duties in self._cpu_data[cpu].duties.items():
                self._get_line_by_label(
                    self._create_cpu_label(name, len(cpus), cpu.type_id)
                ).set_data(self._cpu_data[cpu].ages_seconds, duties)

    def _set_gpu_data(self) -> None:
        if not self._gpu_lines_initialized:
            return
        gpus = self._get_devices_with_type(DeviceType.GPU)
        for gpu in gpus:
            for name, temps in self._gpu_data[gpu].temps.items():
                self._get_line_by_label(
                    self._create_gpu_label(name, len(gpus), gpu.type_id)
                ).set_data(self._gpu_data[gpu].ages_seconds, temps)
            for name, duties in self._gpu_data[gpu].duties.items():
                self._get_line_by_label(
                    self._create_gpu_label(name, len(gpus), gpu.type_id)
                ).set_data(self._gpu_data[gpu].ages_seconds, duties)

    def _set_lc_device_data(self) -> None:
        if not self._liquidctl_lines_initialized:
            return
        for device in self._get_devices_with_type(DeviceType.LIQUIDCTL):
            for name, temps in self._lc_devices_data[device].temps.items():
                self._get_line_by_label(
                    self._create_device_label(device.name_short, name, device.type_id)
                ).set_data(self._lc_devices_data[device].ages_seconds, temps)
            for name, duty in self._lc_devices_data[device].duties.items():
                self._get_line_by_label(
                    self._create_device_label(device.name_short, name, device.type_id)
                ).set_data(self._lc_devices_data[device].ages_seconds, duty)

    def _set_hwmon_device_data(self) -> None:
        if not self._hwmon_lines_initialized:
            return
        for device in self._get_devices_with_type(DeviceType.HWMON):
            for name, temps in self._hwmon_devices_data[device].temps.items():
                self._get_line_by_label(
                    self._create_device_label(device.name, name, device.type_id)
                ).set_data(self._hwmon_devices_data[device].ages_seconds, temps)
            for name, duty in self._hwmon_devices_data[device].duties.items():
                self._get_line_by_label(
                    self._create_device_label(device.name, name, device.type_id)
                ).set_data(self._hwmon_devices_data[device].ages_seconds, duty)

    def _set_composite_data(self) -> None:
        composite_device = self._get_first_device_with_type(DeviceType.COMPOSITE)
        if self._composite_lines_initialized and composite_device:
            for name, temps in self._composite_data.temps.items():
                self._get_line_by_label(name).set_data(self._composite_data.ages_seconds, temps)

    def verify_data_lengths(self):
        """
        Verify that all lines have the same data length and attempts to correct the issue if possible.
        This is helpful for correcting status update issues and keeping the graph working, instead of raising an exception
        """
        if not self.lines:
            return
        x_length = len(self.lines[0].get_xdata(orig=True))  # we assume cpu is the most stable of status_history
        warning_logged: bool = False
        for line in self.lines:
            line_length = len(line.get_xdata(orig=True))
            if line_length != x_length:
                if not warning_logged:
                    log.warning("There are unequal status history lengths for system overview lines. Compensating and clearing cache.")
                    self.clear_cached_graph_data()
                    DaemonRepo.reload_all_statuses = True
                    warning_logged = True
                if line_length > x_length:
                    x_data = list(line.get_xdata(orig=True))[-x_length:]
                    y_data = list(line.get_ydata(orig=True))[-x_length:]
                else:  # length is smaller
                    x_data = list(line.get_xdata(orig=True))
                    y_data = list(line.get_ydata(orig=True))
                    for _ in range(x_length - line_length):
                        x_data.append(0)
                        y_data.append(0)
                line.set_data(x_data, y_data)

    def _get_first_device_with_type(self, device_type: DeviceType) -> Device | None:
        return next(
            iter(self._get_devices_with_type(device_type)),
            None
        )

    def _get_devices_with_type(self, device_type: DeviceType) -> list[Device]:
        return [device for device in self._devices if device.type == device_type]

    def _initialize_cpu_lines(self, cpus: list[Device]) -> None:
        lines_cpu: list[Line2D] = []
        for cpu in cpus:
            lines_cpu.extend(
                Line2D(
                    [], [], color=cpu.color(temp_status.name),
                    label=self._create_cpu_label(temp_status.name, len(cpus), cpu.type_id),
                    linewidth=2,
                )
                for temp_status in cpu.status.temps
            )
            lines_cpu.extend(
                Line2D(
                    [], [], color=cpu.color(channel_status.name),
                    label=self._create_cpu_label(channel_status.name, len(cpus), cpu.type_id),
                    linestyle=("dashdot" if channel_status.name.startswith("fan") else "dashed"),
                    linewidth=1,
                )
                for channel_status in cpu.status.channels
            )
            self._cpu_data[cpu] = DeviceData(cpu.status_history)
        self.lines.extend(lines_cpu)
        for line in lines_cpu:
            self.axes.add_line(line)
        self._cpu_lines_initialized = True
        log.debug('initialized cpu lines')

    def _initialize_gpu_lines(self, gpus: list[Device]) -> None:
        lines_gpu: list[Line2D] = []
        for gpu in gpus:
            lines_gpu.extend(
                Line2D(
                    [], [], color=gpu.color(temp_status.name),
                    label=self._create_gpu_label(temp_status.name, len(gpus), gpu.type_id),
                    linewidth=2
                )
                for temp_status in gpu.status.temps
            )
            lines_gpu.extend(
                Line2D(
                    [], [], color=gpu.color(channel_status.name),
                    label=self._create_gpu_label(channel_status.name, len(gpus), gpu.type_id),
                    linestyle=("dashdot" if channel_status.name.startswith("fan") else "dashed"),
                    linewidth=1,
                )
                for channel_status in sorted(gpu.status.channels, key=attrgetter("name"))
            )
            self._gpu_data[gpu] = DeviceData(gpu.status_history)
        self.lines.extend(lines_gpu)
        for line in lines_gpu:
            self.axes.add_line(line)
        self._gpu_lines_initialized = True
        log.debug('initialized gpu lines')

    def _initialize_liquidctl_lines(self, devices: list[Device]) -> None:
        for device in devices:
            if device.lc_driver_type is None:
                continue
            lines_liquidctl = [
                Line2D(
                    [], [], color=device.color(temp_status.name),
                    label=self._create_device_label(device.name_short, temp_status.name, device.type_id),
                    linewidth=2
                )
                for temp_status in sorted(device.status.temps, key=attrgetter("name"))
            ]
            for channel_status in sorted(device.status.channels, key=attrgetter("name")):
                if channel_status.duty is not None:
                    linestyle = 'dashdot' if channel_status.name.startswith('fan') else 'dashed'
                    lines_liquidctl.append(
                        Line2D(
                            [], [], color=device.color(channel_status.name),
                            label=self._create_device_label(
                                device.name_short, channel_status.name, device.type_id
                            ),
                            linestyle=linestyle, linewidth=1
                        )
                    )
            self.lines.extend(lines_liquidctl)
            for line in lines_liquidctl:
                self.axes.add_line(line)
            self._lc_devices_data[device] = DeviceData(device.status_history)
        self._liquidctl_lines_initialized = True
        log.debug('initialized liquidctl lines')

    def _initialize_hwmon_lines(self, hwmon_devices: list[Device]) -> None:
        lines_hwmon: list[Line2D] = []
        for device in hwmon_devices:
            lines_hwmon.extend(
                Line2D(
                    [], [], color=device.color(temp_status.name),
                    label=self._create_device_label(device.name, temp_status.name, device.type_id),
                    linewidth=2
                )
                for temp_status in sorted(device.status.temps, key=attrgetter("name"))
            )
            for channel_status in sorted(device.status.channels, key=attrgetter("name")):
                linestyle = 'dashdot' if channel_status.name.startswith('fan') else 'dashed'
                lines_hwmon.append(
                    Line2D(
                        [], [], color=device.color(channel_status.name),
                        label=self._create_device_label(device.name, channel_status.name, device.type_id),
                        linestyle=linestyle, linewidth=1
                    )
                )
            self._hwmon_devices_data[device] = DeviceData(device.status_history)
        self.lines.extend(lines_hwmon)
        for line in lines_hwmon:
            self.axes.add_line(line)
        self._hwmon_lines_initialized = True
        log.debug('initialized hwmon lines')

    def _initialize_composite_lines(self, composite_device: Device) -> None:
        lines_composite = [
            Line2D([], [], color=composite_device.color(temp_status.name), label=temp_status.name, linewidth=2)
            for temp_status in sorted(composite_device.status.temps, key=attrgetter("name"))
        ]
        self._composite_data = DeviceData(composite_device.status_history)
        self.lines.extend(lines_composite)
        for line in lines_composite:
            self.axes.add_line(line)
        self._composite_lines_initialized = True
        log.debug('initialized composite lines')

    @staticmethod
    def _create_cpu_label(channel_name: str, number_cpus: int, current_cpu_id: int) -> str:
        prefix = f"#{current_cpu_id} " if number_cpus > 1 else ""
        return f'{prefix}{channel_name}' if channel_name.startswith("CPU") else f"{prefix}CPU {channel_name.capitalize()}"

    @staticmethod
    def _create_gpu_label(channel_name: str, number_gpus: int, current_gpu_id: int) -> str:
        prefix = f"#{current_gpu_id} " if number_gpus > 1 else ""
        return f'{prefix}{channel_name}' if channel_name.startswith("GPU") else f"{prefix}GPU {channel_name.capitalize()}"

    def _create_device_label(self, device_name: str, channel_name: str, device_id: int) -> str:
        has_same_name_as_other_device: bool = any(
            device.name_short == device_name and device.type_id != device_id
            for device in self._devices
        )
        prefix = f'LC#{device_id} ' if has_same_name_as_other_device else ''
        return f'{prefix}{device_name} {channel_name.capitalize()}'

    def _redraw_canvas(self) -> None:
        self._blit_cache.clear()
        self.event_source.interval = 10
        self.draw()

    def _get_line_by_label(self, label: str) -> Line2D:
        try:
            return next(line for line in self.lines if line.get_label() == label)
        except StopIteration:
            log.error('No Initialized Plot Line found for label: %s', label)
            return Line2D([], [])

    def _on_pick(self, event: PickEvent) -> None:
        """hide/show specific lines from the legend"""
        if event.mouseevent.button != MouseButton.LEFT:
            return
        chosen_artist = event.artist
        ax_line = self.legend_artists.get(chosen_artist)
        if ax_line is None:
            log.error('Chosen artist in system overview legend was not found')
            return
        is_visible: bool = not ax_line.get_visible()
        ax_line.set_visible(is_visible)
        for artist in (artist for artist, line in self.legend_artists.items() if line == ax_line):
            artist.set_alpha(1.0 if is_visible else 0.2)
            if isinstance(artist, Text):
                Settings.overview_line_is_visible(artist.get_text(), is_visible)
        self.event_source.interval = 100

    def _on_mouse_click_scroll(self, event: MouseEvent) -> None:
        """Zoom action of the main graph"""
        if event.button == MouseButton.RIGHT:
            if self.zoom_level < 4:
                self.zoom_level += 1
            else:
                self.zoom_level = 1
        elif event.button == 'down':
            if self.zoom_level < 4:
                self.zoom_level += 1
            else:
                return
        elif event.button == 'up':
            if self.zoom_level > 1:
                self.zoom_level -= 1
            else:
                return
        else:
            return
        x_limit_in_seconds: int = 0
        if self.zoom_level == 1:
            x_limit_in_seconds = 60
            self.axes.set_xticks(
                [30, 60],
                ['30s', '1m'])
        elif self.zoom_level == 2:
            x_limit_in_seconds = 5 * 60
            self.axes.set_xticks(
                [60, 180, 300],
                ['1m', '3m', '5m'])
        elif self.zoom_level == 3:
            x_limit_in_seconds = 15 * 60
            self.axes.set_xticks(
                [60, 300, 600, 900],
                ['1m', '5m', '10m', '15m'])
        elif self.zoom_level == 4:
            x_limit_in_seconds = 30 * 60
            self.axes.set_xticks(
                [60, 300, 600, 900, 1200, 1800],
                ['1m', '5m', '10m', '15m', '20m', '30m'])
        self.axes.set_xlim(x_limit_in_seconds, 0)
        self.event_source.interval = 100
        self.draw()

    def _end_redraw(self, event: DrawEvent) -> None:
        """We override this so that our animation is redrawn quickly after a plot resize"""
        super()._end_redraw(event)
        self.event_source.interval = 100


@dataclass(frozen=True)
class DeviceData:
    """This class improves graph efficiency by storing a copy of data in the preferred format and only adding new data"""
    device_status_history: list[Status]
    _temps: dict[str, list[float]] = field(default_factory=lambda: defaultdict(list), init=False)
    _duties: dict[str, list[float]] = field(default_factory=lambda: defaultdict(list), init=False)
    _ages_seconds: list[int] = field(default_factory=list, init=False)
    _ages_timestamps: list[datetime] = field(default_factory=list, init=False)

    @property
    def temps(self) -> dict[str, list[float]]:
        self._synchronize_data()
        return self._temps

    @property
    def duties(self) -> dict[str, list[float]]:
        self._synchronize_data()
        return self._duties

    @property
    def ages_seconds(self) -> list[int]:
        return self._ages_seconds

    def clear_cached_data(self) -> None:
        self._temps.clear()
        self._duties.clear()
        self._ages_seconds.clear()
        self._ages_timestamps.clear()

    def _synchronize_data(self) -> None:
        self._remove_outdated_data()
        current_data_size = len(self._ages_seconds)
        statuses_to_sync = len(self.device_status_history) - current_data_size
        if statuses_to_sync > 0:
            for status in self.device_status_history[-statuses_to_sync:]:
                for temp_status in status.temps:
                    self._temps[temp_status.name].append(temp_status.temp)
                for channel_status in status.channels:
                    if channel_status.duty is not None:
                        self._duties[channel_status.name].append(channel_status.duty)
            self._ages_seconds.clear()
            self._ages_timestamps.clear()
            most_recent_timestamp = self.device_status_history[-1].timestamp + MAX_UPDATE_TIMESTAMP_VARIATION
            for status in self.device_status_history:
                self._ages_seconds.append((most_recent_timestamp - status.timestamp).seconds)
                self._ages_timestamps.append(status.timestamp)

    def _remove_outdated_data(self) -> None:
        """This removes stored data that has been removed from the status_history"""
        if not self._ages_seconds or not self.device_status_history:
            return
        while self.device_status_history[0].timestamp != self._ages_timestamps[0]:
            self._ages_timestamps.pop(0)
            self._ages_seconds.pop(0)
            for temp in self._temps.values():
                temp.pop(0)
            for duty in self._duties.values():
                duty.pop(0)
