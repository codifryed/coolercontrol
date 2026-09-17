// SPDX-FileCopyrightText: 2025 Guy Boldon and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#ifndef CONSTANTS_H
#define CONSTANTS_H

#include <string>

const std::string COOLER_CONTROL_VERSION = "5.0.1";
const std::string APP_ID = "org.coolercontrol.CoolerControl";
const std::string APP_ID_ALERT = "org.coolercontrol.CoolerControl-alert";
const std::string APP_ID_SYMBOLIC = "org.coolercontrol.CoolerControl-symbolic";
// The variant qualifier goes before "-symbolic": panels key their recolouring
// off that suffix, and lose the icon entirely when it comes and goes.
const std::string APP_ID_ALERT_SYMBOLIC = "org.coolercontrol.CoolerControl-alert-symbolic";
// Pre-5.0.0 name for the same icon. Tried before the colour fallback so a
// third-party package that still installs it keeps the symbolic badge.
// Goes when the packaging/metadata symlink under that name goes.
const std::string APP_ID_ALERT_SYMBOLIC_LEGACY = "org.coolercontrol.CoolerControl-symbolic-alert";
const std::string DBUS_SERVICE_NAME = "org.coolercontrol.CoolerControl.SingleInstance";
const std::string DBUS_PATH = "/";
const std::string SETTING_DAEMON_ADDRESS = "daemonAddress";
const std::string SETTING_DAEMON_PORT = "daemonPort";
const std::string SETTING_DAEMON_SSL_ENABLED = "daemonSSLEnabled";
const std::string SETTING_START_IN_TRAY = "startInTray";
const std::string SETTING_STARTUP_DELAY = "startupDelay";
const std::string SETTING_CLOSE_TO_TRAY = "closeToTray";
const std::string SETTING_ZOOM_FACTOR = "zoomFactor";
const std::string SETTING_WINDOW_GEOMETRY = "windowGeometry";
// Pre-4.4 single-daemon keys. Read once on upgrade, then removed: see loadAccessToken.
const std::string SETTING_ACCESS_TOKEN = "accessToken";
const std::string SETTING_ACCESS_TOKEN_ID = "accessTokenId";
// A token belongs to the daemon that minted it, so both are kept per host:port. Two
// groups rather than one nested pair, so neither name is a prefix of the other and a
// QSettings::remove of either cannot take the other's subtree with it.
const std::string SETTING_GROUP_ACCESS_TOKENS = "daemonAccessTokens";
const std::string SETTING_GROUP_ACCESS_TOKEN_IDS = "daemonAccessTokenIds";
// Strings the UI pushes over IPC for Qt-rendered dialogs, cached so a discarded
// renderer does not have to be asked.
const std::string SETTING_GROUP_TRANSLATIONS = "translations";
// Whether the user has been shown the one-time close-to-tray prompt.
const std::string SETTING_CLOSE_PROMPT_SHOWN = "closePromptShown";
// Identity and label of the sensors pinned in the UI, pushed over IPC.
// Pinned sensors belong to one daemon, so they are stored per host:port under this
// group. The name differs from the legacy key below on purpose: QSettings::remove takes
// a whole subtree, so sharing the name would wipe every other daemon on each write.
const std::string SETTING_GROUP_PINNED_SENSORS = "trayPinnedSensors";
const std::string SETTING_PINNED_SENSORS_LEGACY = "pinnedSensors";
// Require a normally valid TLS certificate. Off by default, because the daemon ships a
// self-signed one and the common case is a loopback connection.
const std::string SETTING_TLS_STRICT = "tlsStrict";
// SHA-256 pins accepted by the user for remote daemons, keyed by host:port.
const std::string SETTING_GROUP_TLS_PINS = "tlsPins";
// The saved daemon list, as a QSettings array. Which entry is live is not stored here:
// that stays in the flat address keys above, so older builds keep working and there is
// no index to repair when the list changes. See connections.h.
const std::string SETTING_GROUP_CONNECTIONS = "daemonConnections";
const std::string DEFAULT_DAEMON_ADDRESS = "localhost";
constexpr int DEFAULT_DAEMON_PORT = 11987;
constexpr bool DEFAULT_DAEMON_SSL_ENABLED = true;
constexpr int DEFAULT_CONNECTION_TIMEOUT_MS = 8000;
// 2s is good at startup, so as not to hit the daemon with lots of requests at once (+UI)
constexpr int DEFAULT_CONNECTION_RETRY_INTERVAL_MS = 2000;
// Backoff for an event stream the daemon answers but refuses. The daemon is reachable,
// so retrying at the connection interval forever only produces noise.
constexpr int SSE_RETRY_BASE_MS = 2000;
constexpr int SSE_RETRY_MAX_MS = 60000;
// How long the window stays hidden before the renderer is torn down. Above the UI's own
// 7s stale-history threshold, so a restore that reloads and a discard never disagree.
constexpr int DISCARD_DELAY_MS = 10000;
// How long the daemon must stay unreachable before the user is told, measured across
// failed health probes. A drop that resolves inside this window is a blip, not an
// outage, and a mobile connection produces those routinely. Twice the daemon's 5s
// maximum poll rate, and the same floor the UI's own overlay uses.
constexpr int DAEMON_DISCONNECT_NOTIFY_DELAY_MS = 10000;
// Above this many pinned sensors the rows move into their own submenu, so they stop
// competing with Modes/Show/Quit for room in the main tray menu.
constexpr int TRAY_SENSORS_INLINE_MAX = 5;
// Readings refresh on this cadence while the tray menu is open, and not at all while it
// is closed. One bulk status request per tick, regardless of how many sensors are pinned.
constexpr int TRAY_SENSOR_POLL_MS = 1500;
// Stops the poll if a tray host never tells us the menu closed. Without this a missed
// aboutToHide would leave the timer running for the life of the process.
constexpr int TRAY_SENSOR_POLL_MAX_TICKS = 80;
const std::string WEBENGINE_PROFILE_NAME = "coolercontrol";
const std::string ENDPOINT_HEALTH = "/health";
// Unauthenticated, and answers {"shake": true}. Used to confirm the configured address
// really is a CoolerControl daemon before rendering whatever is there.
const std::string ENDPOINT_HANDSHAKE = "/handshake";
const std::string ENDPOINT_TOKENS = "/tokens";
const std::string ENDPOINT_STATUS = "/status";
// Shown in the UI's token list. Kept stable so a re-provision replaces the old entry
// rather than accumulating one per launch.
const std::string ACCESS_TOKEN_LABEL = "CoolerControl Desktop";
const std::string ENDPOINT_MODES = "/modes";
const std::string ENDPOINT_MODES_ACTIVE = "/modes-active";
const std::string ENDPOINT_ALERTS = "/alerts";
const std::string ENDPOINT_SSE = "/sse";
// Narrows the multiplexed stream to the event kinds this client handles, so the
// once-per-poll status ticks stay off a connection with no use for them.
const std::string SSE_EVENTS_QUERY = "events=logs,modes,notifications";

#endif  // CONSTANTS_H
