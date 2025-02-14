#ifndef MAINWINDOW_H
#define MAINWINDOW_H

#include <QMainWindow>
#include <QWebEngineView>
#include <QSystemTrayIcon>
#include <QMenu>
#include <QCloseEvent>

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(QWidget *parent = nullptr);

    ~MainWindow() final;

protected:
    void closeEvent(QCloseEvent *event) override;

private:
    QWebEngineView *view;
    bool closing;
    QSystemTrayIcon *sysTrayIcon;
    QMenu *trayIconMenu;
    QAction *quitAction;
    QAction *showAction;
};
#endif // MAINWINDOW_H
