// SPDX-FileCopyrightText: 2026 Guy Boldon and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "origin_filter.h"

#include <QDebug>
#include <QMutexLocker>

namespace {

/*
  Schemes the engine needs for its own plumbing, none of which reach the network:

    qrc      Qt resources. Older daemon UIs load qwebchannel.js this way.
    data     inline images and fonts, allowed by the daemon's CSP.
    blob     generated in-page, e.g. chart exports.
    about    about:blank, used between loads.
    chrome-error / chrome  Chromium's own error and internal pages. Blocking these turns
             a failed load into a blank window with no explanation.
*/
bool isLocalScheme(const QString& scheme) {
  return scheme == QLatin1String("qrc") || scheme == QLatin1String("data") ||
         scheme == QLatin1String("blob") || scheme == QLatin1String("about") ||
         scheme == QLatin1String("chrome") || scheme == QLatin1String("chrome-error");
}

}  // namespace

OriginFilter::OriginFilter(QObject* parent) : QWebEngineUrlRequestInterceptor(parent) {}

void OriginFilter::setDaemonUrl(const QUrl& url) {
  const QMutexLocker locker(&m_mutex);
  m_scheme = url.scheme();
  m_host = url.host();
  m_port = url.port();
}

bool OriginFilter::isAllowed(const QUrl& url) const {
  if (isLocalScheme(url.scheme())) {
    return true;
  }
  const QMutexLocker locker(&m_mutex);
  if (m_host.isEmpty()) {
    return true;  // no daemon configured yet; nothing to confine to
  }
  // Host and port must match. The scheme is deliberately not compared: the app itself
  // switches between http and https for the same daemon.
  return url.host().compare(m_host, Qt::CaseInsensitive) == 0 && url.port(m_port) == m_port;
}

void OriginFilter::interceptRequest(QWebEngineUrlRequestInfo& info) {
  const auto url = info.requestUrl();
  if (isAllowed(url)) {
    return;
  }
  qWarning() << "Blocked a request outside the daemon origin:" << url.toDisplayString();
  info.block(true);
}
