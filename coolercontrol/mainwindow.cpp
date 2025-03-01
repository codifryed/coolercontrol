#include "mainwindow.h"

#include <QAction>
#include <QApplication>
#include <QDebug>
#include <QDesktopServices>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QMenu>
#include <QNetworkReply>
#include <QSettings>
#include <QStringBuilder>  // for % operator
#include <QSystemTrayIcon>
#include <QThread>
#include <QTimer>
#include <QWebEngineNewWindowRequest>
#include <QWebEngineSettings>
#include <QWebEngineView>
#include <QWizardPage>

#include "constants.h"

MainWindow::MainWindow(QWidget* parent)
    : QMainWindow(parent),
      m_view(new QWebEngineView(this)),
      m_profile(new QWebEngineProfile("coolercontrol", m_view)),
      m_page(new QWebEnginePage(m_profile)),
      m_channel(new QWebChannel(m_page)),
      m_ipc(new IPC(this)),
      m_wizard(new QWizard(this)),
      m_manager(new QNetworkAccessManager(this)) {
  // SETUP
  ////////////////////////////////////////////////////////////////////////////////////////////////
  setCentralWidget(m_view);
  m_profile->settings()->setAttribute(QWebEngineSettings::Accelerated2dCanvasEnabled, true);
  m_profile->settings()->setAttribute(QWebEngineSettings::ScreenCaptureEnabled, false);
  m_profile->settings()->setAttribute(QWebEngineSettings::PluginsEnabled, false);
  m_profile->settings()->setAttribute(QWebEngineSettings::PdfViewerEnabled, false);
  // local storage: ~/.local/share/{APP_NAME}
  m_profile->settings()->setAttribute(QWebEngineSettings::LocalStorageEnabled, true);
  m_profile->setPersistentCookiesPolicy(
      QWebEngineProfile::PersistentCookiesPolicy::ForcePersistentCookies);
  m_channel->registerObject("ipc", m_ipc);
  m_page->setWebChannel(m_channel);
  // This allows external links in our app to be opened by the external browser:
  connect(m_page, &QWebEnginePage::newWindowRequested, [](QWebEngineNewWindowRequest const& request) {
    QDesktopServices::openUrl(request.requestedUrl());
  });
  m_view->setPage(m_page);

  // Wizard init:
  m_wizard->setWindowTitle("Daemon Connection Error");
  m_wizard->setOption(QWizard::IndependentPages, true);
  m_wizard->setButtonText(QWizard::WizardButton::FinishButton, "&Apply");
  m_wizard->setOption(QWizard::CancelButtonOnLeft, true);
  m_wizard->setButtonText(QWizard::CustomButton1, "&Reset");
  m_wizard->setOption(QWizard::HaveCustomButton1, true);
  m_wizard->setButtonText(QWizard::HelpButton, "&Quit");
  m_wizard->setOption(QWizard::HaveHelpButton, true);
  m_wizard->addPage(new IntroPage);
  auto addressPage = new AddressPage;
  m_wizard->addPage(addressPage);
  m_wizard->setMinimumSize(640, 480);
  connect(m_wizard, &QWizard::helpRequested, []() { QApplication::quit(); });
  connect(m_wizard, &QWizard::customButtonClicked, [addressPage]([[maybe_unused]] const int which) {
    addressPage->resetAddressInputValues();
  });
  connect(m_wizard, &QDialog::accepted, [this]() {
    QSettings settings;
    settings.setValue(SETTING_DAEMON_ADDRESS, m_wizard->field("address").toString());
    settings.setValue(SETTING_DAEMON_PORT, m_wizard->field("port").toInt());
    settings.setValue(SETTING_DAEMON_SSL_ENABLED, m_wizard->field("ssl").toBool());
    m_view->load(getDaemonUrl());
  });

  // wait to fully initialize if there is a delay set:
  if (const auto startupDelay = m_ipc->getStartupDelay(); startupDelay > 0) {
    qInfo() << "Waiting for startup delay: " << startupDelay << "s";
    QThread::sleep(startupDelay);
  }

  // SYSTEM TRAY:
  ////////////////////////////////////////////////////////////////////////////////////////////////
  const auto ccHeader = new QAction(QIcon(":/icons/icon.png"), tr("CoolerControl"), this);
  ccHeader->setDisabled(true);
  connect(ccHeader, &QAction::triggered, [this]() {
    // Use CC Tray Header as show-only trigger. (un-minimize doesn't seem to work)
    show();
    activateWindow();
  });
  m_showAction =
      m_ipc->getStartInTray() ? new QAction(tr("&Show"), this) : new QAction(tr("&Hide"), this);
  connect(m_showAction, &QAction::triggered, [this]() {
    if (isVisible()) {
      hide();
    } else {
      show();
      activateWindow();
    }
  });

  m_addressAction = new QAction(tr("&Daemon Address"), this);
  connect(m_addressAction, &QAction::triggered, [this]() { displayAddressWizard(); });

  m_quitAction = new QAction(QIcon::fromTheme("application-exit", QIcon()), tr("&Quit"), this);
  connect(m_quitAction, &QAction::triggered, [this]() {
    m_forceQuit = true;
    // Triggers the close event but with the forceQuit flag set
    close();
  });
  m_trayIconMenu = new QMenu(this);
  m_trayIconMenu->setTitle("CoolerControl");
  m_trayIconMenu->addAction(ccHeader);
  m_trayIconMenu->addSeparator();
  m_modesTrayMenu = new QMenu(this);
  m_modesTrayMenu->setTitle("Modes");
  m_modesTrayMenu->setEnabled(false);
  m_trayIconMenu->addMenu(m_modesTrayMenu);
  m_trayIconMenu->addAction(m_showAction);
  m_trayIconMenu->addAction(m_addressAction);
  m_trayIconMenu->addSeparator();
  m_trayIconMenu->addAction(m_quitAction);

  m_sysTrayIcon = new QSystemTrayIcon(this);
  m_sysTrayIcon->setContextMenu(m_trayIconMenu);
  m_sysTrayIcon->setIcon(QIcon(":/icons/icon.ico"));
  m_sysTrayIcon->setToolTip("CoolerControl");
  m_sysTrayIcon->show();

  // left click:
  connect(m_sysTrayIcon, &QSystemTrayIcon::activated, [this](auto reason) {
    if (reason == QSystemTrayIcon::Trigger) {
      if (isVisible()) {
        hide();
      } else {
        show();
        activateWindow();
      }
    }
  });

  // LOAD UI:
  ////////////////////////////////////////////////////////////////////////////////////////////////
  m_view->load(getDaemonUrl());
  connect(m_view, &QWebEngineView::loadFinished, [this](const bool pageLoadedSuccessfully) {
    if (!pageLoadedSuccessfully) {
      displayAddressWizard();
      notifyDaemonConnectionError();
    } else {
      qInfo() << "Successfully connected to UI at: " << getDaemonUrl().url();
      m_isDaemonConnected = true;
      requestDaemonErrors();
      requestAllModes();
      requestActiveMode();
      watchLogsAndConnection();
      watchModeActivation();
      watchAlerts();
    }

    // todo: IPC command send from UI on login - to set password in Qt for Mode changes from
    // sysTray.
  });

  // emit ipc->sendText("Hello from C++");
  // todo: we can probably change the log download blob/link in the UI to point to an external link
  // to see the raw text api endpoint

  // todo: check for existing running CC application(single-instance)?
}

