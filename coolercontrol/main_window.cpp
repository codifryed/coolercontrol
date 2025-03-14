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

#include "main_window.h"

#include <QAction>
#include <QApplication>
#include <QDebug>
#include <QDesktopServices>
#include <QFileDialog>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QMenu>
#include <QNetworkCookieJar>
#include <QNetworkReply>
#include <QSettings>
#include <QStringBuilder>  // for % operator
#include <QSystemTrayIcon>
#include <QThread>
#include <QTimer>
#include <QWebEngineCookieStore>
#include <QWebEngineDownloadRequest>
#include <QWebEngineNewWindowRequest>
#include <QWebEngineSettings>
#include <QWebEngineView>
#include <QWizardPage>

#include "constants.h"

MainWindow::MainWindow(QWidget* parent)
    : QMainWindow(parent),
      m_view(new QWebEngineView(parent)),
      m_profile(new QWebEngineProfile("coolercontrol", m_view)),
      m_page(new QWebEnginePage(m_profile)),
      m_channel(new QWebChannel(m_page)),
      m_ipc(new IPC(this)),
      m_wizard(new QWizard(parent)),
      m_manager(new QNetworkAccessManager(parent)) {
  setCentralWidget(m_view);
  m_profile->settings()->setAttribute(QWebEngineSettings::Accelerated2dCanvasEnabled, true);
  m_profile->settings()->setAttribute(QWebEngineSettings::ScreenCaptureEnabled, false);
  m_profile->settings()->setAttribute(QWebEngineSettings::PluginsEnabled, false);
  m_profile->settings()->setAttribute(QWebEngineSettings::PdfViewerEnabled, false);
  // local storage: ~/.local/share/{APP_NAME}
  m_profile->settings()->setAttribute(QWebEngineSettings::LocalStorageEnabled, true);
  m_profile->setPersistentCookiesPolicy(
      QWebEngineProfile::PersistentCookiesPolicy::NoPersistentCookies);
  connect(m_profile, &QWebEngineProfile::downloadRequested,
          [this](QWebEngineDownloadRequest* download) {
            Q_ASSERT(download && download->state() == QWebEngineDownloadRequest::DownloadRequested);
            if (download->isSavePageDownload()) {
              qInfo() << "Saving web pages is disabled.";
              return;
            }
            const QString path = QFileDialog::getSaveFileName(
                this, tr("Save as"),
                QDir(download->downloadDirectory()).filePath(download->downloadFileName()));
            if (path.isEmpty()) return;  // cancelled

            download->setDownloadDirectory(QFileInfo(path).path());
            download->setDownloadFileName(QFileInfo(path).fileName());
            download->accept();
          });
  m_channel->registerObject("ipc", m_ipc);
  m_page->setWebChannel(m_channel);
  // This allows external links in our app to be opened by the external browser:
  connect(m_page, &QWebEnginePage::newWindowRequested,
          [](QWebEngineNewWindowRequest const& request) {
            QDesktopServices::openUrl(request.requestedUrl());
          });
  m_view->setPage(m_page);
  const auto cookieStore = m_profile->cookieStore();
  connect(cookieStore, &QWebEngineCookieStore::cookieAdded,
          [this](const QNetworkCookie& cookie) { m_manager->cookieJar()->insertCookie(cookie); });
  connect(cookieStore, &QWebEngineCookieStore::cookieRemoved,
          [this](const QNetworkCookie& cookie) { m_manager->cookieJar()->deleteCookie(cookie); });
  cookieStore->loadAllCookies();

  connect(this, &MainWindow::daemonConnectionLost, this, &MainWindow::reestablishDaemonConnection,
          Qt::QueuedConnection);
  connect(this, &MainWindow::watchForSSE, this, &MainWindow::startWatchingSSE,
          Qt::QueuedConnection);
  connect(this, &MainWindow::setZoomFactorSignal, this, &MainWindow::setZoomFactor,
          Qt::QueuedConnection);
  connect(this, &MainWindow::setTrayMenuModesSignal, this, &MainWindow::setTrayMenuModes,
          Qt::QueuedConnection);
  connect(this, &MainWindow::acknowledgeDaemonErrorsSignal, this,
          &MainWindow::acknowledgeDaemonErrors, Qt::QueuedConnection);
  connect(this, &MainWindow::forceQuitSignal, this, &MainWindow::forceQuit, Qt::QueuedConnection);

  initWizard();
  initDelay();
  initSystemTray();
  initWebUI();
}

