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

#include "notifier.h"

#include <QDBusArgument>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QDebug>
#include <QImage>

static const QString APP_NAME = QStringLiteral("CoolerControl");
static const QString APP_ID = QStringLiteral("org.coolercontrol.CoolerControl");
static const QString DBUS_SERVICE = QStringLiteral("org.freedesktop.Notifications");
static const QString DBUS_PATH = QStringLiteral("/org/freedesktop/Notifications");
static const QString DBUS_INTERFACE = QStringLiteral("org.freedesktop.Notifications");

static const QString ICON_RESOURCE_TRIGGERED = QStringLiteral(":/icons/alert-triggered.png");
static const QString ICON_RESOURCE_RESOLVED = QStringLiteral(":/icons/alert-resolved.png");
static const QString ICON_RESOURCE_ERROR = QStringLiteral(":/icons/alert-error.png");
static const QString ICON_RESOURCE_INFO = QStringLiteral(":/icons/information.png");
static const QString ICON_RESOURCE_SHUTDOWN = QStringLiteral(":/icons/shutdown.png");

static QString iconResourcePath(int icon) {
  switch (icon) {
    case 1:
      return ICON_RESOURCE_TRIGGERED;
    case 2:
      return ICON_RESOURCE_RESOLVED;
    case 3:
      return ICON_RESOURCE_ERROR;
    case 5:
      return ICON_RESOURCE_SHUTDOWN;
    default:
      return ICON_RESOURCE_INFO;
  }
}

// Builds the "image-data" hint value: a D-Bus struct (signature iiibiiay) that carries the icon
// pixels inline, per the Freedesktop Notifications spec. Fields, in order:
//   i  width            pixel width
//   i  height           pixel height
//   i  rowstride        bytes per row (width * channels)
//   b  has_alpha        true (we always emit RGBA)
//   i  bits_per_sample  8
//   i  channels         4 (RGBA)
//   ay pixel_data       row-major RGBA8888 bytes
// Sending pixels inline means the notification server needs no icon theme or file path lookup.
static QVariant buildImageData(int icon) {
  const QImage img(iconResourcePath(icon));
  if (img.isNull()) {
    qWarning() << "Failed to load notification icon resource for icon:" << icon;
    return {};
  }
  const QImage rgba = img.convertToFormat(QImage::Format_RGBA8888);
  const int width = rgba.width();
  const int height = rgba.height();
  const int channels = 4;
  const int bitsPerSample = 8;
  const bool hasAlpha = true;
  const int rowstride = width * channels;
  const QByteArray pixelData(reinterpret_cast<const char*>(rgba.constBits()), rgba.sizeInBytes());

  QDBusArgument arg;
  arg.beginStructure();
  arg << width << height << rowstride << hasAlpha << bitsPerSample << channels << pixelData;
  arg.endStructure();
  return QVariant::fromValue(arg);
}

static bool isGnome() {
  const QString desktop = qEnvironmentVariable("XDG_CURRENT_DESKTOP").toLower();
  return desktop.contains(QLatin1String("gnome"));
}

// Fires a desktop notification by calling org.freedesktop.Notifications.Notify on the session bus
// (Freedesktop Notifications spec: https://specifications.freedesktop.org/notification/latest).
// Method signature is (susssasa{sv}i) -> u; arguments, in order:
//   s      app_name        "CoolerControl"
//   u      replaces_id     0, always a new notification (we never replace one)
//   s      app_icon        APP_ID; GNOME shows this as the application icon
//   s      summary         title text
//   s      body            body text
//   as     actions         empty (we offer no actions)
//   a{sv}  hints           see below
//   i      expire_timeout  -1, let the server decide when it disappears
// Hints we set:
//   "desktop-entry" (s)         APP_ID, set only when NOT on GNOME: GNOME misbehaves when it is
//                               set, while KDE uses it to persist the notification.
//   "resident"      (b)         true, keep the notification resident.
//   "image-data"    (iiibiiay)  inline icon pixels for known icons 1..5 (see buildImageData).
//   "image-path"    (s)         APP_ID, fallback icon when no specific icon applies.
//   "sound-name"    (s)         "alarm-clock-elapsed", only when audio is requested.
//   "urgency"       (y)         single byte: 0 low, 1 normal, 2 critical.
// The returned notification id is ignored; we send non-blocking (QDBus::NoBlock).
void Notifier::send(const QString& summary, const QString& body, int icon, bool audio,
                    int urgency) {
  QDBusMessage msg = QDBusMessage::createMethodCall(DBUS_SERVICE, DBUS_PATH, DBUS_INTERFACE,
                                                    QStringLiteral("Notify"));
  const uint replaceId = 0;
  const QStringList actions;
  QVariantMap hints;

  if (!isGnome()) {
    // Gnome has a bug if the desktop-entry is set.
    // For KDE it enables proper persistence of notifications.
    hints[QStringLiteral("desktop-entry")] = APP_ID;
  }
  hints[QStringLiteral("resident")] = true;

  if (icon >= 1 && icon <= 5) {
    const QVariant imageData = buildImageData(icon);
    if (imageData.isValid()) {
      hints[QStringLiteral("image-data")] = imageData;
    }
  } else {
    hints[QStringLiteral("image-path")] = APP_ID;
  }

  if (audio) {
    hints[QStringLiteral("sound-name")] = QStringLiteral("alarm-clock-elapsed");
  }
  // Freedesktop urgency hint is a single byte: 0=low, 1=normal, 2=critical.
  hints[QStringLiteral("urgency")] = static_cast<uchar>(qBound(0, urgency, 2));

  const int expireTimeout = -1;
  msg << APP_NAME << replaceId << APP_ID << summary << body << actions << hints << expireTimeout;

  const QDBusMessage reply = QDBusConnection::sessionBus().call(msg, QDBus::NoBlock);
  if (reply.type() == QDBusMessage::ErrorMessage) {
    qWarning() << "D-Bus notification failed:" << reply.errorMessage();
  }
}