void MainWindow::closeEvent(QCloseEvent* event) {
  m_ipc->saveWindowGeometry(saveGeometry());
  if (m_ipc->getCloseToTray() && !m_forceQuit) {
    hide();
    event->ignore();
    return;
  }
  m_ipc->syncSettings();
  event->accept();
  QApplication::quit();
}

void MainWindow::hideEvent(QHideEvent* event) { setTrayActionToShow(); }

void MainWindow::showEvent(QShowEvent* event) { setTrayActionToHide(); }

QUrl MainWindow::getDaemonUrl() {
  const QSettings settings;
  const auto host =
      settings.value(SETTING_DAEMON_ADDRESS, DEFAULT_DAEMON_ADDRESS.data()).toString();
  const auto port = settings.value(SETTING_DAEMON_PORT, DEFAULT_DAEMON_PORT).toInt();
  const auto sslEnabled =
      settings.value(SETTING_DAEMON_SSL_ENABLED, DEFAULT_DAEMON_SSL_ENABLED).toBool();
  const auto schema = sslEnabled ? tr("https") : tr("http");
  QUrl url;
  url.setScheme(schema);
  url.setHost(host);
  url.setPort(port);
  return url;
}

QUrl MainWindow::getEndpointUrl(const QString& endpoint) {
  auto url = getDaemonUrl();
  url.setPath(endpoint);
  return url;
}

