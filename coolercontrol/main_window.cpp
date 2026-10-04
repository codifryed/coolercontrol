// SPDX-FileCopyrightText: 2025 Guy Boldon and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "main_window.h"

#include <QAction>
#include <QActionGroup>
#include <QApplication>
#include <QCheckBox>
#include <QDebug>
#include <QDesktopServices>
#include <QDir>
#include <QFile>
#include <QFileDialog>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QMenu>
#include <QMessageBox>
#include <QNetworkCookieJar>
#include <QNetworkReply>
#include <QRegularExpression>
#include <QSettings>
#include <QShortcut>
#include <QStringBuilder>  // for % operator
#include <QSystemTrayIcon>
#include <QThread>
#include <QTimer>
#include <QWebEngineCookieStore>
#include <QWebEngineDownloadRequest>
#include <QWebEngineFullScreenRequest>
#include <QWebEngineNewWindowRequest>
#include <QWebEngineSettings>
#include <QWebEngineView>
#include <QWizardPage>
#include <algorithm>
#include <initializer_list>
#include <memory>

#include "connections.h"
#include "constants.h"
#include "notifier.h"
#include "origin_filter.h"
#include "sse_parser.h"
#include "system_palette.h"
#include "tls_trust.h"
#include "translations.h"

/*
  Icon themes disagree on what exists: Breeze carries the plain names, modern Adwaita
  only the symbolic ones, and neither covers everything. Take the first name the current
  theme actually has, and no icon at all when it has none. A wrong icon reads worse than
  none, so nothing here substitutes an approximation.
*/
QIcon themeIcon(const std::initializer_list<const char*> names) {
  for (const auto* name : names) {
    if (const auto candidate = QLatin1String(name); QIcon::hasThemeIcon(candidate)) {
      return QIcon::fromTheme(candidate);
    }
  }
  return {};
}

QIcon showIcon() { return themeIcon({"view-restore", "view-restore-symbolic"}); }

QIcon hideIcon() { return themeIcon({"window-minimize", "window-minimize-symbolic"}); }

/*
  Qt has no API for the settings file's mode, and QSettings creates it 0644. It does
  preserve the mode across later atomic rewrites (verified on Qt 6.11), so one call once
  the file exists is enough; this runs on each credential write only so a file that gets
  recreated cannot silently regress. Both credentials live in that file: the session
  cookie and the daemon access token. The token is the more sensitive of the two, since
  it does not expire and carries write access.
*/
QString accessTokenKey(const QString& daemonKey) {
  return QString(SETTING_GROUP_ACCESS_TOKENS.data()) % "/" % daemonKey;
}

QString accessTokenIdKey(const QString& daemonKey) {
  return QString(SETTING_GROUP_ACCESS_TOKEN_IDS.data()) % "/" % daemonKey;
}

void restrictSettingsFilePermissions(const QSettings& settings) {
  if (!QFile::setPermissions(settings.fileName(),
                             QFileDevice::ReadOwner | QFileDevice::WriteOwner)) {
    qWarning() << "Could not restrict permissions on" << settings.fileName();
  }
}

class PersistentCookieJar final : public QNetworkCookieJar {
 public:
  explicit PersistentCookieJar(QObject* parent = nullptr) : QNetworkCookieJar(parent) { load(); }

  bool insertCookie(const QNetworkCookie& cookie) override {
    if (QNetworkCookieJar::insertCookie(cookie)) {
      save();
      return true;
    }
    return false;
  }

  bool deleteCookie(const QNetworkCookie& cookie) override {
    if (QNetworkCookieJar::deleteCookie(cookie)) {
      save();
      return true;
    }
    return false;
  }

 private:
  void save() const {
    QSettings settings;
    QByteArray cookieData;
    for (const auto& cookie : allCookies()) {
      cookieData.append(cookie.toRawForm());
      cookieData.append("\n");
    }
    settings.setValue("networkCookies", cookieData);
    settings.sync();
    restrictSettingsFilePermissions(settings);
  }

  void load() {
    const QSettings settings;
    const auto cookieData = settings.value("networkCookies").toByteArray();
    if (cookieData.isEmpty()) return;
    const auto cookies = QNetworkCookie::parseCookies(cookieData);
    for (const auto& cookie : cookies) {
      QNetworkCookieJar::insertCookie(cookie);
    }
    qDebug() << "Loaded" << cookies.size() << "persisted cookies for QNetworkAccessManager";
  }
};

MainWindow::MainWindow(QWidget* parent)
    : QMainWindow(parent),
      m_view(new QWebEngineView(parent)),
      m_profile(new QWebEngineProfile(WEBENGINE_PROFILE_NAME.c_str(), m_view)),
      m_page(new QWebEnginePage(m_profile)),
      m_channel(new QWebChannel(m_page)),
      m_ipc(new IPC(this)),
      m_wizard(new QWizard(parent)),
      m_introPage(new IntroPage(m_wizard)),
      m_addressPage(new AddressPage(m_wizard)),
      m_manager(new QNetworkAccessManager(parent)),
      m_originFilter(new OriginFilter(this)),
      m_retryTimer(new QTimer(parent)),
      m_discardTimer(new QTimer(parent)),
      m_sensorPollTimer(new QTimer(parent)) {
  setCentralWidget(m_view);
  m_profile->settings()->setAttribute(QWebEngineSettings::Accelerated2dCanvasEnabled, true);
  m_profile->settings()->setAttribute(QWebEngineSettings::FullScreenSupportEnabled, true);
  m_profile->settings()->setAttribute(QWebEngineSettings::ScreenCaptureEnabled, false);
  m_profile->settings()->setAttribute(QWebEngineSettings::PluginsEnabled, false);
  m_profile->settings()->setAttribute(QWebEngineSettings::JavascriptCanAccessClipboard, true);
  m_profile->settings()->setAttribute(QWebEngineSettings::PdfViewerEnabled, false);
  // local storage: ~/.local/share/{APP_NAME}
  m_profile->settings()->setAttribute(QWebEngineSettings::LocalStorageEnabled, true);
  m_profile->setPersistentCookiesPolicy(
      QWebEngineProfile::PersistentCookiesPolicy::ForcePersistentCookies);
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
  // Read once up front so the first page load can pull it, then follow changes.
  refreshSystemPalette();
  const auto paletteWatcher = new system_palette::Watcher(this);
  connect(paletteWatcher, &system_palette::Watcher::changed, this,
          &MainWindow::refreshSystemPalette);
  // This allows external links in our app to be opened by the external browser. Web links
  // only: any other scheme would be handed to whatever local handler is registered for it.
  connect(m_page, &QWebEnginePage::newWindowRequested,
          [](QWebEngineNewWindowRequest const& request) {
            const QUrl url = request.requestedUrl();
            const QString scheme = url.scheme();
            if (scheme != QLatin1String("http") && scheme != QLatin1String("https")) {
              qWarning() << "Blocked a new window for a non-web link:" << url.toDisplayString();
              return;
            }
            QDesktopServices::openUrl(url);
          });
  connect(m_page, &QWebEnginePage::fullScreenRequested,
          [this](QWebEngineFullScreenRequest request) {
            // Qt/WebEngine has a strange bug here where toggleOn() is aligned with the JS engine
            //  and not with the Window's real full-screen state. Can cause an edge case issue when
            //  shortcut and toggle are used and the user refreshes the UI.
            qDebug() << "FullScreen request: " << request.toggleOn();
            request.accept();
            setWindowState(windowState() ^ Qt::WindowFullScreen);
          });
  // Confine the engine to the configured daemon before anything is loaded.
  m_originFilter->setDaemonUrl(getDaemonUrl());
  m_profile->setUrlRequestInterceptor(m_originFilter);
  m_view->setPage(m_page);
  m_manager->setCookieJar(new PersistentCookieJar(m_manager));
  const auto cookieStore = m_profile->cookieStore();
  connect(cookieStore, &QWebEngineCookieStore::cookieAdded,
          [this](const QNetworkCookie& cookie) { m_manager->cookieJar()->insertCookie(cookie); });
  connect(cookieStore, &QWebEngineCookieStore::cookieRemoved,
          [this](const QNetworkCookie& cookie) { m_manager->cookieJar()->deleteCookie(cookie); });
  cookieStore->loadAllCookies();
  loadAccessToken();

  connect(this, &MainWindow::daemonConnectionLost, this, &MainWindow::reestablishDaemonConnection,
          Qt::QueuedConnection);
  connect(this, &MainWindow::watchForSSE, this, &MainWindow::startWatchingSSE,
          Qt::QueuedConnection);
  m_retryTimer->setInterval(DEFAULT_CONNECTION_RETRY_INTERVAL_MS);
  connect(m_retryTimer, &QTimer::timeout, this, &MainWindow::tryDaemonConnection);
  m_discardTimer->setSingleShot(true);
  // Sits just above the UI's own stale-history threshold, so the two never disagree
  // about whether a restore counts as fresh.
  m_discardTimer->setInterval(DISCARD_DELAY_MS);
  connect(m_discardTimer, &QTimer::timeout, this, &MainWindow::discardPage);
  m_sensorPollTimer->setInterval(TRAY_SENSOR_POLL_MS);
  connect(m_sensorPollTimer, &QTimer::timeout, this, &MainWindow::pollTraySensors);
  connect(m_ipc, &IPC::forceWindowShow, this, [this]() {
    // This is used so the UI Window will show when password input is required
    setAttribute(Qt::WidgetAttribute::WA_DontShowOnScreen, false);
    showNormal();
    raise();
    activateWindow();
    qInfo() << "Force UI Window to show.";
  });

  initWizard();
  initDelay();
  initSystemTray();
  initWebUI();

  const auto fullscreenKeyToggle = new QShortcut(this);
  fullscreenKeyToggle->setKey(Qt::Key_F11);
  connect(fullscreenKeyToggle, &QShortcut::activated, [this]() {
    qDebug() << "FullScreen Key Triggered";
    setWindowState(windowState() ^ Qt::WindowFullScreen);
    emit m_ipc->fullScreenToggled(isFullScreen());
  });
}

