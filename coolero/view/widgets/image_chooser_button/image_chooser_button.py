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
from typing import Generator

from PIL import Image, ImageOps, ImageSequence, ImageDraw
from PIL.ImageSequence import Iterator
from PySide6.QtCore import Qt, Signal, SignalInstance, QObject, QSize, QEvent
from PySide6.QtGui import QPixmap, QMovie, QPainter, QPainterPath
from PySide6.QtWidgets import QFileDialog, QPushButton

from coolero.dialogs.dialog_style import DIALOG_STYLE
from coolero.settings import Settings
from coolero.view.core.functions import Functions

_LOG = logging.getLogger(__name__)
_WH: int = 320  # the Width and Height of our LCD screen resolution


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
                 radius: int = 177,  # this allows us to keep a circle even when the window is really small
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
        self.tmp_image_path: Path | None = None
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
        self.setIconSize(QSize(_WH, _WH))
        if self.gif_movie is not None:
            self.gif_movie.stop()
            self.gif_movie = None
        if image_path is None:
            self.image_path = image_path
            self.setIcon(QPixmap(self.default_image_file))
            self.image_changed.emit(image_path)
        else:
            try:
                with Image.open(image_path) as image:
                    # preprocess the image so that liquidctl device time is minimized and make it how we want it.
                    if image.format is not None and image.format == "GIF":
                        frames: Iterator = ImageSequence.Iterator(image)
                        frames = self._resize_frames(frames)
                        starting_image = next(frames)
                        starting_image.info = image.info
                        self.tmp_image_path = Settings.tmp_path.joinpath("lcd_image.gif")
                        starting_image.save(
                            self.tmp_image_path, format="GIF", save_all=True, append_images=list(frames), loop=0,
                        )
                        self.image_path = image_path
                        self.gif_movie = QMovie(str(self.tmp_image_path))
                        self.gif_movie.frameChanged.connect(
                            lambda: self.setIcon(self._convert_to_circular_pixmap(self.gif_movie.currentPixmap()))
                        )
                        self.gif_movie.start()
                    else:
                        image = ImageOps.fit(image, (_WH, _WH))  # resizes and crops, keeping aspect ratio
                        self.tmp_image_path = Settings.tmp_path.joinpath("lcd_image.png")
                        image.save(self.tmp_image_path)
                        self.image_path = image_path
                        self.setIcon(
                            self._convert_to_circular_pixmap(image.toqpixmap()))
                    self.image_changed.emit(image_path)
            except BaseException as exc:
                _LOG.error("Image could not be loaded: %s", exc)
                self.set_image(None)  # reset image

    @staticmethod
    def _resize_frames(frames: Iterator) -> Generator[Iterator, None, None]:
        for frame in frames:
            resized = frame.copy()
            yield ImageOps.fit(resized, (320, 320))

    @staticmethod
    def _convert_to_circular_pixmap(source_pixmap: QPixmap) -> QPixmap:
        target_pixmap = QPixmap(QSize(_WH, _WH))
        target_pixmap.fill(Qt.transparent)
        painter = QPainter(target_pixmap)
        painter.setRenderHint(QPainter.Antialiasing, True)
        painter.setRenderHint(QPainter.SmoothPixmapTransform, True)
        path = QPainterPath()
        path.addRoundedRect(0, 0, _WH, _WH, (_WH / 2), (_WH / 2))
        painter.setClipPath(path)
        painter.drawPixmap(0, 0, source_pixmap)  # draws onto the target pixmap with clipPath
        painter.end()
        del painter
        return target_pixmap

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