void MainWindow::displayAddressWizard() const {
  if (m_wizard->isVisible()) {
    return;
  }
  m_wizard->open();
}

void MainWindow::handleStartInTray() {
  restoreGeometry(m_ipc->getWindowGeometry());
  setZoomFactor(m_ipc->getZoomFactor());
  if (m_ipc->getStartInTray()) {
    hide();
    m_page->setLifecycleState(QWebEnginePage::LifecycleState::Frozen);
    // todo: can play with this more in the future. It's tricky but there is a possibility of even
    // less resource usage: page->setLifecycleState(QWebEnginePage::LifecycleState::Discarded);
  } else {
    show();
  }
}

void MainWindow::setZoomFactor(const double zoomFactor) const { m_view->setZoomFactor(zoomFactor); }

void MainWindow::delay(const int millisecondsWait) {
  QEventLoop loop;
  QTimer t;
  t.connect(&t, &QTimer::timeout, &loop, &QEventLoop::quit);
  t.start(millisecondsWait);
  loop.exec();
}

void MainWindow::setTrayActionToShow() const { m_showAction->setText(tr("&Show")); }

void MainWindow::setTrayActionToHide() const { m_showAction->setText(tr("&Hide")); }

void MainWindow::notifyDaemonConnectionError() const {
  m_sysTrayIcon->showMessage("Daemon Connection Error",
                           "Connection with the daemon could not be established",
                           QIcon::fromTheme("network-error", QIcon()));
}

void MainWindow::notifyDaemonErrors() const {
  m_sysTrayIcon->showMessage("Daemon Errors",
                           "The daemon logs contain errors. You should investigate.",
                           QIcon::fromTheme("dialog-warning", QIcon()));
}

void MainWindow::notifyDaemonDisconnected() const {
  m_sysTrayIcon->showMessage("Daemon Disconnected", "Connection with the daemon has been lost",
                           QIcon::fromTheme("network-error", QIcon()));
}

void MainWindow::notifyDaemonConnectionRestored() const {
  m_sysTrayIcon->showMessage("Daemon Connection Restored",
                           "Connection with the daemon has been restored.",
                           QIcon::fromTheme("emblem-default", QIcon()));
}

void MainWindow::requestDaemonErrors() const {
  QNetworkRequest healthRequest;
  healthRequest.setTransferTimeout(DEFAULT_CONNECTION_TIMEOUT_MS);
  healthRequest.setUrl(getEndpointUrl(ENDPOINT_HEALTH.data()));
  const auto healthReply = m_manager->get(healthRequest);
  connect(healthReply, &QNetworkReply::finished, [healthReply, this]() {
    const QString ReplyText = healthReply->readAll();
    const QJsonObject rootObj = QJsonDocument::fromJson(ReplyText.toUtf8()).object();
    if (const auto daemonVersion = rootObj.value("details").toObject().value("version").toString();
        daemonVersion.isEmpty()) {
      qWarning() << "Health version response is empty - must NOT be connected to the daemon API.";
    }
    if (const auto errors = rootObj.value("details").toObject().value("errors").toInt();
        errors > 0) {
      m_deamonHasErrors = true;
      notifyDaemonErrors();
    }
    healthReply->deleteLater();
  });
}

void MainWindow::acknowledgeDaemonErrors() const { m_deamonHasErrors = false; }

void MainWindow::requestAllModes() const {
  QNetworkRequest modesRequest;
  modesRequest.setTransferTimeout(DEFAULT_CONNECTION_TIMEOUT_MS);
  modesRequest.setUrl(getEndpointUrl(ENDPOINT_MODES.data()));
  const auto modesReply = m_manager->get(modesRequest);
  connect(modesReply, &QNetworkReply::finished, [modesReply, this]() {
    const QString modesJson = modesReply->readAll();
    setTrayMenuModes(modesJson);
    modesReply->deleteLater();
  });
}

