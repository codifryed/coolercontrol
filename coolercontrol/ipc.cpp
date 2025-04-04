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

#include "ipc.h"

#include <QFileDialog>

#include "constants.h"
#include "main_window.h"

IPC::IPC(QObject* parent)
    : QObject(parent),
      m_settings(new QSettings(this)),
      m_mainWindow(qobject_cast<MainWindow*>(parent)) {
  connect(m_mainWindow, &MainWindow::setZoomFactorSignal, m_mainWindow, &MainWindow::setZoomFactor,
          Qt::QueuedConnection);
  connect(m_mainWindow, &MainWindow::setTrayMenuModesSignal, m_mainWindow,
          &MainWindow::setTrayMenuModes, Qt::QueuedConnection);
  connect(m_mainWindow, &MainWindow::acknowledgeDaemonErrorsSignal, m_mainWindow,
          &MainWindow::acknowledgeDaemonErrors, Qt::QueuedConnection);
  connect(m_mainWindow, &MainWindow::forceQuitSignal, m_mainWindow, &MainWindow::forceQuit,
          Qt::QueuedConnection);
}

bool IPC::getStartInTray() const {
  return m_settings->value(SETTING_START_IN_TRAY.data(), false).toBool();
}

int IPC::getStartupDelay() const {
  return m_settings->value(SETTING_STARTUP_DELAY.data(), 0).toInt();
}

bool IPC::getCloseToTray() const {
  return m_settings->value(SETTING_CLOSE_TO_TRAY.data(), false).toBool();
}

bool IPC::getIsFullScreen() const { return m_mainWindow->isFullScreen(); }

double IPC::getZoomFactor() const {
  return m_settings->value(SETTING_ZOOM_FACTOR.data(), 1.0).toDouble();
}

QByteArray IPC::getWindowGeometry() const {
  return m_settings->value(SETTING_WINDOW_GEOMETRY.data(), 0).toByteArray();
}

QString IPC::filePathDialog(const QString& title) const {
  return QFileDialog::getOpenFileName(m_mainWindow, title, QDir::homePath());
}

QString IPC::directoryPathDialog(const QString& title) const {
  return QFileDialog::getExistingDirectory(m_mainWindow, title, QDir::homePath(),
                                           QFileDialog::ShowDirsOnly);
}

void IPC::setStartInTray(const bool startInTray) const {
  m_settings->setValue(SETTING_START_IN_TRAY.data(), startInTray);
}

void IPC::setStartupDelay(const int startupDelay) const {
  m_settings->setValue(SETTING_STARTUP_DELAY.data(), startupDelay);
}

void IPC::setCloseToTray(const bool closeToTray) const {
  m_settings->setValue(SETTING_CLOSE_TO_TRAY.data(), closeToTray);
}

void IPC::setZoomFactor(const double zoomFactor) const {
  m_settings->setValue(SETTING_ZOOM_FACTOR.data(), zoomFactor);
  emit m_mainWindow->setZoomFactorSignal(zoomFactor);
}

void IPC::setModes(const QString& modesJson) const {
  emit m_mainWindow->setTrayMenuModesSignal(modesJson);
}

void IPC::saveWindowGeometry(const QByteArray& geometry) const {
  m_settings->setValue(SETTING_WINDOW_GEOMETRY.data(), geometry);
}

void IPC::acknowledgeDaemonIssues() const { emit m_mainWindow->acknowledgeDaemonErrorsSignal(); }

void IPC::syncSettings() const { m_settings->sync(); }

void IPC::forceQuit() const {
  // this is only called when open from the UI currently
  emit m_mainWindow->forceQuitSignal();
}