void MainWindow::initWizard() {
  m_wizard->setWindowTitle(uiString("wizard.windowTitle", tr("Daemon Connection Error")));
  m_wizard->setOption(QWizard::IndependentPages, true);
  m_wizard->setButtonText(QWizard::WizardButton::FinishButton,
                          uiString("wizard.apply", tr("&Apply")));
  m_wizard->setOption(QWizard::CancelButtonOnLeft, true);
  m_wizard->setButtonText(QWizard::CustomButton1, uiString("wizard.retry", tr("&Retry")));
  m_wizard->setOption(QWizard::HaveCustomButton1, true);
  m_wizard->setButtonText(QWizard::HelpButton, uiString("wizard.quitApp", tr("&Quit App")));
  m_wizard->setOption(QWizard::HaveHelpButton, true);
  m_wizard->addPage(m_introPage);
  m_wizard->addPage(m_addressPage);
  m_wizard->setMinimumSize(640, 480);
  connect(m_wizard, &QWizard::helpRequested, this, &MainWindow::forceQuit, Qt::QueuedConnection);
  connect(m_wizard, &QWizard::customButtonClicked, [this](const int which) {
    if (which == 6) {  // Retry CustomButton1
      delay(500);
      m_uiLoadingStopped = false;
      loadVerifiedDaemonUi();
      m_wizard->hide();
    }
  });
  connect(m_wizard, &QDialog::accepted,
          [this]() { applyDaemonConnection(m_addressPage->commit()); });
  // Removing an entry never reaches applyDaemonConnection, so the menu is refreshed on
  // the way out whatever the user did in there.
  connect(m_wizard, &QDialog::finished, this, [this]() { rebuildDaemonsTrayMenu(); });
}

void MainWindow::initDelay() const {
  if (const auto startupDelay = m_ipc->getStartupDelay(); startupDelay > 0) {
    qInfo() << "Waiting for startup delay: " << startupDelay << "s";
    QThread::sleep(startupDelay);
  }
}

void MainWindow::initSystemTray() {
  m_sysTrayIcon = new QSystemTrayIcon(this->parent());
  const auto ccHeader = new QAction(
      QIcon::fromTheme(APP_ID.data(), QIcon(":/icons/org.coolercontrol.CoolerControl.svg")),
      tr("CoolerControl"), m_sysTrayIcon);
  ccHeader->setDisabled(true);
  m_showAction = m_ipc->getStartInTray()
                     ? new QAction(showIcon(), uiString("tray.show", tr("&Show")), m_sysTrayIcon)
                     : new QAction(hideIcon(), uiString("tray.hide", tr("&Hide")), m_sysTrayIcon);
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

  m_addressAction =
      new QAction(themeIcon({"network-server", "network-server-symbolic"}),
                  uiString("tray.daemonConnection", tr("&Daemon Connection\u2026")), m_sysTrayIcon);
  connect(m_addressAction, &QAction::triggered, [this]() { displayAddressWizard(); });

  m_quitAction = new QAction(themeIcon({"application-exit", "application-exit-symbolic"}),
                             uiString("tray.quit", tr("&Quit")), m_sysTrayIcon);
  connect(m_quitAction, &QAction::triggered, this, &MainWindow::forceQuit, Qt::QueuedConnection);
  m_trayIconMenu = new QMenu(this);
  m_trayIconMenu->setTitle("CoolerControl");
  m_trayIconMenu->addAction(ccHeader);
  m_trayIconMenu->addSeparator();
  // Pinned-sensor rows go above Modes, either inline or inside this submenu once there
  // are enough of them to crowd the main menu.
  m_sensorsTrayMenu = new QMenu(this);
  m_sensorsTrayMenu->setIcon(
      themeIcon({"utilities-system-monitor", "speedometer-symbolic", "temperature-normal-symbolic",
                 "power-profile-performance-symbolic"}));
  m_sensorsTrayMenu->setTitle(uiString("tray.sensors", tr("Sensors")));
  m_trayIconMenu->addMenu(m_sensorsTrayMenu);
  m_sensorsTrayMenu->menuAction()->setVisible(false);
  m_modesTrayMenu = new QMenu(this);
  m_modesTrayMenu->setIcon(
      themeIcon({"format-list-unordered", "view-list-symbolic", "view-list-bullet-symbolic"}));
  m_modesTrayMenu->setTitle(uiString("tray.modes", tr("Modes")));
  m_modesTrayMenu->setEnabled(false);
  m_trayIconMenu->addMenu(m_modesTrayMenu);
  m_trayIconMenu->addAction(m_showAction);
  // Next to the entry that manages connections, and below Show/Hide so that stays where
  // muscle memory expects it.
  m_daemonsTrayMenu = new QMenu(this);
  m_daemonsTrayMenu->setIcon(themeIcon({"network-server", "network-server-symbolic"}));
  m_daemonsTrayMenu->setTitle(uiString("tray.daemons", tr("Daemons")));
  m_trayIconMenu->addMenu(m_daemonsTrayMenu);
  rebuildDaemonsTrayMenu();
  m_trayIconMenu->addAction(m_addressAction);
  m_trayIconMenu->addSeparator();
  m_trayIconMenu->addAction(m_quitAction);

  // Readings are fetched only while the menu is opening, so nothing polls in the
  // background and a closed menu costs nothing.
  connect(m_trayIconMenu, &QMenu::aboutToShow, this, &MainWindow::refreshTraySensors);
  connect(m_trayIconMenu, &QMenu::aboutToHide, this, &MainWindow::stopTraySensorPolling);

  m_sysTrayIcon->setContextMenu(m_trayIconMenu);
  m_sysTrayIcon->setIcon(QIcon::fromTheme(
      APP_ID_SYMBOLIC.data(),
      QIcon::fromTheme(APP_ID.data(),
                       QIcon(":/icons/org.coolercontrol.CoolerControl-symbolic.svg"))));
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
  connect(m_page, &QWebEnginePage::certificateError, this,
          [this](const QWebEngineCertificateError& error) {
            const auto chain = error.certificateChain();
            const auto host = error.url().host();
            const auto port = error.url().port(DEFAULT_DAEMON_PORT);
            const auto leaf = chain.isEmpty() ? QSslCertificate() : chain.first();
            qDebug() << "Certificate error from" << error.url().toDisplayString() << ":"
                     << error.description();
            // A page load is the one moment a prompt makes sense, so this is where trust
            // on first use is established. Every other request consults the pin it leaves
            // behind rather than asking again.
            if (confirmCertificate(host, port, leaf, false)) {
              auto accepted = error;
              accepted.acceptCertificate();
            } else {
              auto rejected = error;
              rejected.rejectCertificate();
            }
          });
  loadVerifiedDaemonUi();
  connect(m_view, &QWebEngineView::loadFinished, [this](const bool pageLoadedSuccessfully) {
    if (!pageLoadedSuccessfully) {
      if (m_uiLoadRetryCount < MAX_UI_LOAD_RETRIES) {
        m_uiLoadRetryCount++;
        qDebug() << "UI load failed, retrying..." << m_uiLoadRetryCount << "/"
                 << MAX_UI_LOAD_RETRIES;
        QTimer::singleShot(1500, this, [this]() { m_view->load(getDaemonUrl()); });
        return;
      }
      m_uiLoadingStopped = true;
      m_uiLoadRetryCount = 0;
      displayAddressWizard();
      notifyDaemonConnectionError();
    } else {
      // Blanking the page is deliberate, and that load reports success. Treating it as a
      // loaded UI cleared the stop flag a refused certificate had just set, logged a false
      // success and restarted the retry timer, which left m_startup set and closeEvent
      // ignoring every close for the rest of the process.
      if (m_view->url().toString() == QStringLiteral("about:blank")) {
        return;
      }
      m_uiLoadingStopped = false;
      m_uiLoadRetryCount = 0;
      qInfo() << "Successfully loaded UI at: " << getDaemonUrl().url();
      if (m_startup) {  // don't do this for Wizard retries
        m_retryTimer->start();
      }
    }
  });
}

/*
  Confirms the configured address is a CoolerControl daemon before rendering it.

  The address is user-configurable, so without this a typo can point an embedded browser
  at an arbitrary site. /handshake is unauthenticated and answers {"shake": true}, so the
  check works before login and costs one small request.

  This proves "responds like a daemon", not "is trustworthy": any server can return that
  JSON. It is a guard against misconfiguration; TLS pinning is what guards against an
  attacker.
*/
/*
  Stops after a certificate is refused.

  A page loaded earlier keeps its own connections, and the web engine remembers the
  exception it was already granted, so leaving it on screen would let it carry on talking
  to a daemon the user just declined. Only trust refusals do this: a daemon that is merely
  unreachable should keep its page and reconnect, as it always has.
*/
void MainWindow::refuseDaemonCertificate(const QString& reason) {
  m_lastConnectionError = reason;
  m_uiLoadRetryCount = 0;
  m_uiLoadingStopped = true;
  m_view->load(QUrl(QStringLiteral("about:blank")));
  displayAddressWizard();
}

