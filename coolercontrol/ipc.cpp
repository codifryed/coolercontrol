#include "ipc.h"

#include <QApplication>

#include "constants.h"
#include "mainwindow.h"

IPC::IPC(QObject* parent) : QObject(parent), settings(new QSettings()) {
  //        connect(dialog, &Dialog::sendText, this, &Core::sendText);
}

bool IPC::getStartInTray() const {
  return settings->value(SETTING_START_IN_TRAY.data(), false).toBool();
}

int IPC::getStartupDelay() const {
  return settings->value(SETTING_STARTUP_DELAY.data(), 0).toInt();
}

bool IPC::getCloseToTray() const {
  return settings->value(SETTING_CLOSE_TO_TRAY.data(), false).toBool();
}

double IPC::getZoomFactor() const {
  return settings->value(SETTING_ZOOM_FACTOR.data(), 1.0).toDouble();
}

QByteArray IPC::getWindowGeometry() const {
  return settings->value(SETTING_WINDOW_GEOMETRY.data(), 0).toByteArray();
}

void IPC::setStartInTray(const bool startInTray) const {
  settings->setValue(SETTING_START_IN_TRAY.data(), startInTray);
}

void IPC::setStartupDelay(const int startupDelay) const {
  settings->setValue(SETTING_STARTUP_DELAY.data(), startupDelay);
}

void IPC::setCloseToTray(const bool closeToTray) const {
  settings->setValue(SETTING_CLOSE_TO_TRAY.data(), closeToTray);
}

void IPC::setZoomFactor(const double zoomFactor) const {
  settings->setValue(SETTING_ZOOM_FACTOR.data(), zoomFactor);
  qobject_cast<MainWindow*>(parent())->setZoomFactor(zoomFactor);
}

void IPC::saveWindowGeometry(const QByteArray& geometry) const {
  settings->setValue(SETTING_WINDOW_GEOMETRY.data(), geometry);
}

void IPC::syncSettings() const { settings->sync(); }

void IPC::forceQuit() const {
  // this is only called when open form the UI currently
  // closing saves the window geometry when quit from UI:
  qobject_cast<MainWindow*>(parent())->close();
  syncSettings();
  QApplication::quit();
}
