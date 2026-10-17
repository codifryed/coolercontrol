// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use log::debug;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

/// Maximum buffered notifications before oldest are dropped.
const NOTIFICATION_CHANNEL_CAPACITY: usize = 16;

/// Icon types for desktop notifications.
/// Each variant maps to a specific icon image displayed in the notification.
/// Serialized as `snake_case` strings for SSE consumers (Qt app, web UI).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotificationIcon {
    Triggered = 1,
    Resolved = 2,
    Error = 3,
    Info = 4,
    Shutdown = 5,
}

/// A desktop notification event broadcast to connected clients via SSE.
/// The daemon decides when to notify; clients decide how to display.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DesktopNotification {
    pub title: String,
    pub body: String,
    pub icon: NotificationIcon,
    pub audio: bool,
    pub urgency: u8, // 0 = low, 1 = normal, 2 = critical
}

/// Broadcast-only handle for publishing notification events to SSE
/// subscribers. No actor task needed; this is a thin wrapper around a
/// tokio broadcast channel.
#[derive(Clone)]
pub struct NotificationHandle {
    broadcaster: broadcast::Sender<DesktopNotification>,
    cancel_token: CancellationToken,
}

impl NotificationHandle {
    pub fn new(cancel_token: CancellationToken) -> Self {
        let (broadcaster, _) = broadcast::channel(NOTIFICATION_CHANNEL_CAPACITY);
        Self {
            broadcaster,
            cancel_token,
        }
    }

    /// Broadcasts a notification to all connected SSE subscribers.
    /// No-op if there are no active listeners.
    pub fn broadcast(&self, notification: DesktopNotification) {
        if self.broadcaster.receiver_count() > 0 {
            let _ = self.broadcaster.send(notification);
        }
    }

    pub fn broadcaster(&self) -> &broadcast::Sender<DesktopNotification> {
        &self.broadcaster
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }
}

/// Broadcasts a desktop notification event to connected clients via SSE.
///
/// The daemon no longer sends D-Bus notifications directly (system
/// services cannot reach user session buses on dbus-broker systems).
/// Instead, clients (Qt app, web UI) receive the event and handle
/// display natively.
pub fn notify_all_sessions(
    summary: &str,
    body: &str,
    icon: NotificationIcon,
    audio: bool,
    urgency: Option<u8>,
    notification_handle: Option<&NotificationHandle>,
) {
    let notification = DesktopNotification {
        title: summary.to_string(),
        body: body.to_string(),
        icon,
        audio,
        urgency: urgency.unwrap_or(1),
    };
    debug!(
        "Broadcasting notification: {} - {}",
        notification.title, notification.body
    );
    if let Some(handle) = notification_handle {
        handle.broadcast(notification);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_icon_serializes_to_snake_case() {
        // Icon variants must serialize as snake_case for SSE consumers.
        let json = serde_json::to_string(&NotificationIcon::Triggered).unwrap();
        assert_eq!(json, "\"triggered\"");
        let json = serde_json::to_string(&NotificationIcon::Shutdown).unwrap();
        assert_eq!(json, "\"shutdown\"");
    }

    #[test]
    fn notification_icon_deserializes_from_snake_case() {
        // Consumers must be able to deserialize icon names back.
        let icon: NotificationIcon = serde_json::from_str("\"error\"").unwrap();
        assert!(matches!(icon, NotificationIcon::Error));
    }

    #[test]
    fn desktop_notification_serde_roundtrip() {
        // A notification must survive JSON serialization and deserialization.
        let notification = DesktopNotification {
            title: "Alert Triggered".to_string(),
            body: "CPU temperature exceeded threshold".to_string(),
            icon: NotificationIcon::Triggered,
            audio: true,
            urgency: 2,
        };
        let json = serde_json::to_string(&notification).unwrap();
        let deserialized: DesktopNotification = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.title, notification.title);
        assert_eq!(deserialized.body, notification.body);
        assert_eq!(deserialized.audio, notification.audio);
        assert_eq!(deserialized.urgency, notification.urgency);
    }

    #[test]
    fn notification_handle_broadcast_no_receivers() {
        // Broadcasting with no receivers must not panic.
        let handle = NotificationHandle::new(CancellationToken::new());
        handle.broadcast(DesktopNotification {
            title: "test".to_string(),
            body: "test".to_string(),
            icon: NotificationIcon::Info,
            audio: false,
            urgency: 1,
        });
        // No panic = success.
    }

    #[test]
    fn notification_handle_broadcast_with_receiver() {
        // A subscribed receiver must receive the broadcast notification.
        let handle = NotificationHandle::new(CancellationToken::new());
        let mut rx = handle.broadcaster().subscribe();
        handle.broadcast(DesktopNotification {
            title: "Alert".to_string(),
            body: "Temperature high".to_string(),
            icon: NotificationIcon::Triggered,
            audio: true,
            urgency: 2,
        });
        let received = rx.try_recv().unwrap();
        assert_eq!(received.title, "Alert");
        assert_eq!(received.urgency, 2);
        assert!(received.audio);
    }

    #[test]
    fn notify_all_sessions_without_handle_does_not_panic() {
        // Calling with None handle must be a safe no-op.
        notify_all_sessions("Test", "Body", NotificationIcon::Info, false, None, None);
    }
}
