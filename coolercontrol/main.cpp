#include "mainwindow.h"

#include <QApplication>
#include <QLoggingCategory>
#include <QWebEngineUrlScheme>

int main(int argc, char *argv[]) {
    // Enables js debug output: (very cool)
    // QLoggingCategory::setFilterRules("*.debug=true");
    // todo: debugging enabled:
    qputenv("QTWEBENGINE_REMOTE_DEBUGGING", QByteArray::number(9000));
    QLoggingCategory::setFilterRules("qt.webenginecontext.debug=true");

    // Standard Qt Paths:
    // https://doc.qt.io/qt-6/qstandardpaths.htm

    // todo: handle dbus checking for applicaton already running (freedesktop standard)

    // todo: QSettings for:
    //  - window size & position
    //  - start-in-tray

    // todo: handle window position & size:
    //  - save window size & position on close & hide, and restore on open & show
    //  - load window size & position on startup

    // instantiate the application
    QApplication a(argc, argv);
    QApplication::setWindowIcon(
        QIcon::fromTheme("application-x-executable", QIcon(":/icons/icon.png"))
    );
    QCoreApplication::setOrganizationName("org.coolercontrol.CoolerControl");
    QApplication::setApplicationName("org.coolercontrol.CoolerControl");
    QApplication::setDesktopFileName("org.coolercontrol.CoolerControl");
    //settings: ~/.config/{app_id}/{app_id}.conf
    // todo: needed?:
    QApplication::setApplicationVersion("2.0.0");
    QApplication::setQuitOnLastWindowClosed(false);
    // todo: do we need this for Qt 6.2?
    // QCoreApplication::setAttribute(Qt::AA_EnableHighDpiScaling);

    MainWindow w;
    w.setWindowTitle("CoolerControl");
    w.setMinimumSize(400, 400);
    w.resize(1600, 900);
    w.handleStartInTray();
    return QApplication::exec();
}
