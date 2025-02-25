#include "mainwindow.h"
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
      , manager(new QNetworkAccessManager) {
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

    // wait to fully initialize if there is a delay set:
    if (const auto startupDelay = ipc->getStartupDelay(); startupDelay > 0) {
        qInfo() << "Waiting for startup delay: " << startupDelay << "s";
        QThread::sleep(startupDelay);
    }

    // SYSTEM TRAY:
    ////////////////////////////////////////////////////////////////////////////////////////////////
    const auto ccHeader = new QAction(QIcon(":/icons/icon.png"), tr("CoolerControl"), this);
    connect(ccHeader, &QAction::triggered, [this]() {
        // Use CC Tray Header as show-only trigger. (un-minimize doesn't seem to work)
        show();
        activateWindow();
    });
    showAction =
            ipc->getStartInTray()
                ? new QAction(QIcon::fromTheme("window-new", QIcon()), tr("&Show"), this)
                : new QAction(QIcon::fromTheme("window-close", QIcon()), tr("&Hide"), this);
    connect(showAction, &QAction::triggered, [this]() {
        if (isVisible()) {
            hide();
        } else {
            show();
            activateWindow();
        }
    });

    addressAction = new QAction(QIcon::fromTheme("address-book-new", QIcon()), tr("&Daemon Address"), this);
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
                showAction->setText(tr("&Show"));
            } else {
                show();
                activateWindow();
                showAction->setText(tr("&Hide"));
            }
        }
    });

    // LOAD UI:
    ////////////////////////////////////////////////////////////////////////////////////////////////
    view->load(getDaemonUrl());
    connect(view, &QWebEngineView::loadFinished, [this](const bool pageLoadedSuccessfully) {
        if (!pageLoadedSuccessfully) {
            displayAddressWizard();
        } else {
            qInfo() << "Successfully connected to UI at: " << getDaemonUrl().url();

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
                    const auto modeAction = new QAction(modeName, this);
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

    });

    // todo: we can probably change the log download blob/link in the UI to point to an external link to see the raw text api endpoint

    // todo: check for existing running CC application? (there must be some standard for Qt???)
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
    // todo: for testing, the UI address is often different than the daemon address (npm server)
    url.setPort(DEFAULT_DAEMON_PORT);
    return url;
}

void MainWindow::displayAddressWizard() {
    if (wizard == nullptr) {
        wizard = new QWizard;
        wizard->setWindowTitle("Daemon Connection Error");
        wizard->setButtonText(QWizard::WizardButton::FinishButton, "&Apply");
        wizard->setButtonText(QWizard::WizardButton::CancelButton, "&Quit");
        wizard->setButtonText(QWizard::CustomButton1, "&Reset");
        wizard->setOption(QWizard::HaveCustomButton1, true);
        wizard->addPage(new IntroPage);
        auto addressPage = new AddressPage;
        wizard->addPage(addressPage);
        connect(wizard, &QWizard::customButtonClicked, [this, addressPage]() {
            addressPage->resetAddressInputValues();
        });
    }
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
    showAction->setIcon(QIcon::fromTheme("window-new", QIcon()));
}

void MainWindow::setTrayActionToHide() const {
    showAction->setText(tr("&Hide"));
    showAction->setIcon(QIcon::fromTheme("window-close", QIcon()));
}

void MainWindow::notifyDaemonErrors() const {
    sysTrayIcon->showMessage(
        "Daemon Errors",
        "The daemon logs contain errors. You should investigate.",
        QIcon::fromTheme("face-worried", QIcon())
    );
}
