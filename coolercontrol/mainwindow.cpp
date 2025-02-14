#include "mainwindow.h"

#include <QWebEngineView>
#include <QSystemTrayIcon>
#include <QMenu>
#include <QAction>
#include <QApplication>
#include <QCloseEvent>
#include <QWebEngineNewWindowRequest>

#include "mywebenginepage.h"

MainWindow::MainWindow(QWidget *parent)
    : QMainWindow(parent)
      , view(new QWebEngineView(this)) {
    setCentralWidget(view);
    // todo: we may not need our own page in the end: will keep for now to handle other cases:
    // Note: we can probably change the download link in the UI to point to an external link to see the raw text api endpoint
    const auto page = new MyWebEnginePage(this);
    // This allows external links in our app to be opened by the external browser:
    connect(page, &QWebEnginePage::newWindowRequested, [](QWebEngineNewWindowRequest const &request) {
        QDesktopServices::openUrl(request.requestedUrl());
    });
    view->setPage(page);
    view->load(QUrl("http://localhost:11987"));
    // page->load(QUrl("http://localhost:11987"));

    // todo: zoom adjustement (probably with webchannel)
    // view->setZoomFactor()

    closing = false;

    const auto ccHeader = new QAction(QIcon(":/icons/icon.png"), tr("CoolerControl"), this);
    ccHeader->setDisabled(true);
    showAction = new QAction(tr("&Hide"), this);
    connect(showAction, &QAction::triggered, [this]() {
        if (isVisible()) {
            hide();
            showAction->setText(tr("&Show"));
        } else {
            show();
            activateWindow();
            showAction->setText(tr("&Hide"));
        }
    });
    quitAction = new QAction(tr("&Quit"), this);
    connect(quitAction, &QAction::triggered, [this]() {
        closing = true;
        // This closes the window, but doesn't quit the application
        close();
    });
    trayIconMenu = new QMenu(this);
    trayIconMenu->addAction(ccHeader);
    trayIconMenu->addSeparator();
    trayIconMenu->addAction(showAction);
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
}

void MainWindow::closeEvent(QCloseEvent *event) {
    QApplication::quit();
    // todo: logic for CloseToTray setting:
    // if (closing) {
    //     event->accept();
    //     deleteLater();
    //     QApplication::quit();
    // } else {
    //     this->hide();
    //     event->ignore();
    // }
}

MainWindow::~MainWindow() {
    delete view;
    delete sysTrayIcon;
    delete trayIconMenu;
    delete quitAction;
    delete showAction;
};
