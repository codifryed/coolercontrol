// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::alerts::AlertLog;
use crate::api::actor::{AlertHandle, DeviceHealthHandle, ModeHandle, StatusHandle};
use crate::api::modes::ActiveMode;
use crate::api::status::StatusResponse;
use crate::api::{AppState, CCError};
use crate::device_health::{
    DeviceHealthDto, FailsafeDelta, HealthEvent, SourceDelta, UnreachableDelta,
};
use crate::logger::LogBufHandle;
use crate::notifier::{DesktopNotification, NotificationHandle};
use crate::system_event::{SystemEvent, SystemEventHandle};
use aide::generate::GenContext;
use aide::openapi::{Example, MediaType, Operation, ReferenceOr, Response, SchemaObject};
use aide::operation::OperationOutput;
use aide::NoApi;
use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, KeepAliveStream};
use axum::response::Sse;
use futures_util::stream::{select_all, SelectAll};
use futures_util::StreamExt;
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::ops::Not;
use std::pin::Pin;
use std::time::Duration;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use zbus::export::futures_core::Stream;

const DEFAULT_KEEP_ALIVE_INTERVAL_SECONDS: u64 = 30;

/// One merged connection's event source. Boxed so the selected substreams, which have
/// unrelated concrete types, can share a `Vec`.
type EventStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

/// What every SSE handler returns: the selected substreams merged onto one connection.
type SseResponse = NoApi<Sse<KeepAliveStream<SelectAll<EventStream>>>>;

/// Every event `/sse` publishes, pairing the SSE event name with its payload.
///
/// This is the single source of truth for the wire format. Substreams construct these
/// and `Event::from` is the only place an event is named, so the compiler rejects a new
/// variant until it has a name and a payload. The `OpenAPI` schema derives from the same
/// type, which is what keeps the documentation from drifting away from what is sent.
///
/// The serde representation models a frame logically as `{"event": ..., "data": ...}`.
/// The wire form is SSE framing (`event: status\ndata: {...}`), not that envelope:
/// `OpenAPI` 3.1 has no way to describe a stream of tagged frames, so the envelope is the
/// closest honest description. `sse_event_names_match_serde_tags` pins the two together.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(tag = "event", content = "data", rename_all = "kebab-case")]
pub enum SseEvent {
    /// One poll tick of every device's most recent status.
    Status(StatusResponse),
    /// Temp sources that appeared or disappeared this tick.
    Missing(Vec<SourceDelta>),
    /// Temp sources that went stale or recovered this tick.
    StaleSource(Vec<SourceDelta>),
    /// Channels that entered or left failsafe this tick.
    Failsafe(Vec<FailsafeDelta>),
    /// Devices that stopped or resumed answering this tick.
    Unreachable(Vec<UnreachableDelta>),
    /// Full device-health snapshot, sent to a consumer that lagged the transitions.
    Health(DeviceHealthDto),
    /// One or more daemon log lines, pre-coalesced. Raw text, not JSON.
    Log(String),
    /// The mode that just became active.
    Mode(ActiveMode),
    /// An alert whose state changed.
    Alert(AlertLog),
    /// A desktop notification for the client to display.
    Notification(DesktopNotification),
    /// A change in observed system state, discriminated by its `kind`.
    System(SystemEvent),
}

impl From<SseEvent> for Event {
    fn from(event: SseEvent) -> Self {
        match event {
            SseEvent::Status(payload) => json_event("status", &payload),
            SseEvent::Missing(payload) => json_event("missing", &payload),
            SseEvent::StaleSource(payload) => json_event("stale-source", &payload),
            SseEvent::Failsafe(payload) => json_event("failsafe", &payload),
            SseEvent::Unreachable(payload) => json_event("unreachable", &payload),
            SseEvent::Health(payload) => json_event("health", &payload),
            // The only payload that is not JSON.
            SseEvent::Log(line) => Event::default().event("log").data(line),
            SseEvent::Mode(payload) => json_event("mode", &payload),
            SseEvent::Alert(payload) => json_event("alert", &payload),
            SseEvent::Notification(payload) => json_event("notification", &payload),
            SseEvent::System(payload) => json_event("system", &payload),
        }
    }
}

fn json_event<T: Serialize>(name: &str, payload: &T) -> Event {
    Event::default()
        .event(name)
        .json_data(payload)
        .expect("derived DTO serialization cannot fail")
}

/// Documents the `/sse` 200 response. The handler returns a stream, which aide cannot
/// describe on its own, so the response is built here from `SseEvent`'s schema plus
/// literal wire samples.
pub struct SseStream;