void MainWindow::loadVerifiedDaemonUi() {
  /*
    Force a fresh handshake. Qt pools connections, and a pooled one is reused without
    renegotiating, so QNetworkReply::sslErrors never fires again for the life of that
    connection. Measured: request 1 ignoring errors succeeds, request 2 refusing them
    still succeeds because the policy is never asked, and only after clearing the cache
    does it get consulted. Without this, turning certificate validation on has no effect
    until the app is restarted.
  */
  m_manager->clearConnectionCache();
  QNetworkRequest handshakeRequest;
  handshakeRequest.setTransferTimeout(DEFAULT_CONNECTION_TIMEOUT_MS);
  handshakeRequest.setUrl(getEndpointUrl(ENDPOINT_HANDSHAKE.data()));
  const auto reply = m_manager->get(handshakeRequest);
  // Captured here so `finished` can tell "could not connect" apart from "this
  // certificate has never been confirmed", which look identical from the outside.
  const auto pendingLeaf = std::make_shared<QSslCertificate>();
  connect(reply, &QNetworkReply::sslErrors, reply,
          [reply, pendingLeaf, this](const QList<QSslError>&) {
            const auto chain = reply->sslConfiguration().peerCertificateChain();
            if (!chain.isEmpty()) {
              *pendingLeaf = chain.first();
            }
            const auto url = reply->url();
            // Silent: a dialog must not run inside the TLS callback. If this needs a
            // decision, `finished` asks once the request has unwound.
            if (confirmCertificate(url.host(), url.port(DEFAULT_DAEMON_PORT), *pendingLeaf, true)) {
              reply->ignoreSslErrors();
            }
          });
  connect(reply, &QNetworkReply::finished, [reply, pendingLeaf, this]() {
    const auto status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    const auto shook =
        status == 200 &&
        QJsonDocument::fromJson(reply->readAll()).object().value("shake").toBool(false);
    reply->deleteLater();
    if (shook) {
      m_uiLoadRetryCount = 0;
      m_lastConnectionError.clear();
      m_view->load(getDaemonUrl());
      return;
    }
    const auto failure = describeReplyError(reply);
    /*
      The probe may have failed only because this certificate has never been confirmed.
      Asking has to happen here: the page load is the other place that can prompt, and it
      never runs until this succeeds. Doing it any earlier would mean a modal dialog
      inside a TLS callback.
    */
    if (!pendingLeaf->isNull()) {
      const auto url = getDaemonUrl();
      const auto host = url.host();
      const auto port = url.port(DEFAULT_DAEMON_PORT);
      const auto decision = tls_trust::decide(host, port, *pendingLeaf, false);
      if (decision == tls_trust::Decision::AskFirstUse ||
          decision == tls_trust::Decision::AskChanged) {
        if (confirmCertificate(host, port, *pendingLeaf, false)) {
          m_uiLoadRetryCount = 0;
          loadVerifiedDaemonUi();  // pinned now, so this pass gets through
          return;
        }
        qWarning() << "Not loading" << host << ": its certificate was not trusted.";
        refuseDaemonCertificate(
            uiString("wizard.errorCertUntrusted", tr("The daemon's certificate was not trusted.")));
        return;
      }
      if (decision == tls_trust::Decision::Reject) {
        // Strict validation turned this down. Retrying cannot change the answer, so go
        // straight to the dialog instead of burning the retry budget first.
        qWarning() << "Refusing" << host << ": its certificate does not validate and"
                   << "certificate validation is enabled.";
        refuseDaemonCertificate(
            uiString("wizard.errorCertInvalid",
                     tr("The daemon's certificate does not validate, and certificate "
                        "validation is enabled.")));
        return;
      }
    }
    // A daemon that is merely slow to start looks identical to a wrong address here, so
    // reuse the same retry budget the page load itself uses before bothering the user.
    if (m_uiLoadRetryCount < MAX_UI_LOAD_RETRIES) {
      m_uiLoadRetryCount++;
      qDebug() << "Daemon handshake failed (status" << status << "), retrying" << m_uiLoadRetryCount
               << "/" << MAX_UI_LOAD_RETRIES;
      QTimer::singleShot(1500, this, [this]() { loadVerifiedDaemonUi(); });
      return;
    }
    m_uiLoadRetryCount = 0;
    m_uiLoadingStopped = true;
    m_lastConnectionError = failure;
    qWarning() << "Not loading" << getDaemonUrl().toDisplayString()
               << ": it did not respond as a CoolerControl daemon." << failure;
    displayAddressWizard();
    notifyDaemonConnectionError();
  });
}

void MainWindow::forceQuit() {
  qDebug() << "Force Quit Triggered";
  m_forceQuit = true;
  // Triggers the close event but with the forceQuit flag set
  close();
}

void MainWindow::forceRefresh() const {
  qInfo() << "Forced UI refresh";
  // Note: clearHttpCache() is intentionally NOT called here because it can
  // interfere with QWebEngineProfile's cookie state, causing session cookies
  // to be lost on page reload. The daemon's cache headers already handle asset
  // freshness correctly (no-cache for index.html, immutable for hashed assets).
  m_uiLoadingStopped = false;
  m_view->load(getDaemonUrl());
}

void MainWindow::closeEvent(QCloseEvent* event) {
  if (!m_forceQuit && m_startup && !m_uiLoadingStopped && !m_webLoadFinished) {
    // Killing the app during initialization can cause a crash
    event->ignore();
    return;
  }
  if (isVisible()) {
    m_ipc->saveWindowGeometry(saveGeometry());
  }
  if (!m_forceQuit && !m_ipc->getCloseToTray() && !offerCloseToTray()) {
    // Chose the tray. Hide now whether or not they asked us to remember it, otherwise
    // the window would simply refuse to close.
    delay(100);
    hide();
    event->ignore();
    return;
  }
  if (m_ipc->getCloseToTray() && !m_forceQuit) {
    delay(100);
    hide();
    event->ignore();
    return;
  }
  m_forceQuit = true;    // keeps from triggering disconnect notification
  m_retryTimer->stop();  // stop reconnecting if running
  emit dropConnections();
  m_ipc->syncSettings();
  event->accept();
  m_page->deleteLater();
  delay(200);
  QApplication::quit();
}

/*
  Users leave the window open because they believe closing it stops cooling control. It
  does not: coolercontrold runs as its own service either way. Correcting that once, at
  the moment it matters, is what makes the tray worth entering, and everything the tray
  saves depends on people actually using it.

  Returns true when the app should carry on quitting, false when the user chose the tray.
  Shown once ever; the choice can be changed in settings afterwards.
*/
bool MainWindow::offerCloseToTray() const {
  QSettings settings;
  if (settings.value(SETTING_CLOSE_PROMPT_SHOWN.data(), false).toBool()) {
    return true;  // asked already, and closeToTray is off, so quit
  }
  QMessageBox dialog;
  dialog.setWindowTitle(uiString("closePrompt.title", tr("Close to System Tray?")));
  dialog.setIcon(QMessageBox::Question);
  dialog.setText(uiString("closePrompt.title", tr("Close to System Tray?")));
  dialog.setInformativeText(
      uiString("closePrompt.body",
               tr("The CoolerControl daemon keeps running in the background either way, so "
                  "your cooling settings stay active. Keep the UI in the system tray for "
                  "quick access and desktop notifications, or quit it entirely.")));
  const auto trayButton = dialog.addButton(uiString("closePrompt.keepInTray", tr("Keep in Tray")),
                                           QMessageBox::AcceptRole);
  dialog.addButton(uiString("closePrompt.quit", tr("Quit")), QMessageBox::RejectRole);
  dialog.setDefaultButton(trayButton);
  QCheckBox remember(uiString("closePrompt.remember", tr("Remember my choice")));
  remember.setChecked(true);
  dialog.setCheckBox(&remember);
  dialog.exec();

  const auto keepInTray = dialog.clickedButton() == trayButton;
  settings.setValue(SETTING_CLOSE_PROMPT_SHOWN.data(), true);
  if (remember.isChecked()) {
    m_ipc->setCloseToTray(keepInTray);
  }
  return !keepInTray;
}

void MainWindow::setTranslations(const QString& translationsJson) const {
  cacheUiStrings(translationsJson);
}

QString MainWindow::daemonSettingsKey() const {
  const auto url = getDaemonUrl();
  return tls_trust::hostPort(url.host(), url.port(DEFAULT_DAEMON_PORT));
}

/*
  Stored per daemon, because the UI that pushes this list is served by the daemon itself.
  A daemon running an older UI never calls this at all, and even a current one cannot
  call it until its page has loaded, so a single shared value leaves whichever machine
  was connected last showing its sensors under the new connection. Measured against a
  4.3.1 daemon, whose bundle has no setPinnedSensors: twenty rows from another machine,
  none of which resolve to anything here.
*/
void MainWindow::setPinnedSensors(const QString& sensorsJson) const {
  QSettings settings;
  settings.remove(SETTING_PINNED_SENSORS_LEGACY.data());  // pre-4.4 shared value
  settings.beginGroup(SETTING_GROUP_PINNED_SENSORS.data());
  settings.setValue(daemonSettingsKey(), sensorsJson);
  settings.endGroup();
  qDebug() << "Cached pinned sensors for" << daemonSettingsKey();
}

