#ifndef MAINWINDOW_H
#define MAINWINDOW_H

#include "constants.h"
#include "addresswizard.h"
#include "ipc.h"
#include "webpage.h"

#include <QMainWindow>
#include <QWebEngineView>
#include <QWebEnginePage>
#include <QWebEngineProfile>
#include <QSystemTrayIcon>
#include <QMenu>
#include <QCloseEvent>
#include <QWebChannel>
#include <QNetworkAccessManager>

class MainWindow final : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(QWidget *parent = nullptr);

    void handleStartInTray();

    static void delay(int millisecondsWait);

    void setZoomFactor(double zoomFactor) const;

protected:
    void closeEvent(QCloseEvent *event) override;

    void hideEvent(QHideEvent *event) override;

    void showEvent(QShowEvent *event) override;

private:
    QWebEngineView *view;
    QWebEngineProfile *profile;
    QWebEnginePage *page;
    QWebChannel *channel;
    IPC *ipc;
    bool forceQuit;
    QSystemTrayIcon *sysTrayIcon;
    QMenu *trayIconMenu;
    QMenu *modesTrayMenu;
    QAction *quitAction;
    QAction *addressAction;
    QAction *showAction;
    QWizard *wizard;
    QNetworkAccessManager * manager;

    bool deamonHasErrors = false;

    static QUrl getDaemonUrl();

    static QUrl getEndpointUrl(const QString& endpoint);

    void displayAddressWizard();

    void setTrayActionToShow() const;

    void setTrayActionToHide() const;

    void notifyDaemonErrors() const;
};
#endif // MAINWINDOW_H
