#include "ipc.h"

#include <iostream>
#include <QApplication>

#include "constants.h"


IPC::IPC(QObject *parent)
    : QObject(parent)
      , settings(new QSettings()) {
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

void IPC::setStartInTray(const bool startInTray) const {
    settings->setValue(SETTING_START_IN_TRAY.data(), startInTray);
}

void IPC::setStartupDelay(const int startupDelay) const {
    settings->setValue(SETTING_STARTUP_DELAY.data(), startupDelay);
}

void IPC::setCloseToTray(const bool closeToTray) const {
    settings->setValue(SETTING_CLOSE_TO_TRAY.data(), closeToTray);
}

void IPC::forceQuit() {
    QApplication::quit();
}
