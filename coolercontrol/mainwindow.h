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

protected:
    void closeEvent(QCloseEvent *event) override;

private:
    QWebEngineView *view;
    QWebEngineProfile *profile;
    QWebEnginePage *page;
    QWebChannel *channel;
    IPC *ipc;
    bool closing;
    QSystemTrayIcon *sysTrayIcon;
    QMenu *trayIconMenu;
    QAction *quitAction;
    QAction *addressAction;
    QAction *showAction;
    QWizard *wizard;
    static QUrl getDaemonUrl();
    void displayAddressWizard();
};
#endif // MAINWINDOW_H