/*
  Rebuilds the pinned-sensor rows and keeps their readings fresh while the menu is open.

  Values are polled rather than fetched once, so a menu left open shows live numbers.
  Polling runs only between aboutToShow and aboutToHide: nothing ticks while the menu is
  closed, and this client never subscribes to the status stream. One bulk request per
  tick covers every row, so the cost does not grow with the number of pins.
*/
void MainWindow::refreshTraySensors() {
  QSettings settings;
  settings.beginGroup(SETTING_GROUP_PINNED_SENSORS.data());
  const auto json = settings.value(daemonSettingsKey()).toString();
  settings.endGroup();
  if (json != m_builtSensorsJson) {
    buildTraySensorRows(QJsonDocument::fromJson(json.toUtf8()).array());
    m_builtSensorsJson = json;
  }
  if (m_traySensors.isEmpty()) {
    return;
  }
  m_sensorPollTicks = 0;
  pollTraySensors();  // do not wait a full interval for the first reading
  m_sensorPollTimer->start();
}

void MainWindow::stopTraySensorPolling() const { m_sensorPollTimer->stop(); }

void MainWindow::clearTraySensorReadings() const {
  for (const auto& sensor : m_traySensors) {
    sensor.action->setText(sensor.label);
  }
}

QIcon MainWindow::sensorColorIcon(const QString& color) {
  const QColor swatch(color);
  if (!swatch.isValid()) {
    return {};  // no colour set: fall back to a plain row rather than a black square
  }
  constexpr int size = 32;  // generous, so hosts that scale the icon up stay sharp
  QPixmap pixmap(size, size);
  pixmap.fill(Qt::transparent);
  QPainter painter(&pixmap);
  painter.setRenderHint(QPainter::Antialiasing);
  painter.setBrush(swatch);
  painter.setPen(Qt::NoPen);
  painter.drawEllipse(pixmap.rect().adjusted(6, 6, -6, -6));
  return {pixmap};
}

/*
  Opens the window on the sensor's own page.

  The route arrives already resolved and percent-encoded from the UI. It is still
  validated here: it crosses a process boundary and embeds channel names, and one branch
  below hands it to runJavaScript.
*/
void MainWindow::openTraySensorPage(const QString& route) {
  static const QRegularExpression safeRoute(QStringLiteral("^#?/[A-Za-z0-9/_.~%-]*$"));
  if (!safeRoute.match(route).hasMatch()) {
    qWarning() << "Ignoring a tray route that is not a plain path:" << route;
    return;
  }
  const auto fragment = route.startsWith('#') ? route : QStringLiteral("#") % route;
  setAttribute(Qt::WidgetAttribute::WA_DontShowOnScreen, false);
  if (m_page->lifecycleState() == QWebEnginePage::LifecycleState::Active) {
    m_page->runJavaScript(QStringLiteral("location.hash = '%1'").arg(fragment));
  } else {
    // Discarded: reactivating reloads anyway, so load straight to the target instead of
    // landing on the home page and navigating a second time.
    m_page->setLifecycleState(QWebEnginePage::LifecycleState::Active);
    m_reloadOnShow = false;
    m_uiLoadingStopped = false;
    m_view->load(QUrl(getDaemonUrl().toString() % "/" % fragment));
  }
  showNormal();
  raise();
  activateWindow();
}

void MainWindow::buildTraySensorRows(const QJsonArray& sensors) {
  for (const auto& sensor : m_traySensors) {
    delete sensor.action;  // removes it from whichever menu holds it
  }
  m_traySensors.clear();

  // Past a handful, rows crowd out Modes/Show/Quit, so give them their own submenu.
  const auto useSubmenu = sensors.size() > TRAY_SENSORS_INLINE_MAX;
  m_sensorsTrayMenu->setTitle(uiString("tray.sensors", tr("Sensors")));
  m_sensorsTrayMenu->menuAction()->setVisible(useSubmenu && !sensors.isEmpty());

  for (const auto sensorValue : sensors) {
    const auto sensor = sensorValue.toObject();
    const auto label = sensor.value("label").toString();
    const auto action = new QAction(label, useSubmenu ? m_sensorsTrayMenu : m_trayIconMenu);
    // A colour swatch, matching the Monitoring panel, so a row reads as that sensor
    // rather than as a disabled menu entry.
    action->setIcon(sensorColorIcon(sensor.value("color").toString()));
    const auto route = sensor.value("route").toString();
    if (route.isEmpty()) {
      action->setEnabled(false);
    } else {
      connect(action, &QAction::triggered, this, [this, route]() { openTraySensorPage(route); });
    }
    if (useSubmenu) {
      m_sensorsTrayMenu->addAction(action);
    } else {
      m_trayIconMenu->insertAction(m_sensorsTrayMenu->menuAction(), action);
    }
    m_traySensors.append(TraySensor{sensor.value("deviceUid").toString(),
                                    sensor.value("channelName").toString(), label,
                                    sensor.value("unit").toString(), action});
  }
}

// Formats whichever fields the daemon reported for this channel. A fan reports both rpm
// and duty, a PSU rail only watts, a temp only its value, so nothing can be assumed.
// The daemon reports every speed value as rpm; `unit` is what the channel's label says
// it really is, and is empty for an actual fan speed.
static QString formatSensorReading(const QJsonObject& status, const QString& channelName,
                                   const QString& unit) {
  QStringList parts;
  for (const auto tempValue : status.value("temps").toArray()) {
    const auto temp = tempValue.toObject();
    if (temp.value("name").toString() != channelName) {
      continue;
    }
    parts << QString::number(temp.value("temp").toDouble(), 'f', 1) % " °C";
  }
  for (const auto channelValue : status.value("channels").toArray()) {
    const auto channel = channelValue.toObject();
    if (channel.value("name").toString() != channelName) {
      continue;
    }
    if (channel.contains("rpm")) {
      parts << QString::number(channel.value("rpm").toInt()) % " " %
                   (unit.isEmpty() ? QStringLiteral("RPM") : unit);
    }
    if (channel.contains("duty")) {
      parts << QString::number(channel.value("duty").toDouble(), 'f', 0) % "%";
    }
    if (channel.contains("watts")) {
      parts << QString::number(channel.value("watts").toDouble(), 'f', 1) % " W";
    }
    if (channel.contains("freq")) {
      parts << QString::number(channel.value("freq").toInt()) % " MHz";
    }
  }
  return parts.join(QStringLiteral("  "));
}

void MainWindow::pollTraySensors() const {
  if (++m_sensorPollTicks > TRAY_SENSOR_POLL_MAX_TICKS) {
    stopTraySensorPolling();
    return;
  }
  QNetworkRequest request;
  request.setTransferTimeout(DEFAULT_CONNECTION_TIMEOUT_MS);
  request.setUrl(getEndpointUrl(ENDPOINT_STATUS.data()));
  request.setHeader(QNetworkRequest::ContentTypeHeader, "application/json");
  applyAuth(request);
  // An empty body asks for the most recent status only, a few KB for every device.
  const auto reply = m_manager->post(request, QByteArray("{}"));
  applyTlsPolicy(reply);
  connect(reply, &QNetworkReply::finished, [reply, this]() {
    if (reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt() >= 300) {
      reply->deleteLater();
      return;
    }
    const auto devices =
        QJsonDocument::fromJson(reply->readAll()).object().value("devices").toArray();
    for (const auto& sensor : m_traySensors) {
      for (const auto deviceValue : devices) {
        const auto device = deviceValue.toObject();
        if (device.value("uid").toString() != sensor.deviceUid) {
          continue;
        }
        const auto history = device.value("status_history").toArray();
        if (history.isEmpty()) {
          break;
        }
        const auto reading =
            formatSensorReading(history.last().toObject(), sensor.channelName, sensor.unit);
        if (!reading.isEmpty()) {
          sensor.action->setText(sensor.label % "   " % reading);
        }
        break;
      }
    }
    reply->deleteLater();
  });
}

/*
  Trust decision for a daemon certificate, plus the prompt when one is needed.

  The daemon is self-signed by default, so refusing everything unsigned by a CA would
  break every install. Loopback is accepted silently; a remote daemon is pinned on first
  use after the user confirms, and a pin that later stops matching asks again with
  different wording, because that is the case worth interrupting for.
*/
bool MainWindow::confirmCertificate(const QString& host, const int port,
                                    const QSslCertificate& leaf, const bool silent) {
  const auto decision = tls_trust::decide(host, port, leaf, false);
  if (decision == tls_trust::Decision::Accept) {
    return true;
  }
  if (decision == tls_trust::Decision::Reject) {
    qWarning() << "Refusing the certificate for" << host << "(strict TLS is enabled)";
    return false;
  }
  if (silent) {
    // Background requests must not raise dialogs. They fail until a page load has
    // established the pin, and the retry timer picks them up afterwards.
    return false;
  }
  const auto changed = decision == tls_trust::Decision::AskChanged;
  const auto print = tls_trust::fingerprint(leaf);
  QMessageBox dialog;
  dialog.setIcon(changed ? QMessageBox::Warning : QMessageBox::Question);
  dialog.setWindowTitle(changed ? uiString("cert.changedTitle", tr("Certificate Changed"))
                                : uiString("cert.title", tr("Unverified Daemon Certificate")));
  dialog.setText(changed ? uiString("cert.changedTitle", tr("Certificate Changed"))
                         : uiString("cert.title", tr("Unverified Daemon Certificate")));
  const auto body =
      changed
          ? uiString("cert.changedBody",
                     tr("The certificate for %1 is not the one previously trusted. This "
                        "can mean the daemon was reinstalled, or that something is "
                        "intercepting the connection."))
          : uiString("cert.body", tr("%1 uses a self-signed certificate, which cannot be verified "
                                     "automatically. Continue only if you recognise this daemon."));
  dialog.setInformativeText(body.arg(host) % "\n\n" %
                            uiString("cert.fingerprint", tr("Fingerprint (SHA-256):")) % "\n" %
                            print);
  const auto trustButton = dialog.addButton(uiString("cert.trust", tr("Trust This Certificate")),
                                            QMessageBox::AcceptRole);
  dialog.addButton(uiString("cert.cancel", tr("Cancel")), QMessageBox::RejectRole);
  dialog.setDefaultButton(changed ? nullptr : trustButton);
  dialog.exec();
  if (dialog.clickedButton() != trustButton) {
    qWarning() << "User declined the certificate for" << host;
    return false;
  }
  tls_trust::storePin(host, port, print);
  qInfo() << "Pinned the daemon certificate for" << host;
  return true;
}

