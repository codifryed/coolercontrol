// SPDX-FileCopyrightText: 2025 Guy Boldon and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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

  // The desktop's own colors as JSON, or empty when it exposes none. Pulled once on
  // load; later changes arrive through systemPaletteChanged.
  Q_INVOKABLE [[nodiscard]] QString getSystemPalette() const;

  /*
      Slots are invoked from the JS client side and are processed on the server side.
  */
 public slots:
  void setStartInTray(bool startInTray) const;

  void setStartupDelay(int startupDelay) const;

  void setCloseToTray(bool closeToTray) const;

  void setZoomFactor(double zoomFactor) const;

  void setFullScreen(bool fullScreen) const;

  void setModes(const QString& modesJson) const;

  void saveWindowGeometry(const QByteArray& geometry) const;

  void acknowledgeDaemonIssues() const;

  void setAlertsActive(bool active) const;

  // JSON map of the strings Qt renders itself, in the UI's active locale. Qt has no
  // translation pipeline, and a discarded renderer cannot be asked, so these are cached.
  void setTranslations(const QString& translationsJson) const;

  // Identity and label of the sensors pinned in the UI. Values are not sent: Qt fetches
  // them when the tray menu opens, since the renderer is gone while in the tray.
  void setPinnedSensors(const QString& sensorsJson) const;

  void forceQuit() const;

  void forceRefresh() const;

  void syncSettings() const;

  void loadFinished() const { emit webLoadFinished(); }

  void forceShow() const { emit forceWindowShow(); }

  /*
      Signals are emitted from the C++ side and are handed to callbacks on the JS client side.
  */
 signals:
  void sendText(const QString& text);

  void webLoadFinished() const;

  void forceWindowShow() const;

  void fullScreenToggled(bool fullScreen) const;

  void systemPaletteChanged(const QString& paletteJson) const;

 private:
  QSettings* m_settings;
  MainWindow* m_mainWindow;
};

#endif  // IPC_H
