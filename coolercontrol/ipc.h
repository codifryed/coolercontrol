// CoolerControl - monitor and control your cooling and other devices
// Copyright (c) 2021-2025  Guy Boldon and contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

#ifndef IPC_H
#define IPC_H

#include <QSettings>

#include "main_window.h"

// forward declaration:
class MainWindow;

/*
    An instance of this class gets published over the WebChannel and is then accessible to HTML
   clients.
*/
class IPC final : public QObject {
  Q_OBJECT

 public:
  explicit IPC(QObject* parent = nullptr);

  Q_INVOKABLE [[nodiscard]] bool getStartInTray() const;

  Q_INVOKABLE [[nodiscard]] int getStartupDelay() const;

  Q_INVOKABLE [[nodiscard]] bool getCloseToTray() const;

  Q_INVOKABLE [[nodiscard]] bool getIsFullScreen() const;

  Q_INVOKABLE [[nodiscard]] double getZoomFactor() const;

  Q_INVOKABLE [[nodiscard]] QByteArray getWindowGeometry() const;

  Q_INVOKABLE [[nodiscard]] QString filePathDialog(const QString& title) const;

  Q_INVOKABLE [[nodiscard]] QString directoryPathDialog(const QString& title) const;

  /*
      Slots are invoked from the JS client side and are processed on the server side.
  */
 public slots:
  void setStartInTray(bool startInTray) const;

  void setStartupDelay(int startupDelay) const;

  void setCloseToTray(bool closeToTray) const;

  void setZoomFactor(double zoomFactor) const;

  void setModes(const QString& modesJson) const;

  void saveWindowGeometry(const QByteArray& geometry) const;

  void acknowledgeDaemonIssues() const;

  void forceQuit() const;

  void syncSettings() const;

  void loadFinished() const { emit webLoadFinished(); }

  /*
      Signals are emitted from the C++ side and are handed to callbacks on the JS client side.
  */
 signals:
  void sendText(const QString& text);

  void webLoadFinished() const;

  void fullScreenToggled(bool fullScreen) const;

 private:
  QSettings* m_settings;
  MainWindow* m_mainWindow;
};

#endif  // IPC_H
