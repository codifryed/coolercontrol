#include "ipc.h"

#include <QApplication>
#include <QFileDialog>

#include "constants.h"
#include "main_window.h"

IPC::IPC(QObject* parent) : QObject(parent), m_settings(new QSettings()) {
  //        connect(dialog, &Dialog::sendText, this, &Core::sendText);
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

double IPC::getZoomFactor() const {
  return m_settings->value(SETTING_ZOOM_FACTOR.data(), 1.0).toDouble();
}

QByteArray IPC::getWindowGeometry() const {
  return m_settings->value(SETTING_WINDOW_GEOMETRY.data(), 0).toByteArray();
}

QString IPC::filePathDialog(const QString& title) const {
  return QFileDialog::getOpenFileName(qobject_cast<MainWindow*>(parent()), title, QDir::homePath());
}

QString IPC::directoryPathDialog(const QString& title) const {
  return QFileDialog::getExistingDirectory(qobject_cast<MainWindow*>(parent()), title,
                                           QDir::homePath(), QFileDialog::ShowDirsOnly);
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
  qobject_cast<MainWindow*>(parent())->setZoomFactor(zoomFactor);
}

void IPC::setModes(const QString& modesJson) const {
  qobject_cast<MainWindow*>(parent())->setTrayMenuModes(modesJson);
}

void IPC::saveWindowGeometry(const QByteArray& geometry) const {
  m_settings->setValue(SETTING_WINDOW_GEOMETRY.data(), geometry);
}

void IPC::acknowledgeDaemonIssues() const {
  // todo: refactor all these direct calls to signal/slot connections in the constructor
  qobject_cast<MainWindow*>(parent())->acknowledgeDaemonErrors();
}

void IPC::syncSettings() const { m_settings->sync(); }

void IPC::forceQuit() const {
  // this is only called when open form the UI currently
  // closing saves the window geometry when quit from UI:
  qobject_cast<MainWindow*>(parent())->close();
  syncSettings();
  QApplication::quit();
}
