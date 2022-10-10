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

import logging
from pathlib import Path

from PIL import Image
from PySide6.QtCore import Qt, Signal, SignalInstance, QObject, QSize, QEvent
from PySide6.QtGui import QPixmap, QMovie
from PySide6.QtWidgets import QFileDialog, QPushButton

from coolero.dialogs.dialog_style import DIALOG_STYLE
from coolero.settings import Settings
from coolero.view.core.functions import Functions

_LOG = logging.getLogger(__name__)


class ImageChooserButton(QPushButton):
    _style = """
    QPushButton {{
        border: none;
        padding-top: 20px;
        padding-bottom: 20px;
        padding-left: 20px;
        padding-right: 20px;
        color: {_color};
        border-radius: {_radius};
        background-color: {_bg_color};
    }}
    QPushButton:hover {{
        background-color: {_bg_color_hover};
    }}
    QPushButton:pressed {{
        background-color: {_bg_color_pressed};
    }}
    """
    image_changed: SignalInstance = Signal(object)  # type: ignore

    def __init__(self,
                 color: str,
                 bg_color: str,
                 bg_color_hover: str,
                 bg_color_pressed: str,
                 image: Path | None = None,
                 radius: int = 8,
                 parent: QObject | None = None,
                 ) -> None:
        super().__init__()
        if parent is not None:
            self.setParent(parent)
        self.setMinimumHeight(360)
        self.setMinimumWidth(360)
        self.setCursor(Qt.PointingHandCursor)
        self.setStyleSheet(self._style.format(
            _color=color,
            _radius=radius,
            _bg_color=bg_color,
            _bg_color_hover=bg_color_hover,
            _bg_color_pressed=bg_color_pressed
        ))
        self.image_path: Path | None = image
        self.gif_movie: QMovie | None = None
        self.default_image_file: str = Functions.set_image("image_file_320.png")
        self._dialog_style_sheet = DIALOG_STYLE.format(
            _text_size=Settings.app["font"]["text_size"],
            _font_family=Settings.app["font"]["family"],
            _text_color=Settings.theme["app_color"]["text_foreground"],
            _bg_color=Settings.theme["app_color"]["bg_one"]
        )
        self.pressed.connect(self.select_image)
        self.set_image(self.image_path)

    def set_image(self, image_path: Path | None) -> None:
        self.setIconSize(QSize(320, 320))
        if self.gif_movie is not None:
            self.gif_movie.stop()
            self.gif_movie = None
        if image_path is None:
            self.image_path = image_path
            self.setIcon(QPixmap(self.default_image_file))
            self.image_changed.emit(image_path)
        else:
            try:
                image = Image.open(image_path)
                if image.format is not None and image.format == "GIF":
                    image.close()
                    self.image_path = image_path
                    self.gif_movie = QMovie(str(image_path))
                    self.gif_movie.frameChanged.connect(lambda: self.setIcon(self.gif_movie.currentPixmap()))
                    self.gif_movie.start()
                else:
                    image.resize((320, 320))
                    self.image_path = image_path
                    self.setIcon(image.toqpixmap())
                self.image_changed.emit(image_path)
            except BaseException as exc:
                _LOG.error("Image could not be loaded: %s", exc)
                self.set_image(None)  # reset image

    def select_image(self) -> None:
        starting_dir = Path.home() if self.image_path is None else self.image_path
        dialog = QFileDialog(self, caption="Choose LCD Image", directory=str(starting_dir))
        dialog.setFileMode(QFileDialog.ExistingFile)
        # dialog.setStyleSheet(self._dialog_style_sheet)
        # dialog.setOption(QFileDialog.DontUseNativeDialog, True)
        dialog.setNameFilter("Image Files (*.png *.jpg *.jpeg *.tiff *.bmp *.gif);;All Files (*.*)")
        if dialog.exec():
            chosen_files = dialog.selectedFiles()
            for file in chosen_files:
                file_path = Path(file)
                if not file_path.is_file():
                    _LOG.debug("No Image File chosen: %s", file_path)
                    return
                _LOG.debug("Image File chosen: %s", file_path)
                try:
                    file_path = file_path.resolve(strict=True)
                    img = Image.open(file_path)
                    img.verify()
                    img.close()
                    self.set_image(file_path)
                except BaseException as exc:
                    _LOG.error("Could not verify file as usable Image: %s", exc)
                break  # we only select one

    def mousePressEvent(self, event: QEvent) -> None:
        if event.button() == Qt.RightButton:
            self.set_image(None)
        else:
            return super().mousePressEvent(event)
