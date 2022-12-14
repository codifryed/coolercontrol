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

import io
import logging
from pathlib import Path
from typing import Generator

from PIL import Image, ImageOps, ImageSequence
from PIL.ImageSequence import Iterator
from PySide6.QtCore import Qt, Signal, SignalInstance, QObject, QSize, QEvent
from PySide6.QtGui import QPixmap, QMovie, QPainter, QPainterPath
from PySide6.QtWidgets import QFileDialog, QPushButton

from coolercontrol.dialogs.dialog_style import DIALOG_STYLE
from coolercontrol.settings import Settings
from coolercontrol.view.core.functions import Functions

log = logging.getLogger(__name__)
_WH: int = 320  # the Width and Height of our LCD screen resolution
_LCD_TOTAL_MEMORY_KB: int = 24_320


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
        self.image_path_processed: Path | None = None
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

    def select_image(self) -> None:
        starting_dir = Path.home() if self.image_path is None else self.image_path
        dialog = QFileDialog(self, caption="Choose LCD Image", directory=str(starting_dir))
        dialog.setFileMode(QFileDialog.ExistingFile)
        # dialog.setStyleSheet(self._dialog_style_sheet)
        # dialog.setOption(QFileDialog.DontUseNativeDialog, True)
        dialog.setNameFilter("Image Files (*.png *.jpg *.jpeg *.tiff *.bmp *.gif);;All Files (*.*)")
        if dialog.exec():
            chosen_files: list[str] = dialog.selectedFiles()
            for file in chosen_files:
                file_path: Path = Path(file)
                if not file_path.is_file():
                    log.debug("No Image File chosen: %s", file_path)
                    return
                log.debug("Image File chosen: %s", file_path)
                try:
                    file_path = file_path.resolve(strict=True)
                    image: Image = Image.open(file_path)
                    image.verify()  # we verify a valid image file before processing
                    image.close()
                    self.set_image(file_path)
                except BaseException as exc:
                    log.error("Could not verify file as usable Image: %s", exc)
                break  # we only select one

    def set_image(self, image_path: Path | None) -> None:
        self.setIconSize(QSize(_WH, _WH))
        if self.gif_movie is not None:
            self.gif_movie.stop()
            self.gif_movie = None
        if image_path is None:
            self.image_path = None
            self.image_path_processed = None
            self.setIcon(QPixmap(self.default_image_file))
            self.image_changed.emit(None)
        else:
            self.process_and_save_image(image_path)

    def process_and_save_image(self, image_path: Path) -> None:
        """process the image so that liquidctl device time is minimized and make it how we want it."""
        try:
            image: Image = Image.open(image_path)
            if image.format is not None and image.format == "GIF":
                frames: Iterator = ImageSequence.Iterator(image)
                frames = self._resize_frames(frames)
                starting_image = next(frames)
                starting_image.info = image.info
                self.image_path_processed = Settings.tmp_path.joinpath("lcd_image.gif")
                starting_image.save(
                    self.image_path_processed, format="GIF", save_all=True, append_images=list(frames), loop=0,
                )
                processed_image: Image = Image.open(self.image_path_processed)
                image_bytes = io.BytesIO()
                processed_image.save(
                    image_bytes, format="GIF", save_all=True, loop=0,
                )
                self._verify_image_size(image_bytes.getvalue())  # to_bytes() on gif files only gives first frame bytes
                processed_image.close()
                self.image_path = image_path
                self.gif_movie = QMovie(str(self.image_path_processed))
                self.gif_movie.frameChanged.connect(
                    lambda: self.setIcon(self._convert_to_circular_pixmap(self.gif_movie.currentPixmap()))
                )
                self.gif_movie.start()
            else:
                image = ImageOps.fit(image, (_WH, _WH))  # fit() resizes and crops, keeping aspect ratio
                self._verify_image_size(image.tobytes())
                self.image_path_processed = Settings.tmp_path.joinpath("lcd_image.png")
                image.save(self.image_path_processed)
                self.image_path = image_path
                self.setIcon(
                    self._convert_to_circular_pixmap(image.toqpixmap()))
            image.close()
            self.image_changed.emit(image_path)
        except BaseException as exc:
            log.error("Image could not be loaded: %s", exc)
            self.set_image(None)  # reset image

    @staticmethod
    def _resize_frames(frames: Iterator) -> Generator[Iterator, None, None]:
        for frame in frames:
            resized = frame.copy()
            yield ImageOps.fit(resized, (_WH, _WH))

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

    @staticmethod
    def _verify_image_size(image_bytes: bytes) -> None:
        """Verify the in-memory data size to make sure it'll fit in LCD memory"""
        if len(image_bytes) / 1_000 >= _LCD_TOTAL_MEMORY_KB:
            raise ValueError(
                f"Image file after processing must be less than 24MB. Current size: "
                f"{round((len(image_bytes) / 1_000_000), 2)}MB"
            )

    def mousePressEvent(self, event: QEvent) -> None:
        if event.button() == Qt.RightButton:
            self.set_image(None)
        else:
            return super().mousePressEvent(event)