/*
  Every request used to call ignoreSslErrors() unconditionally, which meant the session
  cookie and the access token travelled over a channel nothing verified. They now consult
  the same policy as the page load, silently: a background request cannot raise a dialog,
  so it fails until a page load has pinned the certificate.
*/
void MainWindow::applyTlsPolicy(QNetworkReply* reply) const {
  connect(reply, &QNetworkReply::sslErrors, reply, [reply, this](const QList<QSslError>& errors) {
    const auto chain = reply->sslConfiguration().peerCertificateChain();
    const auto leaf = chain.isEmpty() ? QSslCertificate() : chain.first();
    const auto url = reply->url();
    if (const_cast<MainWindow*>(this)->confirmCertificate(url.host(), url.port(DEFAULT_DAEMON_PORT),
                                                          leaf, true)) {
      reply->ignoreSslErrors();
      return;
    }
    qWarning() << "Refusing" << url.toDisplayString() << "over an unverified"
               << "connection:" << errors.size() << "TLS error(s)";
  });
}

void MainWindow::setDiscardEnabled(const bool enabled) {
  m_discardEnabled = enabled;
  if (!enabled) {
    qInfo() << "Renderer discarding disabled.";
  }
}

/*
  The renderer is by far the largest part of this app's footprint, and nothing is
  looking at it while the window sits in the tray. Discarding tears that process down
  and keeps the page object, which reloads itself when reactivated.

  This is close to free: the UI already reloads on show, because DeviceStore treats
  being hidden past a threshold as reason enough to refetch rather than reconcile a
  stale status history. So the reload was already being paid; only the idle renderer
  was extra.
*/
void MainWindow::discardPage() const {
  if (!m_discardEnabled || isVisible()) {
    return;
  }
  if (m_page->lifecycleState() == QWebEnginePage::LifecycleState::Discarded) {
    return;
  }
  // recommendedState is an advisory ceiling on how aggressive to be, not a constraint.
  // Qt recommends Frozen for a merely hidden page and only reaches for Discarded under
  // memory pressure, so gating on it would mean never discarding at all. Exceeding the
  // recommendation is explicitly supported; the documented cost is losing transient page
  // state, which this UI already throws away on show.
  m_page->setLifecycleState(QWebEnginePage::LifecycleState::Discarded);
  qInfo() << "Renderer discarded while in the tray.";
}

void MainWindow::restorePage() const {
  if (m_page->lifecycleState() != QWebEnginePage::LifecycleState::Active) {
    m_page->setLifecycleState(QWebEnginePage::LifecycleState::Active);
    m_reloadOnShow = false;  // reactivation reloads the page by itself
    return;
  }
  if (m_reloadOnShow) {
    m_reloadOnShow = false;
    m_uiLoadingStopped = false;
    m_view->load(getDaemonUrl());
  }
}

void MainWindow::hideEvent(QHideEvent* event) {
  if (m_startup && !m_webLoadFinished) {
    // opening/closing the window during initialization can cause issues.
    event->ignore();
    return;
  }
  delay(100);
  setTrayActionToShow();
  m_discardTimer->start();
  event->accept();
}

void MainWindow::showEvent(QShowEvent* event) {
  if (m_startup) {
    // opening/closing the window during initialization can cause issues.
    event->ignore();
    return;
  }
  m_discardTimer->stop();
  restorePage();
  delay(100);
  setTrayActionToHide();
  event->accept();
}

QUrl MainWindow::getDaemonUrl(const bool forceHttp) {
  const auto connection = connections::current();
  // Not tr(): a translated URL scheme would break every request this app makes.
  const auto scheme =
      (forceHttp || !connection.sslEnabled) ? QStringLiteral("http") : QStringLiteral("https");
  QUrl url;
  url.setScheme(scheme);
  url.setHost(connection.host);
  url.setPort(connection.port);
  return url;
}

QUrl MainWindow::getEndpointUrl(const QString& endpoint, const bool forceHttp) {
  auto url = getDaemonUrl(forceHttp);
  url.setPath(endpoint);
  // for testing with npm dev server:
  // url.setPort(DEFAULT_DAEMON_PORT);
  return url;
}

/*
  The rows are built once and kept; a switch only moves the checked one. Only the
  submenu's own visibility touches the top-level menu, so hosts that cache the tray
  layout over DBusMenu see a stable one.

  Rebuilding the rows on every switch is what wedged the tray. A StatusNotifierItem host
  draws the menu from its own copy of the layout, and rows destroyed underneath it stay
  on screen addressing ids the app no longer has: the second switch showed two daemons
  checked at once, which this function cannot produce (one liveIndex, exclusive group),
  and its rows then did nothing when clicked. So the row set is only torn down when the
  saved list actually changes length, the way the Modes submenu keeps its rows and just
  flips `checked` in setActiveMode.
*/
void MainWindow::rebuildDaemonsTrayMenu() {
  const auto list = connections::all();
  m_daemonsTrayMenu->menuAction()->setVisible(list.size() > 1);
  auto rows = m_daemonsTrayMenu->actions();
  if (rows.size() != list.size()) {
    m_daemonsTrayMenu->clear();
    // The rows are parented to the group, so this is what frees the previous ones. It
    // has to be deleteLater: this can run from one of those rows' own triggered handler,
    // and deleting it outright would pull the object out from under the running signal.
    if (m_daemonsActionGroup != nullptr) {
      m_daemonsActionGroup->deleteLater();
    }
    m_daemonsActionGroup = new QActionGroup(m_daemonsTrayMenu);
    m_daemonsActionGroup->setExclusive(true);
    for (auto i = 0; i < list.size(); ++i) {
      const auto action = new QAction(m_daemonsActionGroup);
      action->setCheckable(true);
      // The row reads its connection when clicked instead of capturing one. Rows now
      // outlive edits to the saved list, and a captured copy would point at the entry
      // that sat there when the row was made.
      connect(action, &QAction::triggered, this, [this, i]() {
        const auto saved = connections::all();
        if (i < saved.size()) {
          applyDaemonConnection(saved.at(i));
        }
      });
      m_daemonsTrayMenu->addAction(action);
    }
    rows = m_daemonsTrayMenu->actions();
  }
  const auto liveIndex = connections::currentIndex();
  for (auto i = 0; i < list.size(); ++i) {
    rows.at(i)->setText(connections::displayName(list.at(i)));
    rows.at(i)->setChecked(i == liveIndex);
  }
}

/*
  Points the app at a saved daemon.

  The window is brought up rather than left in the tray on purpose. The tray's modes,
  pinned sensors and alert badge are all pushed by the UI over IPC, so until the new
  daemon's page has loaded there is nothing for the tray to show. Connecting without
  showing would leave it half empty with no way to tell why.
*/
void MainWindow::applyDaemonConnection(const connections::Connection connection) {
  connections::upsert(connection);  // the live daemon is always a saved one
  if (connections::sameConnection(connection, connections::current())) {
    rebuildDaemonsTrayMenu();  // re-selection changes nothing, a rename does
    return;
  }
  qInfo() << "Switching to daemon" << connections::displayName(connection);
  connections::setCurrent(connection);
  // Stop before yielding: delay() spins a nested event loop, and a retry tick inside it
  // probes the new address while the old per-daemon state is still live. A 200 took the
  // reconnect branch, announced a false "Connection Restored" and opened an SSE stream
  // that the startup path below then opened a second time.
  m_retryTimer->stop();
  m_changeAddress = true;
  emit dropConnections();
  delay(300);  // give signals a moment to process.
  m_changeAddress = false;
  resetPerDaemonState();
  setAttribute(Qt::WidgetAttribute::WA_DontShowOnScreen, false);
  // Reactivating a discarded page reloads it at its old address, so it is blanked before
  // the new origin is installed. Otherwise the old daemon gets one request that the
  // origin filter then refuses, leaving an error page on screen.
  restorePage();
  m_view->load(QUrl(QStringLiteral("about:blank")));
  showNormal();
  raise();
  activateWindow();
  // showEvent skips its own work while m_startup is set, so the tray label is set here.
  setTrayActionToHide();
  m_originFilter->setDaemonUrl(getDaemonUrl());  // the address just changed
  loadVerifiedDaemonUi();
  rebuildDaemonsTrayMenu();  // moves the checked row
}

