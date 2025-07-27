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

#ifndef DBUS_LISTENER_H
#define DBUS_LISTENER_H

#include <QDBusAbstractAdaptor>
#include <QDBusConnectionInterface>

#include "main_window.h"

class DBusListener final : public QDBusAbstractAdaptor {
  Q_OBJECT
  Q_CLASSINFO("D-Bus Interface", "org.coolercontrol.CoolerControl.SingleInstance")
  Q_CLASSINFO("D-Bus Introspection",
              "<interface name=\"org.coolercontrol.CoolerControl.SingleInstance\">"
              "<method name=\"showInstance\"/>"
              "</interface>")

 private:
  MainWindow* m_mainWindow;

 public:
  explicit DBusListener(MainWindow* parent) : QDBusAbstractAdaptor(parent), m_mainWindow(parent) {}

 public slots:
  Q_NOREPLY void showInstance() const {
    qInfo() << "Request from dbus to force show main window";
    m_mainWindow->hide();
    m_mainWindow->show();
  }
};
#endif  // DBUS_LISTENER_H
