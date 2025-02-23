#ifndef CONSTANTS_H
#define CONSTANTS_H

#include <string>

const std::string SETTING_DAEMON_ADDRESS = "daemonAddress";
const std::string SETTING_DAEMON_PORT = "daemonPort";
const std::string SETTING_DAEMON_SSL_ENABLED = "daemonSSLEnabled";
const std::string SETTING_START_IN_TRAY = "startInTray";
const std::string SETTING_STARTUP_DELAY = "startupDelay";
const std::string SETTING_CLOSE_TO_TRAY = "closeToTray";
const std::string SETTING_WINDOW_GEOMETRY = "windowGeometry";
const std::string DEFAULT_DAEMON_ADDRESS = "localhost";
constexpr int DEFAULT_DAEMON_PORT = 11987;
constexpr bool DEFAULT_DAEMON_SSL_ENABLED = false;

#endif //CONSTANTS_H