void MainWindow::initWizard() {
  m_wizard->setWindowTitle("Daemon Connection Error");
  m_wizard->setOption(QWizard::IndependentPages, true);
  m_wizard->setButtonText(QWizard::WizardButton::FinishButton, "&Apply");
  m_wizard->setOption(QWizard::CancelButtonOnLeft, true);
  m_wizard->setButtonText(QWizard::CustomButton1, "&Retry");
  m_wizard->setOption(QWizard::HaveCustomButton1, true);
  m_wizard->setButtonText(QWizard::HelpButton, "&Quit App");
  m_wizard->setOption(QWizard::HaveHelpButton, true);
  m_wizard->addPage(new IntroPage(m_wizard));
  auto addressPage = new AddressPage(m_wizard);
  m_wizard->addPage(addressPage);
  m_wizard->setMinimumSize(640, 480);
  connect(m_wizard, &QWizard::helpRequested, []() { QApplication::quit(); });
  connect(m_wizard, &QWizard::customButtonClicked, [this](const int which) {
    if (which == 6) {  // Retry CustomButton1
      m_view->load(getDaemonUrl());
      m_wizard->hide();
    }
  });
  connect(m_wizard, &QDialog::accepted, [this]() {
    QSettings settings;
    settings.setValue(SETTING_DAEMON_ADDRESS.data(), m_wizard->field("address").toString());
    settings.setValue(SETTING_DAEMON_PORT.data(), m_wizard->field("port").toInt());
    settings.setValue(SETTING_DAEMON_SSL_ENABLED.data(), m_wizard->field("ssl").toBool());
    m_changeAddress = true;
    emit dropConnections();
    delay(300);  // give signals a moment to process.
    m_startup = true;
    m_changeAddress = false;
    m_isDaemonConnected = false;
    m_view->load(getDaemonUrl());
  });
}

void MainWindow::initDelay() const {
  if (const auto startupDelay = m_ipc->getStartupDelay(); startupDelay > 0) {
    qInfo() << "Waiting for startup delay: " << startupDelay << "s";
    QThread::sleep(startupDelay);
  }
}

void MainWindow::initSystemTray() {
  m_sysTrayIcon = new QSystemTrayIcon(this->parent());
  const auto ccHeader = new QAction(QIcon::fromTheme(APP_ID.data(), QIcon(":/icons/icon.png")),
                                    tr("CoolerControl"), m_sysTrayIcon);
  ccHeader->setDisabled(true);
  m_showAction = m_ipc->getStartInTray() ? new QAction(tr("&Show"), m_sysTrayIcon)
                                         : new QAction(tr("&Hide"), m_sysTrayIcon);
  connect(
      m_showAction, &QAction::triggered, this,
      [this]() {
        if (isVisible()) {
          hide();
        } else {
          showNormal();
          raise();
          activateWindow();
        }
      },
      Qt::QueuedConnection);

  m_addressAction = new QAction(tr("&Daemon Address"), m_sysTrayIcon);
  connect(m_addressAction, &QAction::triggered, [this]() { displayAddressWizard(); });

  m_quitAction =
      new QAction(QIcon::fromTheme("application-exit", QIcon()), tr("&Quit"), m_sysTrayIcon);
  connect(m_quitAction, &QAction::triggered, this, &MainWindow::forceQuit, Qt::QueuedConnection);
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

  m_sysTrayIcon->setContextMenu(m_trayIconMenu);
  m_sysTrayIcon->setIcon(QIcon::fromTheme(APP_ID.data(), QIcon(":/icons/icon.ico")));
  m_sysTrayIcon->setToolTip("CoolerControl");
  m_sysTrayIcon->show();

  // left click:
  connect(
      m_sysTrayIcon, &QSystemTrayIcon::activated, this,
      [this](auto reason) {
        if (reason == QSystemTrayIcon::Trigger) {
          if (isVisible()) {
            hide();
          } else {
            showNormal();
            raise();
            activateWindow();
          }
        }
      },
      Qt::QueuedConnection);
}

void MainWindow::initWebUI() {
  m_view->load(getDaemonUrl());
  connect(m_view, &QWebEngineView::loadFinished, [this](const bool pageLoadedSuccessfully) {
    if (!pageLoadedSuccessfully) {
      displayAddressWizard();
      notifyDaemonConnectionError();
    } else {
      qInfo() << "Successfully loaded UI at: " << getDaemonUrl().url();
      if (m_startup) {  // don't do this for Wizard retries
        while (!m_isDaemonConnected) {
          delay(1000);
          tryDaemonConnection();
        }
        requestDaemonErrors();
        requestAllModes();
        requestActiveMode();
        emit watchForSSE();
        qInfo() << "Successfully connected to the Daemon";
        m_startup = false;
      }
    }
  });
}

