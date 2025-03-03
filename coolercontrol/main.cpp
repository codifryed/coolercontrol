// CoolerControl - monitor and control your cooling and other devices
// Copyright (c) 2021-2025  Guy Boldon and contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

#include <QApplication>
#include <QCommandLineParser>
#include <QDBusConnection>
#include <QLoggingCategory>

#include "constants.h"
#include "main_window.h"

int main(int argc, char* argv[]) {
  qputenv("QT_MESSAGE_PATTERN",
          "%{time} coolercontrol "
          "%{if-debug}\033[0;34mDEBUG%{endif}%{if-info}\033[0;32mINFO%{endif}%{if-warning}\033[0;"
          "33mWARN%{endif}%{if-critical}\033[0;31mCRIT%{endif}%{if-fatal}\033[0;31mFATAL%{endif}"
          "\033[0m [%{category}]: %{message}");
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
  // single-instance
  auto connection = QDBusConnection::sessionBus();
  if (connection.isConnected()) {
    if (!connection.registerService(DBUS_NAME.data())) {
      qCritical()
          << "There appears to already be an instance of CoolerControl running.\nPlease check your"
             "system tray for the application icon or the task manager to find the running "
             "instance.";
      return 1;
    }
  } else {
    qWarning("Cannot connect to the D-Bus session bus.");
  }
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
  const auto exitCode = QApplication::exec();
  if (connection.isConnected()) {
    connection.unregisterService(DBUS_NAME.data());
  }
  return exitCode;
}