/*
  Pushes the desktop's colors to the UI, which uses them only while its theme is
  set to System. Sent rather than pulled because the accent and the light/dark
  preference can change while the app is running, and compared first because the
  portal announces every appearance key, most of which are not colors.
*/
void MainWindow::refreshSystemPalette() {
  const auto palette = system_palette::readAsJson();
  if (palette == m_systemPaletteJson) {
    return;
  }
  m_systemPaletteJson = palette;
  emit m_ipc->systemPaletteChanged(palette);
}

void MainWindow::resetPerDaemonState() {
  m_accessToken.clear();
  loadAccessToken();  // the new daemon's own, under its own key
  m_activeModeUID.clear();
  m_modesTrayMenu->clear();
  m_modesTrayMenu->setEnabled(false);
  stopTraySensorPolling();
  m_sensorPollTicks = 0;
  clearTraySensorReadings();
  m_daemonHasErrors = false;
  m_daemonHasWarnings = false;
  m_uiAlertsActive = false;
  applyTrayIconNotificationBadge();
  m_disconnectNotified = false;
  m_disconnectedFor.invalidate();
  m_sseRetryDelayMs = 0;
  m_loginWindowShown = false;
  m_uiLoadRetryCount = 0;
  m_lastConnectionError.clear();
  m_uiLoadingStopped = false;
  m_reloadOnShow = false;
  m_startup = true;
  // Re-arming m_startup without this leaves the guards that pair the two dead for the new
  // daemon's load: hideEvent and closeEvent both test m_startup && !m_webLoadFinished, and
  // the flag is only ever set true once the first page finishes.
  m_webLoadFinished = false;
}

void MainWindow::displayAddressWizard() const {
  if (m_wizard->isVisible()) {
    return;
  }
  // The page is built once with the window, so it would otherwise still show whatever
  // was configured at startup.
  m_addressPage->reload();
  // Both pages, because the wizard reopens on whichever one was last shown, and a
  // failure that follows Apply lands the user back on the address page.
  m_introPage->setError(m_lastConnectionError);
  m_addressPage->setError(m_lastConnectionError);
  // Opened from the tray with nothing wrong, "Error" is simply untrue.
  m_wizard->setWindowTitle(m_lastConnectionError.isEmpty()
                               ? uiString("wizard.windowTitleOk", tr("Daemon Connection"))
                               : uiString("wizard.windowTitle", tr("Daemon Connection Error")));
  m_wizard->open();
}

QString MainWindow::describeReplyError(const QNetworkReply* reply) {
  if (reply->error() != QNetworkReply::NoError) {
    return reply->errorString();  // "Connection refused", "Host not found", and so on
  }
  const auto status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
  return uiString("wizard.errorNotDaemon",
                  tr("The address answered, but not as a CoolerControl daemon (HTTP %1)."))
      .arg(status);
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
          m_webLoadFinished = true;
          hide();
          setAttribute(Qt::WidgetAttribute::WA_DontShowOnScreen, false);
          qInfo() << "Initialized closed to system tray.";
        },
        Qt::SingleShotConnection);
  } else {
    show();
    connect(
        m_ipc, &IPC::webLoadFinished, this,
        [this]() {
          m_webLoadFinished = true;
          qInfo() << "Initialized open window.";
        },
        Qt::SingleShotConnection);
  }
}

void MainWindow::setZoomFactor(const double zoomFactor) const { m_view->setZoomFactor(zoomFactor); }

void MainWindow::delay(const int millisecondsWait) {
  QEventLoop loop;
  QTimer t;
  connect(&t, &QTimer::timeout, &loop, &QEventLoop::quit);
  t.start(millisecondsWait);
  loop.exec();
}

void MainWindow::setTrayActionToShow() const {
  m_showAction->setIcon(showIcon());
  m_showAction->setText(uiString("tray.show", tr("&Show")));
}

void MainWindow::setTrayActionToHide() const {
  m_showAction->setIcon(hideIcon());
  m_showAction->setText(uiString("tray.hide", tr("&Hide")));
}

void MainWindow::showVersionMismatchDialog(const QString& daemonVersion) const {
  const auto appVersion = QString::fromStdString(COOLER_CONTROL_VERSION);
  QMessageBox dialog;
  dialog.setWindowTitle(uiString("versionMismatch.title", tr("Version Mismatch")));
  dialog.setIcon(QMessageBox::Warning);
  dialog.setText(
      uiString("versionMismatch.text", tr("The desktop app version (%1) does not match the daemon "
                                          "version (%2)."))
          .arg(appVersion, daemonVersion));
  dialog.setInformativeText(
      uiString("versionMismatch.informative",
               tr("Please restart the desktop app to load the correct interface version.")));
  const auto quitButton = dialog.addButton(uiString("versionMismatch.quitApp", tr("&Quit App")),
                                           QMessageBox::AcceptRole);
  dialog.addButton(uiString("versionMismatch.continueAnyway", tr("Continue Anyway")),
                   QMessageBox::RejectRole);
  dialog.setDefaultButton(quitButton);
  dialog.exec();
  if (dialog.clickedButton() == quitButton) {
    m_forceQuit = true;
    QApplication::quit();
  }
}

void MainWindow::notifyDaemonConnectionError() {
  // Qt has some issues around message icons, and we now use DBus notifications
  // now directly to handle the important ones better.
  Notifier::send("Daemon Connection Error", "Connection with the daemon could not be established",
                 1);
}

void MainWindow::notifyDaemonErrors() {
  Notifier::send("Daemon Errors", "The daemon logs contain errors. You should investigate.", 4);
}

void MainWindow::notifyDaemonDisconnected() {
  Notifier::send("Daemon Disconnected", "Connection with the daemon has been lost", 1);
}

void MainWindow::notifyDaemonConnectionRestored() {
  Notifier::send("Daemon Connection Restored", "Connection with the daemon has been restored.", 2);
}

QIcon MainWindow::createIconWithNotificationBadge(const QIcon& baseIcon, const bool redColor) {
  // This doesn't work very well with newer Gnome and symbolic icons. Using actual icons.
  constexpr int iconSize = 64;
  QPixmap pixmap = baseIcon.pixmap(iconSize, iconSize);
  QPainter painter(&pixmap);
  painter.setRenderHint(QPainter::Antialiasing);

  constexpr int dotSize = 20;
  constexpr int x = (iconSize / 2) - (dotSize / 2);
  constexpr int y = iconSize - (dotSize + 6);

  // Bootstrap danger red or warning yellow
  painter.setBrush(redColor ? QColor(220, 53, 69) : QColor(255, 193, 7));
  painter.setPen(QPen(redColor ? QColor(180, 40, 50) : QColor(215, 165, 0), 2));
  painter.drawEllipse(x, y, dotSize, dotSize);

  return (pixmap);
}

void MainWindow::applyTrayIconNotificationBadge(const bool forceBadge) const {
  if (forceBadge || m_daemonHasErrors || m_daemonHasWarnings || m_uiAlertsActive) {
    m_sysTrayIcon->setIcon(QIcon::fromTheme(
        APP_ID_ALERT_SYMBOLIC.data(),
        QIcon::fromTheme(
            APP_ID_ALERT_SYMBOLIC_LEGACY.data(),
            QIcon::fromTheme(
                APP_ID_ALERT.data(),
                QIcon(":/icons/org.coolercontrol.CoolerControl-alert-symbolic.svg")))));
  } else {
    m_sysTrayIcon->setIcon(QIcon::fromTheme(
        APP_ID_SYMBOLIC.data(),
        QIcon::fromTheme(APP_ID.data(),
                         QIcon(":/icons/org.coolercontrol.CoolerControl-symbolic.svg"))));
  }
}

void MainWindow::loadAccessToken() const {
  const auto daemonKey = daemonSettingsKey();
  QSettings settings;
  m_accessToken = settings.value(accessTokenKey(daemonKey)).toByteArray();
  if (m_accessToken.isEmpty()) {
    // Pre-4.4 kept one token for whichever daemon was configured. Adopt it for that
    // daemon and drop the old keys in the same breath: with them gone this cannot fire
    // again later and hand one daemon's token to another.
    if (const auto legacy = settings.value(SETTING_ACCESS_TOKEN.data()).toByteArray();
        !legacy.isEmpty()) {
      m_accessToken = legacy;
      settings.setValue(accessTokenKey(daemonKey), legacy);
      settings.setValue(accessTokenIdKey(daemonKey),
                        settings.value(SETTING_ACCESS_TOKEN_ID.data()).toString());
      settings.remove(SETTING_ACCESS_TOKEN.data());
      settings.remove(SETTING_ACCESS_TOKEN_ID.data());
      settings.sync();
      restrictSettingsFilePermissions(settings);
      qInfo() << "Adopted the stored access token for" << daemonKey;
    }
  }
  if (!m_accessToken.isEmpty()) {
    qInfo() << "Loaded stored daemon access token.";
  }
}

void MainWindow::applyAuth(QNetworkRequest& request) const {
  if (m_accessToken.isEmpty()) {
    return;  // fall back to the session cookie synced from the web engine
  }
  request.setRawHeader("Authorization", "Bearer " + m_accessToken);
}

void MainWindow::clearAccessToken() const {
  if (m_accessToken.isEmpty()) {
    return;
  }
  qWarning() << "Daemon rejected the stored access token. Clearing it.";
  m_accessToken.clear();
  QSettings settings;
  settings.remove(accessTokenKey(daemonSettingsKey()));
  // The id is deliberately kept so the next provision can delete the dead token
  // server-side. Deleting it here is not possible: /tokens is session-only, and a
  // rejected token usually means there is no valid session to delete it with either.
  settings.sync();
  restrictSettingsFilePermissions(settings);  // the cookie is still in this file
}

