#include "mainwindow.h"

#include <QApplication>
#include <QLoggingCategory>
#include <QWebEngineProfile>
#include <QWebEngineSettings>
#include <QWebEngineUrlScheme>

int main(int argc, char *argv[]) {
    // Enables js debug output: (very cool)
    // QLoggingCategory::setFilterRules("*.debug=true");
    QLoggingCategory::setFilterRules("qt.webenginecontext.debug=true");

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
    // todo: needed?:
    QApplication::setApplicationVersion("2.0.0");
    QApplication::setQuitOnLastWindowClosed(false);
    QWebEngineProfile::defaultProfile()->settings()->setAttribute(QWebEngineSettings::Accelerated2dCanvasEnabled, true);
    QWebEngineProfile::defaultProfile()->settings()->setAttribute(QWebEngineSettings::ScreenCaptureEnabled, false);
    QWebEngineProfile::defaultProfile()->settings()->setAttribute(QWebEngineSettings::PluginsEnabled, false);
    QWebEngineProfile::defaultProfile()->settings()->setAttribute(QWebEngineSettings::PdfViewerEnabled, false);
    QCoreApplication::setAttribute(Qt::AA_EnableHighDpiScaling);

    // QWebEngineProfile::defaultProfile()->settings()->setUnknownUrlSchemePolicy(QWebEngineSettings::AllowAllUnknownUrlSchemes);

    MainWindow w;
    w.setWindowTitle("CoolerControl");
    w.setMinimumSize(400, 400);
    w.resize(1600, 900);
    w.show();
    return QApplication::exec();
}
