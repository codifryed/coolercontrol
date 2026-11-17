// SPDX-FileCopyrightText: 2025 Guy Boldon and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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
    m_mainWindow->showWindow();
  }
};
#endif  // DBUS_LISTENER_H
