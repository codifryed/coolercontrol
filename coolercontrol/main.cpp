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

void setChromiumFlags(const bool debugOrFullDebug, const bool disableGpu) {
  QByteArray chromiumFlags;
  if (debugOrFullDebug) {
    chromiumFlags = disableGpu ? "--enable-logging --log-level=0 --disable-gpu"
                               : "--enable-logging --log-level=0";
  } else {
    chromiumFlags = disableGpu ? "--enable-logging --log-level=3 --disable-gpu"
                               : "--enable-logging --log-level=3";
  }
  qputenv("QTWEBENGINE_CHROMIUM_FLAGS", chromiumFlags);
}

void setEnvVars(const bool debugOrFullDebug, const bool disableGpu) {
  if (debugOrFullDebug) {
    qputenv("QTWEBENGINE_REMOTE_DEBUGGING", QByteArray::number(9000));
  }
  if (disableGpu) {
    qputenv("QT_OPENGL", "software");
  }
}

void setLogFilters(const bool debug, const bool fullDebug) {
  if (debug) {
    QLoggingCategory::setFilterRules(
        "default.debug=true\n"
        "qt.webenginecontext.debug=true");
  } else if (fullDebug) {
    QLoggingCategory::setFilterRules(
        "*.debug=true\n"
        "qt.webenginecontext.debug=true");
  } else {
    QLoggingCategory::setFilterRules("js.warning=false");
  }
}

void handleCmdOptions(const bool debug, const bool fullDebug, const bool disableGpu) {
  setChromiumFlags(debug || fullDebug, disableGpu);
  setEnvVars(debug || fullDebug, disableGpu);
  setLogFilters(debug, fullDebug);
}

void parseCLIOptions(const QApplication& a) {
  QCommandLineParser parser;
  parser.setApplicationDescription("CoolerControl GUI Desktop Application");
  parser.addHelpOption();
  parser.addVersionOption();
  const QCommandLineOption debugOption(QStringList() << "d"
                                                     << "debug",
                                       "Enable debug output.");
  parser.addOption(debugOption);
  const QCommandLineOption fullDebugOption(QStringList() << "full-debug",
                                           "Enable full debug output. This outputs a lot of data.");
  parser.addOption(fullDebugOption);
  const QCommandLineOption gpuOption(QStringList() << "disable-gpu",
                                     "Disable GPU hardware acceleration.");
  parser.addOption(gpuOption);
  parser.process(a);
  handleCmdOptions(parser.isSet(debugOption), parser.isSet(fullDebugOption),
                   parser.isSet(gpuOption));
}

/// Entrypoint for the application
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
  QApplication::setWindowIcon(QIcon::fromTheme(APP_ID.data(), QIcon(":/icons/icon.png")));
  QCoreApplication::setOrganizationName(APP_ID.data());
  QApplication::setApplicationName("CoolerControl");
  QApplication::setDesktopFileName(APP_ID.data());
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
  parseCLIOptions(a);

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