impl OperationOutput for SseStream {
    type Inner = SseEvent;

    fn operation_response(ctx: &mut GenContext, _operation: &mut Operation) -> Option<Response> {
        let json_schema = ctx.schema.subschema_for::<SseEvent>();
        Some(Response {
            description: "A stream of Server Sent Events. Each frame is `event: <name>` \
                          followed by `data: <payload>` and a blank line. Dispatch on the \
                          event name. Frames beginning with `:` are keep-alive ticks and \
                          carry no data."
                .to_string(),
            content: IndexMap::from_iter([(
                "text/event-stream".into(),
                MediaType {
                    schema: Some(SchemaObject {
                        json_schema,
                        example: None,
                        external_docs: None,
                    }),
                    examples: wire_examples(),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        })
    }
}

/// Literal frames as they arrive, since the schema above models a frame logically
/// rather than as the bytes on the wire.
fn wire_examples() -> IndexMap<String, ReferenceOr<Example>> {
    let sample = |summary: &str, frame: &str| {
        ReferenceOr::Item(Example {
            summary: Some(summary.to_string()),
            value: Some(serde_json::Value::String(frame.to_string())),
            ..Default::default()
        })
    };
    IndexMap::from_iter([
        (
            "status".to_string(),
            sample(
                "One poll tick, sent continuously",
                "event: status\ndata: {\"devices\":[{\"uid\":\"8f2a...\",\"name\":\"nct6798\",\
                 \"status_history\":[{\"timestamp\":\"2026-08-01T13:22:04.001Z\",\
                 \"temps\":[{\"name\":\"CPUTIN\",\"temp\":42.0}],\
                 \"channels\":[{\"name\":\"fan1\",\"rpm\":1180,\"duty\":45.0}]}]}]}\n\n",
            ),
        ),
        (
            "log".to_string(),
            sample(
                "Raw daemon log text, not JSON",
                "event: log\ndata: 2026-08-01T13:22:04.123 INFO coolercontrold Applied setting\n\n",
            ),
        ),
        (
            "mode".to_string(),
            sample(
                "A mode became active",
                "event: mode\ndata: {\"uid\":\"3c91...\",\"name\":\"Silent\",\
                 \"previous_uid\":\"7ab2...\"}\n\n",
            ),
        ),
        (
            "alert".to_string(),
            sample(
                "An alert changed state",
                "event: alert\ndata: {\"uid\":\"aa10...\",\"name\":\"CPU too hot\",\
                 \"state\":\"Active\",\"message\":\"CPUTIN at 95C\",\"silenced\":false,\
                 \"resolved\":false}\n\n",
            ),
        ),
        (
            "stale-source".to_string(),
            sample(
                "Device-health transitions arrive as a batch of deltas",
                "event: stale-source\ndata: [{\"entity_type\":\"Profile\",\
                 \"entity_uid\":\"11bd...\",\"entity_name\":\"GPU Curve\",\"present\":false}]\n\n",
            ),
        ),
        (
            "notification".to_string(),
            sample(
                "A desktop notification to display",
                "event: notification\ndata: {\"title\":\"CoolerControl\",\
                 \"body\":\"CPU too hot\",\"icon\":\"triggered\",\"audio\":true,\
                 \"urgency\":2}\n\n",
            ),
        ),
        (
            "system".to_string(),
            sample(
                "System state changed outside CoolerControl",
                "event: system\ndata: {\"kind\":\"power_profile\",\
                 \"value\":\"performance\",\"previous\":\"balanced\"}\n\n",
            ),
        ),
        (
            "keep-alive".to_string(),
            sample(
                "Sent only when the subscription has been idle; ignore it",
                ":\n\n",
            ),
        ),
    ])
}

/// A selectable substream of `/sse`. Named after the legacy endpoint it came from, except
/// that `Status` and `Health` are separate broadcasters that both fed `/sse/status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Substream {
    Status,
    Health,
    Logs,
    Modes,
    Alerts,
    Notifications,
    System,
}

impl Substream {
    /// Every substream, the default when `?events=` is absent.
    pub const ALL: [Self; 7] = [
        Self::Status,
        Self::Health,
        Self::Logs,
        Self::Modes,
        Self::Alerts,
        Self::Notifications,
        Self::System,
    ];

    fn parse(token: &str) -> Option<Self> {
        match token {
            "status" => Some(Self::Status),
            "health" => Some(Self::Health),
            "logs" => Some(Self::Logs),
            "modes" => Some(Self::Modes),
            "alerts" => Some(Self::Alerts),
            "notifications" => Some(Self::Notifications),
            "system" => Some(Self::System),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SseQuery {
    /// Comma-separated substreams to subscribe to. All of them when absent.
    events: Option<String>,
}

/// Multiplexes every event kind onto one connection. Browsers cap concurrent connections
/// per origin over HTTP/1.1 (6 in Chrome, counted per profile rather than per tab), so a
/// client opening one stream per event kind starves its own ordinary requests.
pub async fn combined(
    Query(query): Query<SseQuery>,
    State(app_state): State<AppState>,
) -> Result<SseResponse, CCError> {
    let selected = parse_substreams(query.events.as_deref())?;
    Ok(sse_response(&app_state, &selected))
}

/// Maps `?events=` to its substreams. Rejects rather than silently ignoring an unknown
/// token, so a typo surfaces as a 400 instead of a stream that never delivers.
fn parse_substreams(events: Option<&str>) -> Result<Vec<Substream>, CCError> {
    let Some(events) = events else {
        return Ok(Substream::ALL.to_vec());
    };
    if events.trim().is_empty() {
        return Err(CCError::UserError {
            msg: "The 'events' parameter must name at least one event stream".to_string(),
        });
    }
    let mut selected = Vec::with_capacity(Substream::ALL.len());
    for token in events.split(',').map(str::trim) {
        let substream = Substream::parse(token).ok_or_else(|| CCError::UserError {
            msg: format!(
                "Unknown SSE event stream: '{token}'. \
                 Valid values: status, health, logs, modes, alerts, notifications, system"
            ),
        })?;
        if selected.contains(&substream).not() {
            selected.push(substream);
        }
    }
    debug_assert!(
        selected.is_empty().not(),
        "a non-empty events= yields a stream"
    );
    Ok(selected)
}

/// Builds the merged SSE response for the given substreams.
fn sse_response(app_state: &AppState, selected: &[Substream]) -> SseResponse {
    debug_assert!(selected.is_empty().not(), "a substream must be selected");
    let mut streams: Vec<EventStream> = Vec::with_capacity(selected.len());
    for substream in selected {
        streams.push(match substream {
            Substream::Status => status_stream(&app_state.status_handle),
            Substream::Health => health_stream(&app_state.device_health_handle),
            Substream::Logs => log_stream(&app_state.log_buf_handle),
            Substream::Modes => mode_stream(&app_state.mode_handle),
            Substream::Alerts => alert_stream(&app_state.alert_handle),
            Substream::Notifications => notification_stream(&app_state.notification_handle),
            Substream::System => system_stream(&app_state.system_event_handle),
        });
    }
    NoApi(Sse::new(select_all(streams)).keep_alive(
        KeepAlive::new().interval(Duration::from_secs(DEFAULT_KEEP_ALIVE_INTERVAL_SECONDS)),
    ))
}

/// DOWNGRADE-COMPAT(added 5.0.0, remove 5.2.0): see DEPRECATIONS.md. This and the other
/// per-stream handlers below back the legacy `/sse/{logs,status,modes,alerts,notifications}`
/// routes, superseded by `GET /sse?events=`. Kept so a 4.3.x UI still receives events.
pub async fn logs(State(app_state): State<AppState>) -> SseResponse {
    sse_response(&app_state, &[Substream::Logs])
}

fn log_stream(log_buf_handle: &LogBufHandle) -> EventStream {
    let stream = log_lines(log_buf_handle).map(|log| Ok(SseEvent::Log(log).into()));
    Box::pin(stream)
}

/// Live log lines for one subscriber. Bursts arrive pre-coalesced (multi-line) from the
/// log-buffer actor. A lagged subscriber skips missed lines instead of receiving empty
/// events; recent history remains available via GET /logs.
fn log_lines(log_buf_handle: &LogBufHandle) -> impl Stream<Item = String> + use<> {
    let cancel_token = log_buf_handle.cancel_token();
    BroadcastStream::new(log_buf_handle.broadcaster().subscribe())
        .take_until(async move { cancel_token.cancelled().await })
        .filter_map(|result| async { result.ok() })
}

pub async fn status(State(app_state): State<AppState>) -> SseResponse {
    sse_response(&app_state, &[Substream::Status, Substream::Health])
}

fn status_stream(status_handle: &StatusHandle) -> EventStream {
    let cancel_token = status_handle.cancel_token();
    let stream = BroadcastStream::new(status_handle.broadcaster().subscribe())
        .take_until(async move { cancel_token.cancelled().await })
        .map(|status| {
            Ok(SseEvent::Status(StatusResponse {
                devices: status.unwrap_or_default(),
            })
            .into())
        });
    Box::pin(stream)
}

fn health_stream(device_health_handle: &DeviceHealthHandle) -> EventStream {
    let cancel_token = device_health_handle.cancel_token();
    let subscription = device_health_handle.broadcaster().subscribe();
    let handle = device_health_handle.clone();
    let stream = BroadcastStream::new(subscription)
        .take_until(async move { cancel_token.cancelled().await })
        .then(move |event| {
            let health_handle = handle.clone();
            async move {
                let event = match event {
                    Ok(event) => health_event_to_sse(event),
                    // This consumer missed transitions; resync it with the full
                    // current snapshot instead of leaving it permanently stale.
                    Err(BroadcastStreamRecvError::Lagged(_)) => {
                        health_snapshot_to_sse(health_handle.get_all().await)
                    }
                };
                Ok(event)
            }
        });
    Box::pin(stream)
}

/// Maps one tick's device-health transition batch to its named SSE event
/// (`missing`, `stale-source`, `failsafe`, or `unreachable`).
fn health_event_to_sse(event: HealthEvent) -> Event {
    match event {
        HealthEvent::Missing(deltas) => SseEvent::Missing(deltas).into(),
        HealthEvent::StaleSource(deltas) => SseEvent::StaleSource(deltas).into(),
        HealthEvent::Failsafe(deltas) => SseEvent::Failsafe(deltas).into(),
        HealthEvent::Unreachable(deltas) => SseEvent::Unreachable(deltas).into(),
    }
}

/// Full-state `health` event sent to a consumer that lagged the broadcast
/// buffer, so it converges on the current state.
fn health_snapshot_to_sse(snapshot: DeviceHealthDto) -> Event {
    SseEvent::Health(snapshot).into()
}

pub async fn modes(State(app_state): State<AppState>) -> SseResponse {
    sse_response(&app_state, &[Substream::Modes])
}

fn mode_stream(mode_handle: &ModeHandle) -> EventStream {
    let cancel_token = mode_handle.cancel_token();
    let stream = BroadcastStream::new(mode_handle.broadcaster().subscribe())
        .take_until(async move { cancel_token.cancelled().await })
        .map(|mode_activated| Ok(SseEvent::Mode(mode_activated.unwrap_or_default()).into()));
    Box::pin(stream)
}

pub async fn alerts(State(app_state): State<AppState>) -> SseResponse {
    sse_response(&app_state, &[Substream::Alerts])
}

fn alert_stream(alert_handle: &AlertHandle) -> EventStream {
    let cancel_token = alert_handle.cancel_token();
    let stream = BroadcastStream::new(alert_handle.broadcaster().subscribe())
        .take_until(async move { cancel_token.cancelled().await })
        .map(|alert_state| Ok(SseEvent::Alert(alert_state.unwrap_or_default()).into()));
    Box::pin(stream)
}

pub async fn notifications(State(app_state): State<AppState>) -> SseResponse {
    sse_response(&app_state, &[Substream::Notifications])
}

fn notification_stream(notification_handle: &NotificationHandle) -> EventStream {
    let cancel_token = notification_handle.cancel_token();
    let stream = BroadcastStream::new(notification_handle.broadcaster().subscribe())
        .take_until(async move { cancel_token.cancelled().await })
        .filter_map(|result| async { result.ok() })
        .map(|notification| Ok(SseEvent::Notification(notification).into()));
    Box::pin(stream)
}

fn system_stream(system_event_handle: &SystemEventHandle) -> EventStream {
    let cancel_token = system_event_handle.cancel_token();
    let stream = BroadcastStream::new(system_event_handle.broadcaster().subscribe())
        .take_until(async move { cancel_token.cancelled().await })
        .filter_map(|result| async { result.ok() })
        .map(|event| Ok(SseEvent::System(event).into()));
    Box::pin(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notifier::NotificationIcon;
    use crate::system_event::SystemEventKind;

    use tokio_util::sync::CancellationToken;

    /// One value per variant. The match is exhaustive on purpose: a new `SseEvent` variant
    /// fails to compile here until it is added, which is what keeps the checks below total.
    fn sample_of_every_variant() -> Vec<SseEvent> {
        let all = vec![
            SseEvent::Status(StatusResponse {
                devices: Vec::new(),
            }),
            SseEvent::Missing(Vec::new()),
            SseEvent::StaleSource(Vec::new()),
            SseEvent::Failsafe(Vec::new()),
            SseEvent::Unreachable(Vec::new()),
            SseEvent::Health(DeviceHealthDto {
                failsafe: Vec::new(),
                unreachable: Vec::new(),
                missing: Vec::new(),
                stale_source: Vec::new(),
                firmware_overrides: Vec::new(),
                channel_capabilities: Vec::new(),
                system_findings: Vec::new(),
            }),
            SseEvent::Log("a log line\n".to_string()),
            SseEvent::Mode(ActiveMode::default()),
            SseEvent::Alert(AlertLog::default()),
            SseEvent::Notification(DesktopNotification {
                title: "t".to_string(),
                body: "b".to_string(),
                icon: NotificationIcon::Info,
                audio: false,
                urgency: 1,
            }),
            SseEvent::System(SystemEvent {
                kind: SystemEventKind::PowerProfile,
                value: "balanced".to_string(),
                previous: None,
            }),
        ];
        for event in &all {
            // Exhaustiveness check only; the compiler rejects a missing variant here.
            match event {
                SseEvent::Status(_)
                | SseEvent::Missing(_)
                | SseEvent::StaleSource(_)
                | SseEvent::Failsafe(_)
                | SseEvent::Unreachable(_)
                | SseEvent::Health(_)
                | SseEvent::Log(_)
                | SseEvent::Mode(_)
                | SseEvent::Alert(_)
                | SseEvent::Notification(_)
                | SseEvent::System(_) => {}
            }
        }
        all
    }

    /// The event name a frame carries, read back off the serialized SSE frame.
    fn wire_event_name(event: SseEvent) -> String {
        let frame = format!("{:?}", Event::from(event));
        let start = frame.find("event:").expect("every frame names its event") + "event:".len();
        let rest = &frame[start..];
        let end = rest
            .find("\\n")
            .expect("the event field is newline terminated");
        rest[..end].trim().to_string()
    }

    // Goal: the name on the wire and the name in the OpenAPI schema can never diverge.
    // They come from two places (the `From` match and serde's rename_all), and a rename in
    // only one of them would silently publish an event no client is listening for.
    // Methodology: for every variant, compare the SSE event field against the serde tag.
    #[test]
    fn sse_event_names_match_serde_tags() {
        for event in sample_of_every_variant() {
            let tag = serde_json::to_value(&event).expect("the DTO serializes")["event"]
                .as_str()
                .expect("the tag is a string")
                .to_string();
            let wire = wire_event_name(event);
            assert_eq!(wire, tag, "wire event name and serde tag diverged");
        }
    }

    // Goal: pin the exact strings. Both clients match on these literals (`DeviceStore.ts`
    // dispatch and the Qt `SseParser` handler), so a rename is a breaking change that must
    // be a deliberate edit here rather than a side effect of renaming a variant.
    #[test]
    fn sse_event_names_are_stable() {
        let names: Vec<String> = sample_of_every_variant()
            .into_iter()
            .map(wire_event_name)
            .collect();
        assert_eq!(
            names,
            vec![
                "status",
                "missing",
                "stale-source",
                "failsafe",
                "unreachable",
                "health",
                "log",
                "mode",
                "alert",
                "notification",
                "system",
            ]
        );
    }

    // Goal: a log frame carries raw text, not JSON. It is the one payload that is not
    // serialized, and wrapping it in quotes would break both clients' log handling.
    #[test]
    fn log_event_carries_raw_text() {
        let frame = format!("{:?}", Event::from(SseEvent::Log("plain text".to_string())));
        assert!(frame.contains("plain text"), "{frame}");
        assert!(
            frame.contains("\\\"plain text\\\"").not(),
            "the log line must not be JSON encoded: {frame}"
        );
    }

    // Goal: an absent events= subscribes to everything, so a client that asks for nothing
    // in particular keeps the old all-streams behavior.
    #[test]
    fn absent_events_selects_every_substream() {
        let selected = parse_substreams(None).expect("absent is valid");
        assert_eq!(selected, Substream::ALL.to_vec());
    }

    // Goal: every token maps to exactly its own substream and nothing else, since a token
    // that quietly selected the wrong stream would look like a daemon that never sends.
    #[test]
    fn each_token_selects_only_its_substream() {
        let cases = [
            ("status", Substream::Status),
            ("health", Substream::Health),
            ("logs", Substream::Logs),
            ("modes", Substream::Modes),
            ("alerts", Substream::Alerts),
            ("notifications", Substream::Notifications),
            ("system", Substream::System),
        ];
        assert_eq!(
            cases.len(),
            Substream::ALL.len(),
            "every substream needs a token case here"
        );
        for (token, expected) in cases {
            let selected = parse_substreams(Some(token)).expect("token is valid");
            assert_eq!(selected, vec![expected], "token '{token}' mis-mapped");
        }
    }

    // Goal: the multi-token form the UI and the legacy /sse/status alias rely on, including
    // whitespace tolerance and de-duplication of a repeated token.
    #[test]
    fn multiple_tokens_parse_in_order_without_duplicates() {
        let selected = parse_substreams(Some("status, health ,status")).expect("valid");
        assert_eq!(selected, vec![Substream::Status, Substream::Health]);
    }

    // Goal: negative space. A typo must fail loudly at the boundary and name the offending
    // token, rather than opening a connection that silently delivers nothing.
    #[test]
    fn unknown_token_is_rejected_and_named() {
        let err = parse_substreams(Some("status,bogus")).expect_err("bogus is invalid");
        let CCError::UserError { msg } = err else {
            panic!("an unknown token must be a UserError, which maps to 400");
        };
        assert!(
            msg.contains("bogus"),
            "the message must name the token: {msg}"
        );
    }

    // Goal: negative space. `?events=` with no value is a client mistake, not a request for
    // every stream and not a connection that yields nothing.
    #[test]
    fn empty_events_is_rejected() {
        for empty in ["", "  "] {
            let err = parse_substreams(Some(empty)).expect_err("empty is invalid");
            assert!(matches!(err, CCError::UserError { .. }));
        }
    }

    // Goal: merged substreams both deliver on one connection, tagged with their own event
    // names, which is the whole point of the combined endpoint.
    // Methodology: merge the two substreams that can be built without a full AppState,
    // publish one item on each, and check both arrive with the right event names.
    #[test]
    fn merged_substreams_deliver_both_event_kinds() {
        crate::rt::test_runtime(async {
            let cancel_token = CancellationToken::new();
            let log_handle = LogBufHandle::new(cancel_token.clone());
            let notification_handle = NotificationHandle::new(cancel_token);
            let mut merged = std::pin::pin!(select_all(vec![
                log_stream(&log_handle),
                notification_stream(&notification_handle),
            ]));
            log_handle
                .broadcaster()
                .send("a log line\n".to_string())
                .expect("subscriber exists");
            notification_handle
                .broadcaster()
                .send(DesktopNotification {
                    title: "t".to_string(),
                    body: "b".to_string(),
                    icon: NotificationIcon::Info,
                    audio: false,
                    urgency: 1,
                })
                .expect("subscriber exists");
            let mut seen = Vec::with_capacity(2);
            for _ in 0..2 {
                let event = merged.next().await.expect("an event is pending");
                let event = event.expect("the substreams are Infallible");
                // Event has no accessors, so read the wire form it will serialize to.
                seen.push(format!("{event:?}"));
            }
            let wire = seen.join("");
            assert!(
                wire.contains("log"),
                "the log substream must deliver: {wire}"
            );
            assert!(
                wire.contains("notification"),
                "the notification substream must deliver: {wire}"
            );
        });
    }

    // Goal: a subscriber that lags the broadcast channel must silently skip missed lines,
    // never yielding empty items (the old behavior mapped Lagged to empty SSE events).
    // Methodology: subscribe first, overflow the channel far past capacity, then drain until
    // the newest line arrives and check everything yielded is a real line with a gap skipped.
    #[test]
    fn lagged_subscriber_skips_missed_lines() {
        crate::rt::test_runtime(async {
            let handle = LogBufHandle::new(CancellationToken::new());
            let mut lines = std::pin::pin!(log_lines(&handle));
            let sent_count = 40;
            for i in 0..sent_count {
                handle
                    .broadcaster()
                    .send(format!("line {i}\n"))
                    .expect("subscriber exists");
            }
            let last_line = format!("line {}\n", sent_count - 1);
            let mut received = Vec::new();
            for _ in 0..sent_count {
                let Some(line) = lines.next().await else {
                    break;
                };
                let is_last = line == last_line;
                received.push(line);
                if is_last {
                    break;
                }
            }
            assert!(received.len() < sent_count, "lag must have skipped lines");
            assert!(received.iter().all(|line| line.starts_with("line ")));
            assert_eq!(received.last(), Some(&last_line));
        });
    }
}
