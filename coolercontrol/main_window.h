// SPDX-FileCopyrightText: 2025 Guy Boldon and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#ifndef MAINWINDOW_H
#define MAINWINDOW_H

#include <QCloseEvent>
#include <QElapsedTimer>
#include <QJsonArray>
#include <QMainWindow>
#include <QMenu>
#include <QNetworkAccessManager>
#include <QPainter>
#include <QSslCertificate>
#include <QSystemTrayIcon>
#include <QWebChannel>
#include <QWebEngineCertificateError>
#include <QWebEngineProfile>
#include <QWebEngineView>

#include "address_wizard.h"
#include "connections.h"
#include "ipc.h"
#include "origin_filter.h"

// forward declaration:
class IPC;

class MainWindow final : public QMainWindow {
  Q_OBJECT

 public:
  explicit MainWindow(QWidget* parent = nullptr);

  void handleStartInTray();

  // Escape hatch for --no-discard, so a bad interaction on some desktop has a
  // workaround that does not require a rebuild.
  void setDiscardEnabled(bool enabled);

  static void delay(int millisecondsWait);

  void setActiveMode(const QString& modeUID) const;

 public slots:
  void forceQuit();

  void forceRefresh() const;

  void reestablishDaemonConnection() const;

  void tryDaemonConnection();

  void startWatchingSSE() const;

  void setZoomFactor(double zoomFactor) const;

  void setTrayMenuModes(const QString& modesJson) const;

  void acknowledgeDaemonErrors() const;

  // Set by the UI over IPC: whether any enabled, active, unsilenced alert exists.
  void setAlertsActive(bool active) const;

  void setTranslations(const QString& translationsJson) const;

  void setPinnedSensors(const QString& sensorsJson) const;

 signals:
  void forceQuitSignal();

  void forceRefreshSignal();

  void daemonConnectionLost() const;

  void watchForSSE() const;

  void dropConnections() const;

  void setZoomFactorSignal(double zoomFactor) const;

  void setTrayMenuModesSignal(const QString& modesJson) const;

  void acknowledgeDaemonErrorsSignal() const;

  void setAlertsActiveSignal(bool active) const;

  void setTranslationsSignal(const QString& translationsJson) const;

  void setPinnedSensorsSignal(const QString& sensorsJson) const;

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

 public:
  [[nodiscard]] QString systemPaletteJson() const { return m_systemPaletteJson; }

 private:
  QSystemTrayIcon* m_sysTrayIcon;
  QMenu* m_trayIconMenu;
  QMenu* m_modesTrayMenu;
  // Hidden while only one daemon is saved, which is nearly every install.
  QMenu* m_daemonsTrayMenu;
  QActionGroup* m_daemonsActionGroup{nullptr};
  // Last palette pushed to the UI, kept so a repeat read does not re-emit.
  QString m_systemPaletteJson;
  // One disabled row per pinned sensor. Rebuilt only when the pinned set changes, so
  // the menu layout stays stable for hosts that cache it over DBusMenu.
  struct TraySensor {
    QString deviceUid;
    QString channelName;
    QString label;
    // Unit of the speed value, as the UI read it from the label. Empty means rpm.
    QString unit;
    QAction* action;
  };

  mutable QList<TraySensor> m_traySensors;
  // Holds the rows once there are too many to sit in the main menu.
  QMenu* m_sensorsTrayMenu;
  // The pinned-sensor payload the current rows were built from.
  mutable QString m_builtSensorsJson;
  QAction* m_quitAction;
  QAction* m_addressAction;
  QAction* m_showAction;
  QWizard* m_wizard;
  IntroPage* m_introPage;
  AddressPage* m_addressPage;
  // Why the last attempt failed, shown by the wizard. Cleared on a good connection so
  // a hand-opened dialog never blames a problem that is already over.
  mutable QString m_lastConnectionError;
  QNetworkAccessManager* m_manager;
  OriginFilter* m_originFilter;
  QTimer* m_retryTimer;
  // Delays the discard so a quick hide/show toggle never pays a page reload.
  QTimer* m_discardTimer;
  QTimer* m_sensorPollTimer;
  mutable int m_sensorPollTicks{0};
  bool m_discardEnabled{true};
  // Set when the daemon reconnects while hidden. A discarded page reloads on its own
  // when reactivated, so the refresh is deferred to the next show instead of
  // resurrecting a renderer nobody is looking at.
  mutable bool m_reloadOnShow{false};
  mutable bool m_forceQuit{false};
  mutable bool m_startup{true};
  mutable bool m_webLoadFinished{false};
  mutable bool m_loginWindowShown{false};
  mutable bool m_uiLoadingStopped{false};
  // Whether the user has been told the connection dropped. Reconnection never depends
  // on this, only the notifications do, so they stay paired: no "restored" without a
  // "disconnected" first, and no second "disconnected" before a recovery.
  mutable bool m_disconnectNotified{false};
  // How long the daemon has been unreachable. Invalid while connected. Read only by a
  // health probe that just failed, never on its own, so it cannot race a recovery.
  mutable QElapsedTimer m_disconnectedFor;
  mutable int m_sseRetryDelayMs{0};
  // A health probe is outstanding. The retry timer fires every two seconds while the
  // request allows eight, so without this two probes can be in flight at once and both
  // succeed, each opening its own event stream and doubling every notification.
  mutable bool m_healthProbeInFlight{false};
  mutable bool m_changeAddress{false};
  mutable bool m_daemonHasErrors{false};
  mutable bool m_daemonHasWarnings{false};
  // Pushed from the UI over IPC; the UI is the source of truth for alert state.
  mutable bool m_uiAlertsActive{false};
  int m_uiLoadRetryCount{0};
  static constexpr int MAX_UI_LOAD_RETRIES = 3;

