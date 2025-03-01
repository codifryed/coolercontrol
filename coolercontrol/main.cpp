#include <QApplication>
#include <QCommandLineParser>
#include <QLoggingCategory>

#include "constants.h"
#include "main_window.h"

int main(int argc, char* argv[]) {
  qputenv("QT_MESSAGE_PATTERN", "%{time} %{type} %{appname}: %{category} - %{message}");
  // Standard Qt Paths:
  // https://doc.qt.io/qt-6/qstandardpaths.htm
  // settings: ~/.config/{app_id}/{app_id}.conf
  const QApplication a(argc, argv);
  QApplication::setWindowIcon(
      QIcon::fromTheme("application-x-executable", QIcon(":/icons/icon.png")));
  QCoreApplication::setOrganizationName("org.coolercontrol.CoolerControl");
  QApplication::setApplicationName("CoolerControl");
  QApplication::setDesktopFileName("org.coolercontrol.CoolerControl");
  QApplication::setApplicationVersion(COOLER_CONTROL_VERSION.data());
  QApplication::setQuitOnLastWindowClosed(false);
  QCommandLineParser parser;
  parser.setApplicationDescription("CoolerControl GUI Desktop Application");
  parser.addHelpOption();
  parser.addVersionOption();
  const QCommandLineOption debugOption(QStringList() << "d"
                                                     << "debug",
                                       "Enable debug output.");
  parser.addOption(debugOption);
  parser.process(a);
  if (parser.isSet(debugOption)) {
    qputenv("QTWEBENGINE_CHROMIUM_FLAGS", "--enable-logging --log-level=0");
    QLoggingCategory::setFilterRules("*.debug=true");
    QLoggingCategory::setFilterRules("qt.webenginecontext.debug=true");
    qputenv("QTWEBENGINE_REMOTE_DEBUGGING", QByteArray::number(9000));
  } else {
    qputenv("QTWEBENGINE_CHROMIUM_FLAGS", "--enable-logging --log-level=3");
    QLoggingCategory::setFilterRules("js.warning=false");
  }

  MainWindow w;
  w.setWindowTitle("CoolerControl");
  w.setMinimumSize(400, 400);
  w.resize(1600, 900);
  w.handleStartInTray();
  return QApplication::exec();
}