void MainWindow::forceQuit() {
  m_forceQuit = true;
  // Triggers the close event but with the forceQuit flag set
  close();
}

void MainWindow::closeEvent(QCloseEvent* event) {
  if (m_startup) {
    // Killing the app during initialization can cause a crash
    event->ignore();
    return;
  }
  if (isVisible()) {
    m_ipc->saveWindowGeometry(saveGeometry());
  }
  if (m_ipc->getCloseToTray() && !m_forceQuit) {
    delay(100);
    hide();
    event->ignore();
    return;
  }
  m_isDaemonConnected = false;  // stops from trying to reconnect
  emit dropConnections();
  m_ipc->syncSettings();
  event->accept();
  m_page->deleteLater();
  delay(200);
  QApplication::quit();
}

void MainWindow::hideEvent(QHideEvent* event) {
  if (m_startup) {
    // opening/closing the window during initialization can cause issues.
    event->ignore();
    return;
  }
  delay(100);
  setTrayActionToShow();
  event->accept();
}

void MainWindow::showEvent(QShowEvent* event) {
  if (m_startup) {
    // opening/closing the window during initialization can cause issues.
    event->ignore();
    return;
  }
  delay(100);
  setTrayActionToHide();
  event->accept();
}

QUrl MainWindow::getDaemonUrl() {
  const QSettings settings;
  const auto host =
      settings.value(SETTING_DAEMON_ADDRESS.data(), DEFAULT_DAEMON_ADDRESS.data()).toString();
  const auto port = settings.value(SETTING_DAEMON_PORT.data(), DEFAULT_DAEMON_PORT).toInt();
  const auto sslEnabled =
      settings.value(SETTING_DAEMON_SSL_ENABLED.data(), DEFAULT_DAEMON_SSL_ENABLED).toBool();
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
  // for testing with npm dev server:
  // url.setPort(DEFAULT_DAEMON_PORT);
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
    setAttribute(Qt::WidgetAttribute::WA_DontShowOnScreen, true);
    show();  // this triggers browser engine rendering - which we want for startup&login
    connect(
        m_ipc, &IPC::webLoadFinished, this,
        [this]() {
          delay(300);  // small pause to let web engine breath before suspending.
          hide();
          setAttribute(Qt::WidgetAttribute::WA_DontShowOnScreen, false);
          qInfo() << "Initialized closed to system tray.";
        },
        Qt::SingleShotConnection);
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
  connect(healthReply, &QNetworkReply::readyRead, [healthReply, this]() {
    const auto status = healthReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    const QString replyText = healthReply->readAll();
    qDebug() << "Health Endpoint Response Status: " << status << "; Body: " << replyText;
    const QJsonObject rootObj = QJsonDocument::fromJson(replyText.toUtf8()).object();
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
  connect(healthReply, &QNetworkReply::errorOccurred,
          [healthReply](const QNetworkReply::NetworkError code) {
            const auto status =
                healthReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
            qWarning() << "Error occurred connecting to Daemon Health endpoint. Status: " << status
                       << " QtErrorCode: " << code;
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
    const auto status = modesReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    const QString modesJson = modesReply->readAll();
    qDebug() << "Modes Endpoint Response Status: " << status << "; Body: " << modesJson;
    setTrayMenuModes(modesJson);
    modesReply->deleteLater();
  });
  connect(modesReply, &QNetworkReply::errorOccurred,
          [modesReply](const QNetworkReply::NetworkError code) {
            const auto status =
                modesReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
            qWarning() << "Error occurred connecting to Daemon Modes endpoint. Status: " << status
                       << " QtErrorCode: " << code;
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
      QNetworkRequest setModeRequest;
      setModeRequest.setTransferTimeout(DEFAULT_CONNECTION_TIMEOUT_MS);
      auto url = getEndpointUrl(ENDPOINT_MODES_ACTIVE.data());
      url.setPath(url.path() + "/" + modeUID);
      setModeRequest.setUrl(url);
      const auto setModeReply = m_manager->post(setModeRequest, QByteArray());
      connect(setModeReply, &QNetworkReply::finished, [setModeReply, this]() {
        const auto status =
            setModeReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        if (status == 401) {
          m_view->showNormal();  // show window if we have login credentials error
          qWarning() << "Authentication no longer valid when trying to apply Mode. Please login.";
        }
        if (status >= 300) {
          qWarning() << "Error trying to apply Mode. Response Status: " << status;
          setActiveMode(m_activeModeUID);
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
    const auto status =
        modesActiveReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    const QString replyText = modesActiveReply->readAll();
    qDebug() << "ModesActive Endpoint Response Status: " << status << "; Body: " << replyText;
    const QJsonObject rootObj = QJsonDocument::fromJson(replyText.toUtf8()).object();
    setActiveMode(rootObj.value("current_mode_uid").toString());
    modesActiveReply->deleteLater();
  });
}

void MainWindow::startWatchingSSE() const {
  watchConnectionAndLogs();
  watchModeActivation();
  watchAlerts();
}

void MainWindow::watchConnectionAndLogs() const {
  QNetworkRequest sseLogsRequest;
  sseLogsRequest.setAttribute(QNetworkRequest::CacheLoadControlAttribute,
                              QNetworkRequest::AlwaysNetwork);
  sseLogsRequest.setUrl(getEndpointUrl(ENDPOINT_SSE_LOGS.data()));
  const auto sseLogsReply = m_manager->get(sseLogsRequest);
  connect(this, &MainWindow::dropConnections, sseLogsReply, &QNetworkReply::abort,
          Qt::DirectConnection);
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
    const auto status = sseLogsReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    qDebug() << "Log Watch SSE closed with status: " << status;
    // on error or dropped connection will be re-connected once connection is re-established.
    if (m_isDaemonConnected && !m_changeAddress) {
      m_isDaemonConnected = false;
      notifyDaemonDisconnected();
      emit daemonConnectionLost();
      qInfo() << "Connection to the Daemon Lost";
    }
    sseLogsReply->deleteLater();
  });
}

void MainWindow::reestablishDaemonConnection() const {
  if (m_isDaemonConnected || m_changeAddress) {
    return;
  }
  emit dropConnections();
  while (!m_isDaemonConnected) {
    delay(2000);
    tryDaemonConnection();
  }
  qInfo() << "Connection to the Daemon Reestablished";
  emit watchForSSE();
}

void MainWindow::tryDaemonConnection() const {
  QNetworkRequest healthRequest;
  healthRequest.setTransferTimeout(DEFAULT_CONNECTION_TIMEOUT_MS);
  healthRequest.setUrl(getEndpointUrl(ENDPOINT_HEALTH.data()));
  const auto healthReply = m_manager->get(healthRequest);
  connect(healthReply, &QNetworkReply::readyRead, [this, healthReply]() {
    if (!m_isDaemonConnected) {
      m_isDaemonConnected = true;
      if (!m_startup) {
        notifyDaemonConnectionRestored();
      }
    }
    healthReply->deleteLater();
  });
  connect(healthReply, &QNetworkReply::errorOccurred,
          [healthReply](const QNetworkReply::NetworkError code) {
            const auto status =
                healthReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
            qDebug() << "Error occurred establishing connection to Daemon. Status: " << status
                     << " QtErrorCode: " << code;
            healthReply->deleteLater();
          });
}

void MainWindow::watchModeActivation() const {
  QNetworkRequest sseModesRequest;
  sseModesRequest.setAttribute(QNetworkRequest::CacheLoadControlAttribute,
                               QNetworkRequest::AlwaysNetwork);
  sseModesRequest.setUrl(getEndpointUrl(ENDPOINT_SSE_MODES.data()));
  const auto sseModesReply = m_manager->get(sseModesRequest);
  connect(this, &MainWindow::dropConnections, sseModesReply, &QNetworkReply::abort,
          Qt::DirectConnection);
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
  connect(sseModesReply, &QNetworkReply::finished, [sseModesReply]() {
    // on error or dropped connection will be re-connected once connection is re-established.
    const auto status = sseModesReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    qDebug() << "Modes SSE closed with status: " << status;
    sseModesReply->deleteLater();
  });
}

void MainWindow::watchAlerts() const {
  QNetworkRequest alertsRequest;
  alertsRequest.setAttribute(QNetworkRequest::CacheLoadControlAttribute,
                             QNetworkRequest::AlwaysNetwork);
  alertsRequest.setUrl(getEndpointUrl(ENDPOINT_SSE_ALERTS.data()));
  const auto alertsReply = m_manager->get(alertsRequest);
  connect(this, &MainWindow::dropConnections, alertsReply, &QNetworkReply::abort,
          Qt::DirectConnection);
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
  connect(alertsReply, &QNetworkReply::finished, [alertsReply]() {
    const auto status = alertsReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    qDebug() << "Alerts SSE closed with status: " << status;
    // on error or dropped connection will be re-connected once connection is re-established.
    alertsReply->deleteLater();
  });
}