void MainWindow::setTrayMenuModes(const QString& modesJson) const {
  const QJsonObject rootObj = QJsonDocument::fromJson(modesJson.toUtf8()).object();
  const auto modesArray = rootObj.value("modes").toArray();
  m_modesTrayMenu->setDisabled(modesArray.isEmpty());
  m_modesTrayMenu->clear();
  foreach (QJsonValue value, modesArray) {
    const auto modeName = value.toObject().value("name").toString();
    const auto modeUID = value.toObject().value("uid").toString();
    const auto modeAction = new QAction(modeName);
    modeAction->setStatusTip(modeUID);  // We use the statusTip to store UID
    modeAction->setCheckable(true);
    modeAction->setChecked(modeUID == m_activeModeUID);
    connect(modeAction, &QAction::triggered, [this, modeUID]() {
      // todo: This request needs login Cookie to work:
      QNetworkRequest setModeRequest;
      setModeRequest.setTransferTimeout(DEFAULT_CONNECTION_TIMEOUT_MS);
      auto url = getEndpointUrl(ENDPOINT_MODES_ACTIVE.data());
      url.setPath(url.path() + "/" + modeUID);
      setModeRequest.setUrl(url);
      const auto setModeReply = m_manager->post(setModeRequest, nullptr);
      connect(setModeReply, &QNetworkReply::finished, [setModeReply, this]() {
        if (const auto status =
                setModeReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
            status >= 300) {
          qWarning() << "Error trying to apply Mode. Response Status: " << status;
          return;
        }
        setModeReply->deleteLater();
      });
    });
    m_modesTrayMenu->addAction(modeAction);
  }
}

void MainWindow::setActiveMode(const QString& modeUID) const {
  m_activeModeUID = modeUID;
  foreach (QAction* action, m_modesTrayMenu->actions()) {
    if (action->statusTip() == m_activeModeUID) {
      action->setChecked(true);
    } else {
      action->setChecked(false);
    }
  }
}

void MainWindow::requestActiveMode() const {
  QNetworkRequest modesActiveRequest;
  modesActiveRequest.setTransferTimeout(DEFAULT_CONNECTION_TIMEOUT_MS);
  modesActiveRequest.setUrl(getEndpointUrl(ENDPOINT_MODES_ACTIVE.data()));
  const auto modesActiveReply = m_manager->get(modesActiveRequest);
  connect(modesActiveReply, &QNetworkReply::finished, [modesActiveReply, this]() {
    const QString ReplyText = modesActiveReply->readAll();
    const QJsonObject rootObj = QJsonDocument::fromJson(ReplyText.toUtf8()).object();
    setActiveMode(rootObj.value("current_mode_uid").toString());
    modesActiveReply->deleteLater();
  });
}

void MainWindow::watchLogsAndConnection() const {
  QNetworkRequest sseLogsRequest;
  sseLogsRequest.setAttribute(QNetworkRequest::CacheLoadControlAttribute,
                              QNetworkRequest::AlwaysNetwork);
  sseLogsRequest.setUrl(getEndpointUrl(ENDPOINT_SSE_LOGS.data()));
  const auto sseLogsReply = m_manager->get(sseLogsRequest);
  connect(sseLogsReply, &QNetworkReply::readyRead, [sseLogsReply, this]() {
    // This is also called for keepAlive ticks - but with semi-filled message
    const QString log = sseLogsReply->readAll();
    if (const auto logContainsErrors = log.contains("ERROR");
        logContainsErrors && !m_deamonHasErrors) {
      m_deamonHasErrors = true;
      notifyDaemonErrors();
    }
  });
  connect(sseLogsReply, &QNetworkReply::finished, [this, sseLogsReply]() {
    // on error or dropped connection, retry:
    if (m_isDaemonConnected) {
      m_isDaemonConnected = false;
      notifyDaemonDisconnected();
    }
    while (!m_isDaemonConnected) {
      delay(1000);
      verifyDaemonIsConnected();
    }
    watchLogsAndConnection();
    qInfo() << "Connection to the Daemon Reestablished";
    sseLogsReply->deleteLater();
  });
}