// /tokens is session-only, so this must run while the session cookie is valid and must
// not carry the bearer header.
void MainWindow::deleteAccessToken(const QString& tokenId) const {
  if (tokenId.isEmpty()) {
    return;
  }
  QNetworkRequest deleteRequest;
  deleteRequest.setTransferTimeout(DEFAULT_CONNECTION_TIMEOUT_MS);
  auto url = getEndpointUrl(ENDPOINT_TOKENS.data());
  url.setPath(url.path() + "/" + tokenId);
  deleteRequest.setUrl(url);
  const auto deleteReply = m_manager->deleteResource(deleteRequest);
  applyTlsPolicy(deleteReply);
  connect(deleteReply, &QNetworkReply::finished, [deleteReply, tokenId]() {
    const auto status = deleteReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    // A 404 is fine and expected when the user already revoked it in the UI.
    qDebug() << "Removed superseded desktop access token" << tokenId << "status:" << status;
    deleteReply->deleteLater();
  });
}

/*
  Mints this app its own bearer token from an authenticated session, so the tray's
  alerts, modes and notifications survive a daemon restart or a torn-down renderer.
  Until this exists, the app's only credential is the session cookie synced out of
  QWebEngineProfile, which means the tray silently stops working whenever the page
  is not there to re-authenticate.

  /tokens is session-only, so this can only run while the cookie is valid.
*/
void MainWindow::provisionAccessToken() const {
  if (!m_accessToken.isEmpty()) {
    return;
  }
  // Captured by value: a switch mid-flight must not file this token under whichever
  // daemon happens to be live when the reply lands.
  const auto daemonKey = daemonSettingsKey();
  // Left behind by a previous token this daemon rejected. Deleted once the replacement
  // exists, so a revoke-and-reconnect cycle cannot accumulate dead entries.
  const auto supersededId = QSettings().value(accessTokenIdKey(daemonKey)).toString();
  QNetworkRequest tokenRequest;
  tokenRequest.setTransferTimeout(DEFAULT_CONNECTION_TIMEOUT_MS);
  tokenRequest.setUrl(getEndpointUrl(ENDPOINT_TOKENS.data()));
  tokenRequest.setHeader(QNetworkRequest::ContentTypeHeader, "application/json");
  QJsonObject body;
  body.insert("label", QString::fromStdString(ACCESS_TOKEN_LABEL));
  body.insert("write_access", true);  // the tray activates Modes
  const auto tokenReply =
      m_manager->post(tokenRequest, QJsonDocument(body).toJson(QJsonDocument::Compact));
  applyTlsPolicy(tokenReply);
  connect(tokenReply, &QNetworkReply::finished, [tokenReply, supersededId, daemonKey, this]() {
    const auto status = tokenReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    if (status >= 300) {
      // Not fatal: the session cookie still works while the renderer is alive, and
      // this is retried on the next successful connection.
      qWarning() << "Could not create a desktop access token. Status: " << status;
      tokenReply->deleteLater();
      return;
    }
    const QJsonObject rootObj = QJsonDocument::fromJson(tokenReply->readAll()).object();
    const auto token = rootObj.value("token").toString();
    if (token.isEmpty()) {
      qWarning() << "Access token response contained no token.";
      tokenReply->deleteLater();
      return;
    }
    m_accessToken = token.toUtf8();
    QSettings settings;
    settings.setValue(accessTokenKey(daemonKey), m_accessToken);
    settings.setValue(accessTokenIdKey(daemonKey), rootObj.value("id").toString());
    settings.sync();
    restrictSettingsFilePermissions(settings);
    qInfo() << "Created a desktop access token for daemon requests.";
    if (supersededId != rootObj.value("id").toString()) {
      deleteAccessToken(supersededId);
    }
    tokenReply->deleteLater();
  });
}

void MainWindow::requestDaemonErrors() const {
  QNetworkRequest healthRequest;
  healthRequest.setTransferTimeout(DEFAULT_CONNECTION_TIMEOUT_MS);
  healthRequest.setUrl(getEndpointUrl(ENDPOINT_HEALTH.data()));
  applyAuth(healthRequest);
  const auto healthReply = m_manager->get(healthRequest);
  applyTlsPolicy(healthReply);
  connect(healthReply, &QNetworkReply::finished, [healthReply, this]() {
    const auto status = healthReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    if (status == 401) {
      qDebug() << "Health endpoint returned 401 - not yet authenticated.";
      healthReply->deleteLater();
      return;
    }
    if (healthReply->error() != QNetworkReply::NoError) {
      healthReply->deleteLater();
      return;  // finished fires on failure too; there is no body to read
    }
    const QString replyText = healthReply->readAll();
    qDebug() << "Health Endpoint Response Status: " << status << "; Body: " << replyText;
    const QJsonObject rootObj = QJsonDocument::fromJson(replyText.toUtf8()).object();
    const auto daemonVersion = rootObj.value("details").toObject().value("version").toString();
    if (daemonVersion.isEmpty()) {
      qWarning() << "Health version response is empty - must NOT be connected to the daemon API.";
    } else {
      const auto appVersion = QString::fromStdString(COOLER_CONTROL_VERSION);
      if (daemonVersion != appVersion) {
        showVersionMismatchDialog(daemonVersion);
      }
    }
    if (const auto errors = rootObj.value("details").toObject().value("errors").toInt();
        errors > 0) {
      m_daemonHasErrors = true;
      notifyDaemonErrors();
    }
    if (const auto warnings = rootObj.value("details").toObject().value("warnings").toInt();
        warnings > 0) {
      m_daemonHasWarnings = true;
    }
    applyTrayIconNotificationBadge();
    healthReply->deleteLater();
  });
  connect(healthReply, &QNetworkReply::errorOccurred,
          [healthReply](const QNetworkReply::NetworkError code) {
            const auto status =
                healthReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
            qWarning() << "Error occurred connecting to Daemon Health endpoint. Status: " << status
                       << " QtErrorCode: " << code;
            // finished always follows, and it owns the cleanup.
          });
}

void MainWindow::acknowledgeDaemonErrors() const {
  m_daemonHasErrors = false;
  m_daemonHasWarnings = false;
  applyTrayIconNotificationBadge();
}

// Reflects the UI's alert state on the tray badge. The UI is the single source of
// truth and pushes this over IPC whenever the set of enabled, active, unsilenced
// alerts changes; silencing/disabling happen in the UI and never reach the wire.
void MainWindow::setAlertsActive(const bool active) const {
  m_uiAlertsActive = active;
  applyTrayIconNotificationBadge();
}

