#include "mainwindow.h"

#include <QDebug>
#include <QThread>
#include <QWebEngineView>
#include <QSystemTrayIcon>
#include <QMenu>
#include <QAction>
#include <QApplication>
#include <QSettings>
#include <QWebEngineNewWindowRequest>
#include <QStringBuilder> // for % operator
#include <QWebEngineSettings>
#include <QWizardPage>
#include <QTimer>
#include <QNetworkReply>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QJsonArray>


MainWindow::MainWindow(QWidget *parent)
    : QMainWindow(parent)
      , view(new QWebEngineView(this))
      , profile(new QWebEngineProfile("coolercontrol", view))
      , page(new QWebEnginePage(profile))
      , channel(new QWebChannel(page))
      , ipc(new IPC(this))
      , forceQuit(false)
      , wizard(new QWizard(this))
      , manager(new QNetworkAccessManager(this)) {
    // SETUP
    ////////////////////////////////////////////////////////////////////////////////////////////////
    setCentralWidget(view);
    profile->settings()->setAttribute(QWebEngineSettings::Accelerated2dCanvasEnabled, true);
    profile->settings()->setAttribute(QWebEngineSettings::ScreenCaptureEnabled, false);
    profile->settings()->setAttribute(QWebEngineSettings::PluginsEnabled, false);
    profile->settings()->setAttribute(QWebEngineSettings::PdfViewerEnabled, false);
    // local storage: ~/.local/share/{APP_NAME}
    profile->settings()->setAttribute(QWebEngineSettings::LocalStorageEnabled, true);
    profile->setPersistentCookiesPolicy(QWebEngineProfile::PersistentCookiesPolicy::ForcePersistentCookies);
    channel->registerObject("ipc", ipc);
    page->setWebChannel(channel);
    // This allows external links in our app to be opened by the external browser:
    connect(page, &QWebEnginePage::newWindowRequested, [](QWebEngineNewWindowRequest const &request) {
        QDesktopServices::openUrl(request.requestedUrl());
    });
    view->setPage(page);

    // Wizard init:
    wizard->setWindowTitle("Daemon Connection Error");
    wizard->setButtonText(QWizard::WizardButton::FinishButton, "&Apply");
    wizard->setButtonText(QWizard::WizardButton::CancelButton, "&Quit");
    wizard->setButtonText(QWizard::CustomButton1, "&Reset");
    wizard->setOption(QWizard::HaveCustomButton1, true);
    wizard->addPage(new IntroPage);
    auto addressPage = new AddressPage;
    wizard->addPage(addressPage);
    wizard->setMinimumSize(640, 480);
    connect(wizard, &QWizard::customButtonClicked, [addressPage]() {
        addressPage->resetAddressInputValues();
    });

    // wait to fully initialize if there is a delay set:
    if (const auto startupDelay = ipc->getStartupDelay(); startupDelay > 0) {
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
    showAction =
            ipc->getStartInTray()
                ? new QAction(tr("&Show"), this)
                : new QAction(tr("&Hide"), this);
    connect(showAction, &QAction::triggered, [this]() {
        if (isVisible()) {
            hide();
        } else {
            show();
            activateWindow();
        }
    });

    addressAction = new QAction(tr("&Daemon Address"), this);
    connect(addressAction, &QAction::triggered, [this]() {
        displayAddressWizard();
    });

    quitAction = new QAction(QIcon::fromTheme("application-exit", QIcon()), tr("&Quit"), this);
    connect(quitAction, &QAction::triggered, [this]() {
        forceQuit = true;
        // Triggers the close event but with the forceQuit flag set
        close();
    });
    trayIconMenu = new QMenu(this);
    trayIconMenu->setTitle("CoolerControl");
    trayIconMenu->addAction(ccHeader);
    trayIconMenu->addSeparator();
    modesTrayMenu = new QMenu(this);
    modesTrayMenu->setTitle("Modes");
    modesTrayMenu->setEnabled(false);
    trayIconMenu->addMenu(modesTrayMenu);
    trayIconMenu->addAction(showAction);
    trayIconMenu->addAction(addressAction);
    trayIconMenu->addSeparator();
    trayIconMenu->addAction(quitAction);

    sysTrayIcon = new QSystemTrayIcon(this);
    sysTrayIcon->setContextMenu(trayIconMenu);
    sysTrayIcon->setIcon(QIcon(":/icons/icon.ico"));
    sysTrayIcon->setToolTip("CoolerControl");
    sysTrayIcon->show();

    // left click:
    connect(sysTrayIcon, &QSystemTrayIcon::activated, [this](auto reason) {
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
    view->load(getDaemonUrl());
    connect(view, &QWebEngineView::loadFinished, [this](const bool pageLoadedSuccessfully) {
        if (!pageLoadedSuccessfully) {
            displayAddressWizard();
            notifyDaemonConnectionError();
        } else {
            qInfo() << "Successfully connected to UI at: " << getDaemonUrl().url();
            isDaemonConnected = true;
            requestDaemonErrors();
            requestAllModes();
            requestActiveMode();
            watchLogsAndConnection();
            watchModeActivation();
            watchAlerts();
        }

    });

    // todo: we can probably change the log download blob/link in the UI to point to an external link to see the raw text api endpoint

    // todo: check for existing running CC application(single-instance)?
}

void MainWindow::closeEvent(QCloseEvent *event) {
    ipc->saveWindowGeometry(saveGeometry());
    if (ipc->getCloseToTray() && !forceQuit) {
        hide();
        event->ignore();
        return;
    }
    ipc->syncSettings();
    event->accept();
    QApplication::quit();
}

void MainWindow::hideEvent(QHideEvent *event) {
    setTrayActionToShow();
}

void MainWindow::showEvent(QShowEvent *event) {
    setTrayActionToHide();
}

QUrl MainWindow::getDaemonUrl() {
    const QSettings settings;
    const auto host = settings.value(SETTING_DAEMON_ADDRESS, DEFAULT_DAEMON_ADDRESS.data()).toString();
    const auto port = settings.value(SETTING_DAEMON_PORT, DEFAULT_DAEMON_PORT).toInt();
    const auto sslEnabled = settings.value(SETTING_DAEMON_SSL_ENABLED, DEFAULT_DAEMON_SSL_ENABLED).toBool();
    const auto schema = sslEnabled ? tr("https") : tr("http");
    QUrl url;
    url.setScheme(schema);
    url.setHost(host);
    url.setPort(port);
    return url;
}

QUrl MainWindow::getEndpointUrl(const QString &endpoint) {
    auto url = getDaemonUrl();
    url.setPath(endpoint);
    return url;
}

void MainWindow::displayAddressWizard() const {
    if (wizard->isVisible()) {
        return;
    }
    const auto result = wizard->exec();
    if (result == 0) {
        QApplication::quit();
    } else {
        QSettings settings;
        settings.setValue(SETTING_DAEMON_ADDRESS, wizard->field("address").toString());
        settings.setValue(SETTING_DAEMON_PORT, wizard->field("port").toInt());
        settings.setValue(SETTING_DAEMON_SSL_ENABLED, wizard->field("ssl").toBool());
        view->load(getDaemonUrl());
    }
}

void MainWindow::handleStartInTray() {
    restoreGeometry(ipc->getWindowGeometry());
    setZoomFactor(ipc->getZoomFactor());
    if (ipc->getStartInTray()) {
        hide();
        page->setLifecycleState(QWebEnginePage::LifecycleState::Frozen);
        // todo: can play with this more in the future. It's tricky but there is a possibility of even less resource usage:
        // page->setLifecycleState(QWebEnginePage::LifecycleState::Discarded);
    } else {
        show();
    }
}

void MainWindow::setZoomFactor(const double zoomFactor) const {
    view->setZoomFactor(zoomFactor);
}

void MainWindow::delay(const int millisecondsWait) {
    QEventLoop loop;
    QTimer t;
    t.connect(&t, &QTimer::timeout, &loop, &QEventLoop::quit);
    t.start(millisecondsWait);
    loop.exec();
}

void MainWindow::setTrayActionToShow() const {
    showAction->setText(tr("&Show"));
}

void MainWindow::setTrayActionToHide() const {
    showAction->setText(tr("&Hide"));
}

void MainWindow::notifyDaemonConnectionError() const {
    sysTrayIcon->showMessage(
        "Daemon Connection Error",
        "Connection with the daemon could not be established",
        QIcon::fromTheme("network-error", QIcon())
    );
}

void MainWindow::notifyDaemonErrors() const {
    sysTrayIcon->showMessage(
        "Daemon Errors",
        "The daemon logs contain errors. You should investigate.",
        QIcon::fromTheme("dialog-warning", QIcon())
    );
}

void MainWindow::notifyDaemonDisconnected() const {
    sysTrayIcon->showMessage(
        "Daemon Disconnected",
        "Connection with the daemon has been lost",
        QIcon::fromTheme("network-error", QIcon())
    );
}

void MainWindow::notifyDaemonConnectionRestored() const {
    sysTrayIcon->showMessage(
        "Daemon Connection Restored",
        "Connection with the daemon has been restored.",
        QIcon::fromTheme("emblem-default", QIcon())
    );
}

void MainWindow::requestDaemonErrors() const {
    // DAEMON HEALTH REQUEST:
    QNetworkRequest healthRequest;
    healthRequest.setUrl(getEndpointUrl(ENDPOINT_HEALTH.data()));
    const auto healthReply = manager->get(healthRequest);
    connect(healthReply, &QNetworkReply::finished, [healthReply, this]() {
        const QString ReplyText = healthReply->readAll();
        const QJsonObject rootObj = QJsonDocument::fromJson(ReplyText.toUtf8()).object();
        if (
            const auto daemonVersion = rootObj.value("details").toObject().value("version").toString();
            daemonVersion.isEmpty()
        ) {
            qWarning() << "Health version response is empty - must NOT be connected to the daemon API.";
        }
        if (
            const auto errors = rootObj.value("details").toObject().value("errors").toInt();
            errors > 0
        ) {
            deamonHasErrors = true;
            notifyDaemonErrors();
        }
        healthReply->deleteLater();
    });
}

void MainWindow::requestAllModes() const {
    // LOAD ALL MODES:
    QNetworkRequest modesRequest;
    modesRequest.setUrl(getEndpointUrl(ENDPOINT_MODES.data()));
    const auto modesReply = manager->get(modesRequest);
    connect(modesReply, &QNetworkReply::finished, [modesReply, this]() {
        const QString ReplyText = modesReply->readAll();
        const QJsonObject rootObj = QJsonDocument::fromJson(ReplyText.toUtf8()).object();
        const auto modesArray = rootObj.value("modes").toArray();
        modesTrayMenu->setDisabled(modesArray.isEmpty());
        foreach(QJsonValue value, modesArray) {
            const auto modeName = value.toObject().value("name").toString();
            const auto modeUID = value.toObject().value("uid").toString();
            const auto modeAction = new QAction(modeName);
            modeAction->setStatusTip(modeUID); // We use the statusTip to store UID
            modeAction->setCheckable(true);
            modeAction->setChecked(false);
            connect(modeAction, &QAction::triggered, [this, modeUID, modeAction]() {
                // todo: send Post request to activate this mode
            });
            modesTrayMenu->addAction(modeAction);
        }
        modesReply->deleteLater();
    });
}

void MainWindow::requestActiveMode() const {
    // SET ACTIVE MODE:
    QNetworkRequest modesActiveRequest;
    modesActiveRequest.setUrl(getEndpointUrl(ENDPOINT_MODES_ACTIVE.data()));
    const auto modesActiveReply = manager->get(modesActiveRequest);
    connect(modesActiveReply, &QNetworkReply::finished, [modesActiveReply, this]() {
        const QString ReplyText = modesActiveReply->readAll();
        const QJsonObject rootObj = QJsonDocument::fromJson(ReplyText.toUtf8()).object();
        const auto activeModeUID = rootObj.value("current_mode_uid").toString();
        foreach(QAction *action, modesTrayMenu->actions()) {
            if (action->statusTip() == activeModeUID) {
                action->setChecked(true);
            } else {
                action->setChecked(false);
            }
        }
        modesActiveReply->deleteLater();
    });
}

void MainWindow::watchLogsAndConnection() const {
    QNetworkRequest sseLogsRequest;
    sseLogsRequest.setAttribute(QNetworkRequest::CacheLoadControlAttribute, QNetworkRequest::AlwaysNetwork);
    sseLogsRequest.setUrl(getEndpointUrl(ENDPOINT_SSE_LOGS.data()));
    const auto sseLogsReply = manager->get(sseLogsRequest);
    connect(sseLogsReply, &QNetworkReply::readyRead, [sseLogsReply, this]() {
        // This is also called for keepAlive ticks - but with semi-filled message
        const QString log = sseLogsReply->readAll();
        if (
            const auto logContainsErrors = log.contains("ERROR");
            logContainsErrors && !deamonHasErrors
        ) {
            deamonHasErrors = true;
            notifyDaemonErrors();
        }
    });
    connect(sseLogsReply, &QNetworkReply::finished, [this, sseLogsReply]() {
        // on error or dropped connection, retry:
        if (isDaemonConnected) {
            isDaemonConnected = false;
            notifyDaemonDisconnected();
        }
        while (!isDaemonConnected) {
            delay(1000);
            verifyDaemonIsConnected();
        };
        watchLogsAndConnection();
        qInfo() << "Connection to the Daemon Reestablished";
        sseLogsReply->deleteLater();
    });
}

void MainWindow::verifyDaemonIsConnected() const {
    QNetworkRequest healthRequest;
    healthRequest.setUrl(getEndpointUrl(ENDPOINT_HEALTH.data()));
    const auto healthReply = manager->get(healthRequest);
    connect(healthReply, &QNetworkReply::readyRead, [healthReply, this]() {
        if (!isDaemonConnected) {
            isDaemonConnected = true;
            notifyDaemonConnectionRestored();
        }
        healthReply->deleteLater();
    });
}

void MainWindow::watchModeActivation() const {
    QNetworkRequest sseModesRequest;
    sseModesRequest.setAttribute(QNetworkRequest::CacheLoadControlAttribute, QNetworkRequest::AlwaysNetwork);
    sseModesRequest.setUrl(getEndpointUrl(ENDPOINT_SSE_MODES.data()));
    const auto sseModesReply = manager->get(sseModesRequest);
    connect(sseModesReply, &QNetworkReply::readyRead, [sseModesReply, this]() {
        const QString modeActivated = sseModesReply->readAll();
        const QJsonObject rootObj = QJsonDocument::fromJson(modeActivated.toUtf8()).object();
        if (rootObj.isEmpty()) {
            // This is also called for keepAlive ticks - but semi-empty message
            return;
        }
        const auto modeAlreadyActive = rootObj.value("already_active").toBool();
        const auto activeModeUID = rootObj.value("uid").toString();
        const auto activeModeName = rootObj.value("name").toString();
        const auto msgTitle =
                modeAlreadyActive
                    ? QString("Mode %1 Already Active").arg(activeModeName)
                    : QString("Mode %1 Activated").arg(activeModeName);
        sysTrayIcon->showMessage(
            msgTitle,
            "",
            QIcon::fromTheme("dialog-information", QIcon())
        );
        foreach(QAction *action, modesTrayMenu->actions()) {
            if (action->statusTip() == activeModeUID) {
                action->setChecked(true);
            } else {
                action->setChecked(false);
            }
        }
    });
    connect(sseModesReply, &QNetworkReply::finished, [this, sseModesReply]() {
        // on error or dropped connection, retry:
        while (!isDaemonConnected) {
            delay(1000);
        };
        watchModeActivation();
        sseModesReply->deleteLater();
    });
}

void MainWindow::watchAlerts() const {
    QNetworkRequest alertsRequest;
    alertsRequest.setAttribute(QNetworkRequest::CacheLoadControlAttribute, QNetworkRequest::AlwaysNetwork);
    alertsRequest.setUrl(getEndpointUrl(ENDPOINT_SSE_ALERTS.data()));
    const auto alertsReply = manager->get(alertsRequest);
    connect(alertsReply, &QNetworkReply::readyRead, [alertsReply, this]() {
        const QString alert = alertsReply->readAll();
        const QJsonObject rootObj = QJsonDocument::fromJson(alert.toUtf8()).object();
        if (rootObj.isEmpty()) {
            // This is also called for keepAlive ticks - but semi-empty message
            return;
        }
        const auto alertState = rootObj.value("state").toString();
        const auto alertName = rootObj.value("name").toString();
        const auto alertMessage = rootObj.value("message").toString();
        const auto msgTitle =
                alertState == tr("Active")
                    ? QString("Alert: %1 Triggered").arg(alertName)
                    : QString("Alert: %1 Resolved").arg(alertName);
        const auto msgIcon =
                alertState == tr("Active")
                    ? tr("dialog-warning")
                    : tr("emblem-default");
        sysTrayIcon->showMessage(
            msgTitle,
            alertMessage,
            QIcon::fromTheme(msgIcon, QIcon())
        );
    });
    connect(alertsReply, &QNetworkReply::finished, [this, alertsReply]() {
        // on error or dropped connection, retry:
        while (!isDaemonConnected) {
            delay(1000);
        };
        watchAlerts();
        alertsReply->deleteLater();
    });
}
