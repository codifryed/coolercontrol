// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

/// Maximum buffered system events before the oldest are dropped. System state changes are rare
/// (a user toggling a power profile), so a small buffer covers any realistic burst.
const SYSTEM_EVENT_CHANNEL_CAPACITY: usize = 8;

/// What kind of system state the event describes.
///
/// This exists so external consumers can subscribe to one stream and dispatch on `kind` rather
/// than needing a new event name, and a new SSE substream, per integration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SystemEventKind {
    /// The system power profile, as reported by `power-profiles-daemon` or `tuned-ppd`.
    PowerProfile,
}

/// A change in observed system state, broadcast to connected clients via SSE.
///
/// The daemon reports what changed; clients decide what to do about it. `previous` is `None` for
/// the first observation after the daemon connects to the source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SystemEvent {
    pub kind: SystemEventKind,
    pub value: String,
    pub previous: Option<String>,
}

/// Broadcast-only handle for publishing system events to SSE subscribers. No actor task needed;
/// this is a thin wrapper around a tokio broadcast channel, like `NotificationHandle`.
#[derive(Clone)]
pub struct SystemEventHandle {
    broadcaster: broadcast::Sender<SystemEvent>,
    cancel_token: CancellationToken,
}

impl SystemEventHandle {
    pub fn new(cancel_token: CancellationToken) -> Self {
        let (broadcaster, _) = broadcast::channel(SYSTEM_EVENT_CHANNEL_CAPACITY);
        Self {
            broadcaster,
            cancel_token,
        }
    }

    /// Broadcasts a system event to all connected SSE subscribers.
    /// No-op if there are no active listeners.
    pub fn broadcast(&self, event: SystemEvent) {
        if self.broadcaster.receiver_count() > 0 {
            let _ = self.broadcaster.send(event);
        }
    }

    pub fn broadcaster(&self) -> &broadcast::Sender<SystemEvent> {
        &self.broadcaster
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Goal: `kind` is the dispatch key for external consumers, so its wire form must be a stable
    /// snake_case string.
    /// Methodology: serialize the variant and compare against the literal clients will match on.
    #[test]
    fn system_event_kind_serializes_to_snake_case() {
        let json = serde_json::to_string(&SystemEventKind::PowerProfile).unwrap();
        assert_eq!(json, "\"power_profile\"");
    }

    /// Goal: consumers must be able to parse the kind back out of the wire form.
    /// Methodology: deserialize the literal emitted above.
    #[test]
    fn system_event_kind_deserializes_from_snake_case() {
        let kind: SystemEventKind = serde_json::from_str("\"power_profile\"").unwrap();
        assert_eq!(kind, SystemEventKind::PowerProfile);
    }

    /// Goal: the event must survive a JSON round trip unchanged, including an absent `previous`.
    /// Methodology: round-trip both the first-observation shape and a subsequent change.
    #[test]
    fn system_event_serde_roundtrip() {
        let first = SystemEvent {
            kind: SystemEventKind::PowerProfile,
            value: "balanced".to_string(),
            previous: None,
        };
        let json = serde_json::to_string(&first).unwrap();
        assert_eq!(
            serde_json::from_str::<SystemEvent>(&json).unwrap(),
            first,
            "First observation must round trip with a null previous"
        );

        let changed = SystemEvent {
            kind: SystemEventKind::PowerProfile,
            value: "performance".to_string(),
            previous: Some("balanced".to_string()),
        };
        let json = serde_json::to_string(&changed).unwrap();
        assert_eq!(serde_json::from_str::<SystemEvent>(&json).unwrap(), changed);
    }

    /// Goal: broadcasting with nobody subscribed must not panic, since the listener publishes
    /// unconditionally whether or not a client is attached.
    /// Methodology: broadcast into a handle with no subscribers.
    #[test]
    fn system_event_handle_broadcast_no_receivers() {
        let handle = SystemEventHandle::new(CancellationToken::new());
        handle.broadcast(SystemEvent {
            kind: SystemEventKind::PowerProfile,
            value: "power-saver".to_string(),
            previous: None,
        });
        // No panic = success.
    }

    /// Goal: a subscribed receiver must actually receive what was broadcast.
    /// Methodology: subscribe, broadcast, and read the value back without a runtime.
    #[test]
    fn system_event_handle_broadcast_with_receiver() {
        let handle = SystemEventHandle::new(CancellationToken::new());
        let mut rx = handle.broadcaster().subscribe();
        handle.broadcast(SystemEvent {
            kind: SystemEventKind::PowerProfile,
            value: "performance".to_string(),
            previous: Some("balanced".to_string()),
        });
        let received = rx.try_recv().unwrap();
        assert_eq!(received.kind, SystemEventKind::PowerProfile);
        assert_eq!(received.value, "performance");
        assert_eq!(received.previous.as_deref(), Some("balanced"));
    }
}
