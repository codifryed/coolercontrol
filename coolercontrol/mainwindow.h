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

class MainWindow final : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(QWidget *parent = nullptr);

    void handleStartInTray();

    static void delay(int millisecondsWait);

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
    QAction *quitAction;
    QAction *addressAction;
    QAction *showAction;
    QWizard *wizard;

    static QUrl getDaemonUrl();

    void displayAddressWizard();

    void setTrayActionToShow() const;

    void setTrayActionToHide() const;
};
#endif // MAINWINDOW_H
