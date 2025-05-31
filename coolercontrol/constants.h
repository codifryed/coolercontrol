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

#ifndef CONSTANTS_H
#define CONSTANTS_H

#include <string>

const std::string COOLER_CONTROL_VERSION = "2.2.0";
const std::string APP_ID = "org.coolercontrol.CoolerControl";
const std::string APP_ID_SYMBOLIC = "org.coolercontrol.CoolerControl-symbolic";
const std::string DBUS_NAME = "org.coolercontrol.SingleInstance";
const std::string DBUS_PATH = "/org/coolercontrol/SingleInstance";
const std::string DBUS_INTERFACE = "org.coolercontrol.SingleInstance";
const std::string SETTING_DAEMON_ADDRESS = "daemonAddress";
const std::string SETTING_DAEMON_PORT = "daemonPort";
const std::string SETTING_DAEMON_SSL_ENABLED = "daemonSSLEnabled";
const std::string SETTING_START_IN_TRAY = "startInTray";
const std::string SETTING_STARTUP_DELAY = "startupDelay";
const std::string SETTING_CLOSE_TO_TRAY = "closeToTray";
const std::string SETTING_ZOOM_FACTOR = "zoomFactor";
const std::string SETTING_WINDOW_GEOMETRY = "windowGeometry";
const std::string DEFAULT_DAEMON_ADDRESS = "localhost";
constexpr int DEFAULT_DAEMON_PORT = 11987;
constexpr bool DEFAULT_DAEMON_SSL_ENABLED = false;
constexpr int DEFAULT_CONNECTION_TIMEOUT_MS = 8000;
// 2s is good at startup, so as not to hit the daemon with lots of requests at once (+UI)
constexpr int DEFAULT_CONNECTION_RETRY_INTERVAL_MS = 2000;
const std::string USER_ID = "CCAdmin";
const std::string ENDPOINT_HEALTH = "/health";
const std::string ENDPOINT_MODES = "/modes";
const std::string ENDPOINT_MODES_ACTIVE = "/modes-active";
const std::string ENDPOINT_SSE_LOGS = "/sse/logs";
const std::string ENDPOINT_SSE_MODES = "/sse/modes";
const std::string ENDPOINT_SSE_ALERTS = "/sse/alerts";

#endif  // CONSTANTS_H