void MainWindow::requestAllModes() const {
  QNetworkRequest modesRequest;
  modesRequest.setTransferTimeout(DEFAULT_CONNECTION_TIMEOUT_MS);
  modesRequest.setUrl(getEndpointUrl(ENDPOINT_MODES.data()));
  applyAuth(modesRequest);
  const auto modesReply = m_manager->get(modesRequest);
  applyTlsPolicy(modesReply);
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
      applyAuth(setModeRequest);
      const auto setModeReply = m_manager->post(setModeRequest, QByteArray());
      applyTlsPolicy(setModeReply);
      connect(setModeReply, &QNetworkReply::finished, [setModeReply, this]() {
        const auto status =
            setModeReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        if (status == 401) {
          clearAccessToken();
          // Goes through the window-show path rather than m_view->showNormal(): calling
          // it on the child view never brought the window up, so a credentials error
          // raised from the tray left the user with nothing to log in to.
          emit m_ipc->forceWindowShow();
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
  applyAuth(modesActiveRequest);
  const auto modesActiveReply = m_manager->get(modesActiveRequest);
  applyTlsPolicy(modesActiveReply);
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

void MainWindow::startWatchingSSE() const { watchDaemonEvents(); }

/*
  One connection for the three event kinds this client needs. The daemon multiplexes
  them onto /sse and tags each with its own event name; subscribing narrowly keeps the
  once-per-poll status ticks off a connection that has no use for them.
*/
void MainWindow::watchDaemonEvents() const {
  QNetworkRequest sseRequest;
  sseRequest.setAttribute(QNetworkRequest::CacheLoadControlAttribute,
                          QNetworkRequest::AlwaysNetwork);
  auto sseUrl = getEndpointUrl(ENDPOINT_SSE.data(), false);
  sseUrl.setQuery(SSE_EVENTS_QUERY.data());
  sseRequest.setUrl(sseUrl);
  applyAuth(sseRequest);
  const auto sseReply = m_manager->get(sseRequest);
  applyTlsPolicy(sseReply);
  connect(this, &MainWindow::dropConnections, sseReply, &QNetworkReply::abort,
          Qt::DirectConnection);
  // One parser per connection, so a reconnect never resumes on a half-read frame.
  const auto parser = std::make_shared<SseParser>();
  connect(sseReply, &QNetworkReply::readyRead, [sseReply, parser, this]() {
    m_sseRetryDelayMs = 0;  // data flowed, so any earlier backoff is spent
    parser->feed(sseReply->readAll(), [this](const QString& name, const QString& data) {
      if (name == "log") {
        handleLogEvent(data);
      } else if (name == "mode") {
        handleModeEvent(data);
      } else if (name == "notification") {
        handleNotificationEvent(data);
      }
    });
  });
  connect(sseReply, &QNetworkReply::finished, [this, sseReply]() {
    const auto status = sseReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    qDebug() << "Daemon Event SSE closed with status: " << status;
    if (status == 401) {
      /*
        The token was refused, so drop it and restart the health probe: that path
        re-provisions a token and re-opens this stream once it succeeds. Without the
        restart nothing ever runs again, because the retry timer was stopped by the
        last successful probe, and tray modes, mode activation and daemon
        notifications all go silent for the rest of the process lifetime.
        No daemonConnectionLost here: the daemon answered, so nothing is lost, which
        is the same reasoning the >= 400 branch below documents.
      */
      clearAccessToken();
      qDebug() << "Daemon Event SSE returned 401 - retrying after re-authentication.";
      if (!m_forceQuit && !m_changeAddress) {
        m_retryTimer->start();
      }
      sseReply->deleteLater();
      return;
    }
    if (m_forceQuit || m_changeAddress) {
      sseReply->deleteLater();
      return;
    }
    if (status >= 400) {
      /*
        The daemon answered and turned this route down, so the connection is not lost and
        reporting one would be wrong. Measured against a daemon whose /sse predates the
        multiplexed endpoint: health kept returning 200, the stream kept returning 404,
        and the two second retry became an endless stream of paired disconnect and
        reconnect notifications. Back off quietly instead. Everything else still works;
        only the tray's own live updates are degraded.
      */
      if (m_sseRetryDelayMs == 0) {
        qWarning() << "Daemon event stream refused with status" << status
                   << "- tray mode and notification updates will not be live.";
        m_sseRetryDelayMs = SSE_RETRY_BASE_MS;
      } else {
        m_sseRetryDelayMs = std::min(m_sseRetryDelayMs * 2, SSE_RETRY_MAX_MS);
      }
      QTimer::singleShot(m_sseRetryDelayMs, this, [this]() { emit watchForSSE(); });
      sseReply->deleteLater();
      return;
    }
    // on error or dropped connection will be re-connected once connection is re-established.
    // Reconnection is unconditional: gating it could strand the app offline. Only the
    // notification is gated, so a second stream closing cannot report the same loss twice.
    // Starting the clock rather than notifying here is what keeps a mobile blip quiet: the
    // reconnect below still starts immediately, and nothing is reported until a health
    // probe has failed with the clock past the grace period. isValid() keeps a second
    // stream closing from restarting it and pushing the notification further out.
    if (!m_disconnectNotified && !m_disconnectedFor.isValid()) {
      m_disconnectedFor.start();
    }
    emit daemonConnectionLost();
    sseReply->deleteLater();
  });
}

void MainWindow::handleLogEvent(const QString& log) const {
  if (const auto logContainsErrors = log.contains("ERROR");
      logContainsErrors && !m_daemonHasErrors) {
    m_daemonHasErrors = true;
    notifyDaemonErrors();
  }
  if (const auto logContainsWarnings = log.contains("WARN");
      logContainsWarnings && !m_daemonHasWarnings) {
    m_daemonHasWarnings = true;
  }
}

/*
  The outage outlived the grace period, so tell the user.

  Only ever called from a health probe that just failed. A free-running timer was tried
  first and raced the recovery it was waiting on: a daemon taking about the grace period
  to come back produced the timeout and the successful probe in the same instant, so a
  restart fired "disconnected" and "restored" together. Deciding from a failed probe
  means a recovery in flight always wins.

  Everything the drop used to do immediately happens here instead: the tray readings are
  blanked at the same moment, because a reading that is stale by this long is the same
  problem the notification reports, and clearing it on every brief drop only made the
  tray flicker.
*/
void MainWindow::confirmDaemonLossIfOverdue() const {
  if (m_disconnectNotified || !m_disconnectedFor.isValid() ||
      !m_disconnectedFor.hasExpired(DAEMON_DISCONNECT_NOTIFY_DELAY_MS)) {
    return;
  }
  m_disconnectNotified = true;
  clearTraySensorReadings();
  notifyDaemonDisconnected();
  applyTrayIconNotificationBadge(true);
  qInfo() << "Connection to the Daemon Lost";
}

void MainWindow::reestablishDaemonConnection() const {
  if (m_changeAddress) {
    return;
  }
  emit dropConnections();
  m_retryTimer->start();
}

void MainWindow::tryDaemonConnection() {
  if (m_healthProbeInFlight) {
    // A probe outlives the retry interval on a slow link. Letting a second one start
    // means both can succeed and each emits watchForSSE, which is the same stacked
    // stream problem the finished-not-readyRead comment below describes.
    return;
  }
  m_healthProbeInFlight = true;
  QNetworkRequest healthRequest;
  healthRequest.setTransferTimeout(DEFAULT_CONNECTION_TIMEOUT_MS);
  healthRequest.setUrl(getEndpointUrl(ENDPOINT_HEALTH.data()));
  applyAuth(healthRequest);
  const auto healthReply = m_manager->get(healthRequest);
  applyTlsPolicy(healthReply);
  qDebug() << "Attempting to establish connection to the daemon...";
  // finished, not readyRead: readyRead fires once per chunk, and a remote daemon's
  // response often arrives in several. That re-ran this whole block, emitting
  // watchForSSE repeatedly, which opened a stack of SSE streams whose closures each
  // produced a disconnect notification. Locally the body arrives in one chunk, so it
  // only ever showed up over the network.
  connect(healthReply, &QNetworkReply::finished, [this, healthReply]() {
    m_healthProbeInFlight = false;
    const auto status = healthReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    if (status == 401) {
      // A stored token the daemon no longer accepts (revoked, or its config was
      // reset) must not keep us stuck: drop it and let the session flow take over.
      // The next successful connection provisions a fresh one.
      clearAccessToken();
      qDebug() << "Daemon connection returned 401 - waiting for authentication...";
      setAttribute(Qt::WidgetAttribute::WA_DontShowOnScreen, false);
      if (!m_loginWindowShown) {
        showNormal();  // show window once, if we have login credentials error
        m_loginWindowShown = true;
      }
      healthReply->deleteLater();
      return;
    }
    if (healthReply->error() != QNetworkReply::NoError || status != 200) {
      // Unlike readyRead, finished also fires when the request failed. Without this the
      // retry timer would stop and a "reconnected" notification would go out over a
      // connection that never came up.
      m_lastConnectionError = describeReplyError(healthReply);
      qDebug() << "Daemon connection attempt failed:" << m_lastConnectionError;
      confirmDaemonLossIfOverdue();
      healthReply->deleteLater();
      return;
    }
    m_lastConnectionError.clear();
    m_retryTimer->stop();
    // Recovered inside the grace period: the user is never told anything happened.
    // notifyDaemonConnectionRestored below is already paired to m_disconnectNotified,
    // so an unreported outage cannot leave a lone "restored" notification behind.
    m_disconnectedFor.invalidate();
    provisionAccessToken();  // no-op once we hold one
    if (m_startup) {
      requestDaemonErrors();
      requestAllModes();
      requestActiveMode();
      emit watchForSSE();
      qInfo() << "Successfully connected to the Daemon";
      m_startup = false;
    } else {
      qInfo() << "Connection to the Daemon Reestablished";
      // Also covers an address change, where the readings on screen came from the
      // machine we just stopped talking to.
      clearTraySensorReadings();
      if (m_disconnectNotified) {
        m_disconnectNotified = false;
        notifyDaemonConnectionRestored();
      }
      // reset states on reconnection
      m_daemonHasErrors = false;
      m_daemonHasWarnings = false;
      m_loginWindowShown = false;
      if (isHidden()) {
        // Refreshing a window nobody is looking at would resurrect the whole renderer,
        // in exactly the long-idle case this is meant to keep cheap. The tray no longer
        // depends on the page for its own requests, so defer the refresh to the next
        // show. A discarded page reloads on reactivation anyway, so it cannot go stale.
        m_reloadOnShow = true;
      }
      // systray badge update: daemon errors re-checked here; alerts come from the UI.
      requestDaemonErrors();
      emit watchForSSE();
    }
    healthReply->deleteLater();
  });
  connect(healthReply, &QNetworkReply::errorOccurred,
          [healthReply](const QNetworkReply::NetworkError code) {
            const auto status =
                healthReply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
            qDebug() << "Error occurred establishing connection to Daemon. Status: " << status
                     << " QtErrorCode: " << code;
            // finished always follows, and it owns the cleanup.
          });
}

void MainWindow::handleModeEvent(const QString& data) const {
  const QJsonObject rootObj = QJsonDocument::fromJson(data.toUtf8()).object();
  if (rootObj.isEmpty()) {
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
  Notifier::send(msgTitle, "", 4);
}

void MainWindow::handleNotificationEvent(const QString& data) const {
  const QJsonObject obj = QJsonDocument::fromJson(data.toUtf8()).object();
  if (obj.isEmpty()) {
    return;
  }
  const auto title = obj.value("title").toString();
  const auto body = obj.value("body").toString();
  const auto iconStr = obj.value("icon").toString();
  const auto audio = obj.value("audio").toBool();
  const auto urgency = obj.value("urgency").toInt(1);
  // Map icon string to u8 matching NotificationIcon enum values.
  int iconNum = 4;  // default: info
  if (iconStr == "triggered") {
    iconNum = 1;
  } else if (iconStr == "resolved") {
    iconNum = 2;
  } else if (iconStr == "error") {
    iconNum = 3;
  } else if (iconStr == "shutdown") {
    iconNum = 5;
  }
  Notifier::send(title, body, iconNum, audio, urgency);
}
