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

#include "addresswizard.h"
#include "ipc.h"

class MainWindow final : public QMainWindow {
  Q_OBJECT

 public:
  explicit MainWindow(QWidget* parent = nullptr);

  void handleStartInTray();

  static void delay(int millisecondsWait);

  void setZoomFactor(double zoomFactor) const;

  void setTrayMenuModes(const QString& modesJson) const;

  void setActiveMode(const QString& modeUID) const;

 protected:
  void closeEvent(QCloseEvent* event) override;

  void hideEvent(QHideEvent* event) override;

  void showEvent(QShowEvent* event) override;

 private:
  QWebEngineView* view;
  QWebEngineProfile* profile;
  QWebEnginePage* page;
  QWebChannel* channel;
  IPC* ipc;
  bool forceQuit;
  QSystemTrayIcon* sysTrayIcon;
  QMenu* trayIconMenu;
  QMenu* modesTrayMenu;
  QAction* quitAction;
  QAction* addressAction;
  QAction* showAction;
  QWizard* wizard;
  QNetworkAccessManager* manager;
  mutable bool isDaemonConnected = false;

  mutable bool deamonHasErrors = false;

  // This is empty when there is currently no active mode:
  mutable QString activeModeUID = QString();

  static QUrl getDaemonUrl();

  static QUrl getEndpointUrl(const QString& endpoint);

  void displayAddressWizard() const;

  void setTrayActionToShow() const;

  void setTrayActionToHide() const;

  void requestDaemonErrors() const;

  void requestAllModes() const;

  void requestActiveMode() const;

  void watchLogsAndConnection() const;

  void verifyDaemonIsConnected() const;

  void watchModeActivation() const;

  void watchAlerts() const;

  void notifyDaemonConnectionError() const;

  void notifyDaemonErrors() const;

  void notifyDaemonDisconnected() const;

  void notifyDaemonConnectionRestored() const;
};
#endif  // MAINWINDOW_H