  // This is empty when there is currently no active mode:
  mutable QString m_activeModeUID{QString()};

  // Bearer token this app owns, so the tray keeps working without a live renderer.
  // Empty until provisioned from a valid session; cleared on 401.
  mutable QByteArray m_accessToken{QByteArray()};

  void initWizard();

  void initSystemTray();

  void initWebUI();

  void initDelay() const;

  static QUrl getDaemonUrl(bool forceHttp = false);

  static QUrl getEndpointUrl(const QString& endpoint, bool forceHttp = false);

  // Every request this app originates goes through here, so the tray keeps working
  // when the renderer (and with it the session cookie's owner) is gone.
  void applyAuth(QNetworkRequest& request) const;

  void loadAccessToken() const;

  // Mints a write-capable token off the current session. Write access is required
  // because the tray's Modes submenu POSTs /modes-active/{uid}.
  void provisionAccessToken() const;

  void clearAccessToken() const;

  // Removes a token this app previously owned but no longer holds. Session-only route,
  // so it runs from the provision path where the cookie is known good.
  void deleteAccessToken(const QString& tokenId) const;

  // One checkable row per saved daemon, the live one checked.
  void rebuildDaemonsTrayMenu();

  void refreshSystemPalette();

  // Saves the connection and, when it addresses a different daemon than the live one,
  // switches to it. Shared by the wizard's Apply and the tray's daemon entries.
  // By value, not by reference: the tray rows call this and it tears them down, so a
  // reference would point into the very action being destroyed.
  void applyDaemonConnection(connections::Connection connection);

  // Drops everything the previous daemon told us. Anything left behind here shows up as
  // the old machine's modes, sensors or badges under the new connection.
  void resetPerDaemonState();

  void displayAddressWizard() const;

  // Confirms the configured address answers as a CoolerControl daemon, then loads it.
  // Guards the two places an unverified address can first be rendered: startup, and
  // applying a new address from the wizard.
  void loadVerifiedDaemonUi();

  // Clears the screen and opens the address dialog after a refused certificate.
  void refuseDaemonCertificate(const QString& reason);

  /// The transport's own account of a failure, for the connection dialog.
  static QString describeReplyError(const QNetworkReply* reply);

  // Decides whether to trust a daemon certificate, prompting when trust on first use
  // requires it. `silent` suppresses the prompt for background requests, which must not
  // pop dialogs; those simply fail until a page load establishes the pin.
  bool confirmCertificate(const QString& host, int port, const QSslCertificate& leaf, bool silent);

  // Replaces the blanket ignoreSslErrors() every request used to do.
  void applyTlsPolicy(QNetworkReply* reply) const;

  // Rebuilds the pinned-sensor rows and refreshes their readings. Driven by the menu's
  // aboutToShow, so nothing is fetched while the menu is closed.
  void refreshTraySensors();

  void stopTraySensorPolling() const;

  // Rebuilds the rows and decides whether they sit inline or in a submenu.
  void buildTraySensorRows(const QJsonArray& sensors);

  // One bulk status request, applied to every row.
  void pollTraySensors() const;

  /// Settings key for the daemon currently configured, as "host:port".
  QString daemonSettingsKey() const;

  /// Drops the readings, keeping the rows. Values that cannot be refreshed must not
  /// keep being shown: after an outage they are old, and after an address change they
  /// belong to a different machine entirely.
  void clearTraySensorReadings() const;

  /// Reports the outage, if one has outlived the grace period. Call only from a failed
  /// health probe.
  void confirmDaemonLossIfOverdue() const;

  // Shows the window on the channel's own page. The route is resolved by the UI, which
  // owns the rule that a controllable fan belongs on Cooling and a custom sensor on its
  // editor; Qt only navigates.
  void openTraySensorPage(const QString& route);

  // Filled swatch matching the colour the Monitoring panel uses for this sensor.
  static QIcon sensorColorIcon(const QString& color);

  // One-time prompt on the first close, correcting the belief that closing the window
  // stops cooling control. Returns true to carry on quitting, false if the user chose
  // the tray.
  bool offerCloseToTray() const;

  // Tears down the renderer process while the window is in the tray. The page object
  // survives and reloads itself on reactivation.
  void discardPage() const;

  void restorePage() const;

  void setTrayActionToShow() const;

  void setTrayActionToHide() const;

  void requestDaemonErrors() const;

  void requestAllModes() const;

  void requestActiveMode() const;

  void watchDaemonEvents() const;

  void handleLogEvent(const QString& log) const;

  void handleModeEvent(const QString& data) const;

  void handleNotificationEvent(const QString& data) const;

  void showVersionMismatchDialog(const QString& daemonVersion) const;

  static void notifyDaemonConnectionError();

  static void notifyDaemonErrors();

  static void notifyDaemonDisconnected();

  static void notifyDaemonConnectionRestored();

  static QIcon createIconWithNotificationBadge(const QIcon& baseIcon, bool redColor);

  void applyTrayIconNotificationBadge(bool forceBadge = false) const;
};
#endif  // MAINWINDOW_H