void MainWindow::verifyDaemonIsConnected() const {
  QNetworkRequest healthRequest;
  healthRequest.setTransferTimeout(DEFAULT_CONNECTION_TIMEOUT_MS);
  healthRequest.setUrl(getEndpointUrl(ENDPOINT_HEALTH.data()));
  const auto healthReply = m_manager->get(healthRequest);
  connect(healthReply, &QNetworkReply::readyRead, [healthReply, this]() {
    if (!m_isDaemonConnected) {
      m_isDaemonConnected = true;
      notifyDaemonConnectionRestored();
    }
    healthReply->deleteLater();
  });
}

void MainWindow::watchModeActivation() const {
  QNetworkRequest sseModesRequest;
  sseModesRequest.setAttribute(QNetworkRequest::CacheLoadControlAttribute,
                               QNetworkRequest::AlwaysNetwork);
  sseModesRequest.setUrl(getEndpointUrl(ENDPOINT_SSE_MODES.data()));
  const auto sseModesReply = m_manager->get(sseModesRequest);
  connect(sseModesReply, &QNetworkReply::readyRead, [sseModesReply, this]() {
    const QString modeActivated =
        QString(sseModesReply->readAll()).simplified().replace("event: mode data: ", "");
    const QJsonObject rootObj = QJsonDocument::fromJson(modeActivated.toUtf8()).object();
    if (rootObj.isEmpty()) {
      // This is also called for keepAlive ticks - but semi-empty message
      return;
    }
    const auto currentModeUID = rootObj.value("uid").toString();
    const auto currentModeName = rootObj.value("name").toString();
    const auto modeAlreadyActive = currentModeUID == m_activeModeUID;
    setActiveMode(currentModeUID);
    if (m_activeModeUID.isEmpty()) {
      // This will happen if there is currently no active Mode (null)
      // - such as when applying a setting.
      return;
    }
    const auto msgTitle = modeAlreadyActive ? QString("Mode %1 Already Active").arg(currentModeName)
                                            : QString("Mode %1 Activated").arg(currentModeName);
    m_sysTrayIcon->showMessage(msgTitle, "", QIcon::fromTheme("dialog-information", QIcon()));
  });
  connect(sseModesReply, &QNetworkReply::finished, [this, sseModesReply]() {
    // on error or dropped connection, retry:
    while (!m_isDaemonConnected) {
      delay(1000);
    }
    watchModeActivation();
    sseModesReply->deleteLater();
  });
}

void MainWindow::watchAlerts() const {
  QNetworkRequest alertsRequest;
  alertsRequest.setAttribute(QNetworkRequest::CacheLoadControlAttribute,
                             QNetworkRequest::AlwaysNetwork);
  alertsRequest.setUrl(getEndpointUrl(ENDPOINT_SSE_ALERTS.data()));
  const auto alertsReply = m_manager->get(alertsRequest);
  connect(alertsReply, &QNetworkReply::readyRead, [alertsReply, this]() {
    const QString alert =
        QString(alertsReply->readAll()).simplified().replace("event: alert data: ", "");
    const QJsonObject rootObj = QJsonDocument::fromJson(alert.toUtf8()).object();
    if (rootObj.isEmpty()) {
      // This is also called for keepAlive ticks - but semi-empty message
      return;
    }
    const auto alertState = rootObj.value("state").toString();
    const auto alertName = rootObj.value("name").toString();
    const auto alertMessage = rootObj.value("message").toString();
    const auto msgTitle = alertState == tr("Active") ? QString("Alert: %1 Triggered").arg(alertName)
                                                     : QString("Alert: %1 Resolved").arg(alertName);
    const auto msgIcon = alertState == tr("Active") ? tr("dialog-warning") : tr("emblem-default");
    m_sysTrayIcon->showMessage(msgTitle, alertMessage, QIcon::fromTheme(msgIcon, QIcon()));
  });
  connect(alertsReply, &QNetworkReply::finished, [this, alertsReply]() {
    // on error or dropped connection, retry:
    while (!m_isDaemonConnected) {
      delay(1000);
    }
    watchAlerts();
    alertsReply->deleteLater();
  });
}
