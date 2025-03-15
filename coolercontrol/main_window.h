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

#ifndef MAINWINDOW_H
#define MAINWINDOW_H

#include <QCloseEvent>
#include <QMainWindow>
#include <QMenu>
#include <QNetworkAccessManager>
#include <QSystemTrayIcon>
#include <QWebChannel>
#include <QWebEngineProfile>
#include <QWebEngineView>

#include "address_wizard.h"
#include "ipc.h"

// forward declaration:
class IPC;

class MainWindow final : public QMainWindow {
  Q_OBJECT

 public:
  explicit MainWindow(QWidget* parent = nullptr);

  void handleStartInTray();

  static void delay(int millisecondsWait);

  void setActiveMode(const QString& modeUID) const;

 public slots:
  void forceQuit();

  void reestablishDaemonConnection() const;

  void tryDaemonConnection() const;

  void startWatchingSSE() const;

  void setZoomFactor(double zoomFactor) const;

  void setTrayMenuModes(const QString& modesJson) const;

  void acknowledgeDaemonErrors() const;

 signals:
  void forceQuitSignal();

  void daemonConnectionLost() const;

  void watchForSSE() const;

  void dropConnections() const;

  void setZoomFactorSignal(double zoomFactor) const;

  void setTrayMenuModesSignal(const QString& modesJson) const;

  void acknowledgeDaemonErrorsSignal() const;

 protected:
  void closeEvent(QCloseEvent* event) override;

  void hideEvent(QHideEvent* event) override;

  void showEvent(QShowEvent* event) override;

 private:
  QWebEngineView* m_view;
  QWebEngineProfile* m_profile;
  QWebEnginePage* m_page;
  QWebChannel* m_channel;
  IPC* m_ipc;
  QSystemTrayIcon* m_sysTrayIcon;
  QMenu* m_trayIconMenu;
  QMenu* m_modesTrayMenu;
  QAction* m_quitAction;
  QAction* m_addressAction;
  QAction* m_showAction;
  QWizard* m_wizard;
  QNetworkAccessManager* m_manager;
  QTimer* m_retryTimer;
  mutable bool m_forceQuit{false};
  mutable bool m_startup{true};
  mutable bool m_uiLoadingStopped{false};
  mutable bool m_changeAddress{false};
  mutable bool m_daemonHasErrors{false};

  // This is empty when there is currently no active mode:
  mutable QString m_activeModeUID{QString()};
  mutable QByteArray m_passwd{QByteArray()};

  void initWizard();

  void initSystemTray();

  void initWebUI();

  void initDelay() const;

  static QUrl getDaemonUrl();

  static QUrl getEndpointUrl(const QString& endpoint);

  void displayAddressWizard() const;

  void setTrayActionToShow() const;

  void setTrayActionToHide() const;

  void requestDaemonErrors() const;

  void requestAllModes() const;

  void requestActiveMode() const;

  void watchConnectionAndLogs() const;

  void watchModeActivation() const;

  void watchAlerts() const;

  void notifyDaemonConnectionError() const;

  void notifyDaemonErrors() const;

  void notifyDaemonDisconnected() const;

  void notifyDaemonConnectionRestored() const;
};
#endif  // MAINWINDOW_H
