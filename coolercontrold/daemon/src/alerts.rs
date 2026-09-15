// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::api::actor::AlertHandle;
use crate::api::CCError;
use crate::calibration::{CalibrationAlertGate, DiagnosisRegistry};
use crate::device::UID;
use crate::notifier::{self, NotificationHandle, NotificationIcon};
use crate::overrides::OverridesController;
use crate::paths;
#[cfg(not(test))]
use crate::repositories::utils::{ShellCommand, ShellCommandResult};
use crate::setting::{ChannelMetric, ChannelSource};
use crate::{cc_fs, rt, AllDevices};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Local};
use indexmap::IndexMap;
use log::{info, trace, warn};
use moro_local::Scope;
use schemars::JsonSchema;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fmt;
use std::ops::Not;
use std::rc::Rc;
use std::time::{Duration, Instant};
use strum::{Display, EnumString};
use tokio_util::sync::CancellationToken;

const LOG_BUFFER_SIZE: usize = 1000;

/// Upper bound on sources per alert; enforced at the API boundary.
pub const MAX_ALERT_SOURCES: usize = 32;

/// Floor for a non-zero repeat interval so a typo cannot notify every tick.
pub const MIN_REPEAT_INTERVAL_SECONDS: f64 = 60.0;

/// Minimum interval between consecutive alert-log disk flushes to avoid
/// excessive I/O when alerts are firing rapidly. The very first state change
/// after a quiet period is always flushed immediately.
const LOG_FLUSH_COOLDOWN: Duration = Duration::from_secs(5);
const COMMAND_SHUTDOWN: &str =
    "shutdown +1 \"Critical CoolerControl Alert! System will shutdown in 1 minute.\"";
const COMMAND_SHUTDOWN_CANCEL: &str = "shutdown -c";

/// Log text for an edit that cleared the alert, and for one that made every
/// source readable again. True whether the source was removed or replaced.
const MESSAGE_TRIGGER_UNWATCHED: &str = "The triggering sensor is no longer watched";
const MESSAGE_UNREADABLE_UNWATCHED: &str = "The unreadable sensor is no longer watched";

pub type AlertName = String;
pub type AlertLogMessage = String;

/// Runs before `AlertController` loads the file, so no reload is needed. A textual replacement
/// is sound: a UID is 64 hex chars and cannot be a substring of anything else here.
pub async fn migrate_device_uid_in_file(legacy: &UID, current: &UID) -> Result<bool> {
    migrate_device_uid_in(paths::alert_config_file(), legacy, current).await
}

/// Target injected so it is testable without writing to the real config directory.
async fn migrate_device_uid_in(
    path: &std::path::Path,
    legacy: &UID,
    current: &UID,
) -> Result<bool> {
    debug_assert_eq!(legacy.len(), 64);
    debug_assert_eq!(current.len(), 64);
    debug_assert_ne!(legacy, current);
    let Ok(contents) = cc_fs::read_txt(path).await else {
        return Ok(false);
    };
    if contents.contains(legacy.as_str()).not() {
        return Ok(false);
    }
    if contents.contains(current.as_str()) {
        return Ok(false);
    }
    let migrated = contents.replace(legacy.as_str(), current.as_str());
    cc_fs::write(path, migrated.into_bytes()).await?;
    Ok(true)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
// The bools are independent user toggles persisted in alerts.json, not a state machine
// that could be folded into an enum: each one is set on its own in the UI.
#[allow(clippy::struct_excessive_bools)]
pub struct Alert {
    pub uid: UID,
    pub name: AlertName,

    /// DOWNGRADE-COMPAT(added 5.0.0, remove 5.2.0): 4.3.x hard-requires a single
    /// `channel_source` in alerts.json; kept written as `channel_sources[0]`.
    pub channel_source: ChannelSource,

    /// All watched sources. Every entry shares the same `ChannelMetric`.
    /// Empty in pre-4.4.0 files; seeded from `channel_source` by `normalize_sources`.
    #[serde(default)]
    pub channel_sources: Vec<ChannelSource>,

    pub min: f64,
    pub max: f64,

    /// The worst of the per-source states: Error > Active > Inactive.
    pub state: AlertState,

    /// Time in seconds throughout which the alert condition must hold before the alert is
    /// activated.
    ///
    /// For backwards compatibility, default to 0 to
    ///  a) tolerate missing fields and
    ///  b) preserve the previous behavior.
    /// New instances will default to 1 second.
    #[serde(default)]
    pub warmup_duration: f64,

    /// Time in seconds the value must stay back in range before an Active source clears.
    /// 0 clears immediately (the previous behavior).
    #[serde(default)]
    pub cooldown_duration: f64,

    /// Seconds between repeated notifications while a source stays Active. 0 disables repeats.
    #[serde(default)]
    pub repeat_interval: f64,

    /// A disabled alert is not evaluated at all.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// While set to a future time: states are still evaluated and logged,
    /// but notifications and shutdown commands are suppressed.
    #[serde(default)]
    pub silenced_until: Option<DateTime<Local>>,

    /// Toggle a desktop notification when this alert enters an `Active` state. (enabled by default)
    #[serde(default = "default_true")]
    pub desktop_notify: bool,

    /// Toggle a desktop notification when this alert enters an `Inactive` state. (enabled by default)
    #[serde(default = "default_true")]
    pub desktop_notify_recovery: bool,

    /// Toggle whether the desktop notification attempts to play an audio sound
    /// when this alert enters an `Active` state.
    /// Note: only applies when `desktop_notify` is enabled.
    #[serde(default)]
    pub desktop_notify_audio: bool,

    /// Toggle whether to issue a system shutdown when this Alert enters an `Active` state.
    #[serde(default)]
    pub shutdown_on_activation: bool,

    /// Runtime per-source states, parallel to `channel_sources`.
    #[serde(skip)]
    pub source_states: Vec<AlertState>,

    /// Runtime: when the last triggered/repeat notification went out, for `repeat_interval`.
    #[serde(skip)]
    pub last_notified: Option<DateTime<Local>>,

    /// Runtime: the user has been informed of the current Active episode (a log/toast
    /// went out unsilenced). Gates recovery notifications and the silence-expiry catch-up.
    #[serde(skip)]
    pub notified: bool,

    /// Runtime: the same, for the independent Error episode.
    #[serde(skip)]
    pub error_notified: bool,

    /// Runtime: a shutdown command was issued and not yet cancelled.
    #[serde(skip)]
    pub shutdown_scheduled: bool,
}

fn default_true() -> bool {
    true
}

impl Alert {
    /// Enforces the source invariants: `channel_sources` is the authoritative list,
    /// legacy `channel_source` mirrors its first entry, and the runtime per-source
    /// states stay parallel.
    pub fn normalize_sources(&mut self) {
        if self.channel_sources.is_empty() {
            self.channel_sources.push(self.channel_source.clone());
        } else {
            self.channel_source = self.channel_sources[0].clone();
        }
        // The API rejects an over-long list, but a hand-edited alerts.json reaches this
        // without passing it, and every tick walks these sources. Trim rather than refuse
        // the file: one malformed alert must not stop the daemon from loading the rest.
        if self.channel_sources.len() > MAX_ALERT_SOURCES {
            warn!(
                "Alert {} lists {} sources, above the maximum of {MAX_ALERT_SOURCES}. \
                 Keeping the first {MAX_ALERT_SOURCES}.",
                self.name,
                self.channel_sources.len()
            );
            self.channel_sources.truncate(MAX_ALERT_SOURCES);
        }
        if self.source_states.len() != self.channel_sources.len() {
            self.source_states = vec![AlertState::Inactive; self.channel_sources.len()];
        }
        assert!(self.channel_sources.is_empty().not());
        assert!(self.channel_sources.len() <= MAX_ALERT_SOURCES);
        assert_eq!(self.source_states.len(), self.channel_sources.len());
    }

    /// `true` while a user-set silence timestamp lies in the future.
    pub fn is_silenced(&self) -> bool {
        self.silenced_until
            .is_some_and(|until| Local::now() < until)
    }

    /// The worst wire-visible state across all sources: Error > Active > Inactive.
    fn worst_of_visible(&self) -> AlertState {
        let mut worst = AlertState::Inactive;
        for state in &self.source_states {
            match state.visible() {
                AlertState::Error => return AlertState::Error,
                AlertState::Active => worst = AlertState::Active,
                _ => {}
            }
        }
        worst
    }

    fn any_source_visible_active(&self) -> bool {
        self.source_states
            .iter()
            .any(|state| state.visible() == AlertState::Active)
    }

    fn any_source_error(&self) -> bool {
        self.source_states
            .iter()
            .any(|state| state.visible() == AlertState::Error)
    }

    /// Taken before and after a tick; the difference is what may be announced.
    fn machine_state(&self) -> MachineState {
        MachineState {
            active: self.any_source_visible_active(),
            error: self.any_source_error(),
        }
    }

    /// Advances a single source state machine for one tick.
    /// Warmup gates the firing edge; cooldown mirrors it on the clear edge.
    fn transition_source(
        state: AlertState,
        in_range: bool,
        warmup_duration: f64,
        cooldown_duration: f64,
    ) -> AlertState {
        if in_range {
            match state {
                AlertState::Active => {
                    if cooldown_duration > 0.0 {
                        AlertState::Cooldown(Local::now())
                    } else {
                        AlertState::Inactive
                    }
                }
                AlertState::Cooldown(since) => {
                    let elapsed = Local::now().signed_duration_since(since).as_seconds_f64();
                    if elapsed >= cooldown_duration {
                        AlertState::Inactive
                    } else {
                        AlertState::Cooldown(since)
                    }
                }
                AlertState::WarmUp(_) | AlertState::Inactive | AlertState::Error => {
                    AlertState::Inactive
                }
            }
        } else {
            match state {
                // A source in Cooldown never stopped firing; return to Active silently.
                AlertState::Active | AlertState::Cooldown(_) => AlertState::Active,
                AlertState::WarmUp(since) => {
                    let elapsed = Local::now().signed_duration_since(since).as_seconds_f64();
                    if elapsed >= warmup_duration {
                        AlertState::Active
                    } else {
                        AlertState::WarmUp(since)
                    }
                }
                // Error with a fresh value means the error resolved; warm up as usual.
                AlertState::Inactive | AlertState::Error => AlertState::WarmUp(Local::now()),
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Display, EnumString, JsonSchema)]
pub enum AlertState {
    Active,

    /// Alert condition was satisfied at the stored time
    /// but the duration threshold has not been reached.
    WarmUp(DateTime<Local>),

    /// Condition cleared at the stored time but the cooldown duration
    /// has not elapsed; still presented as Active.
    Cooldown(DateTime<Local>),

    Inactive,

    /// Represents an error state. e.g. when one of the components in the alert isn't found.
    Error,
}

impl AlertState {
    /// Collapses the internal timer states to the wire-visible tri-state.
    fn visible(self) -> AlertState {
        match self {
            AlertState::WarmUp(_) => AlertState::Inactive,
            AlertState::Cooldown(_) => AlertState::Active,
            other => other,
        }
    }
}

impl Serialize for AlertState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match *self {
            AlertState::Active | AlertState::Cooldown(_) => serializer.serialize_str("Active"),
            AlertState::Error => serializer.serialize_str("Error"),
            AlertState::Inactive | AlertState::WarmUp(_) => serializer.serialize_str("Inactive"),
        }
    }
}

impl<'de> Deserialize<'de> for AlertState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AlertStateVisitor;

        impl Visitor<'_> for AlertStateVisitor {
            type Value = AlertState;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string representing an alert state variant")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                match value {
                    // Cooldown is never persisted as such, but map it defensively.
                    "Active" | "Cooldown" => Ok(AlertState::Active),
                    "Error" => Ok(AlertState::Error),
                    "WarmUp" | "Inactive" => Ok(AlertState::Inactive),
                    _ => Err(E::custom(format!("unknown variant: {value}"))),
                }
            }
        }

        deserializer.deserialize_str(AlertStateVisitor)
    }
}

/// What a log entry represents. `state` is the worst of all sources, so it cannot
/// say which machine moved: a trigger beside an Error is `state: Error` too.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum AlertLogKind {
    Triggered,
    StillActive,
    Resolved,
    Error,
    ErrorResolved,
    /// Written before this field existed; clients fall back to `state`.
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AlertLog {
    pub uid: UID,
    pub name: AlertName,
    pub state: AlertState,
    pub message: AlertLogMessage,
    pub timestamp: DateTime<Local>,

    /// The state change happened while the alert was silenced;
    /// clients update state from it but raise no toast.
    #[serde(default)]
    pub silenced: bool,

    // DOWNGRADE-COMPAT(added 5.0.0, remove 5.2.0): derivable from `kind`, but a
    // 5.0.x daemon reading alert-logs.json still expects it. See DEPRECATIONS.md.
    #[serde(default)]
    pub resolved: bool,

    /// What happened, independent of whether it interrupts the user.
    #[serde(default)]
    pub kind: AlertLogKind,

    /// A per-source detail, not an alert-level change: record it, raise no toast.
    #[serde(default)]
    pub quiet: bool,
}

impl Default for AlertLog {
    fn default() -> Self {
        AlertLog {
            uid: "Unknown".to_string(),
            name: "Unknown".to_string(),
            state: AlertState::Active,
            message: "Unknown".to_string(),
            timestamp: Local::now(),
            silenced: false,
            resolved: false,
            kind: AlertLogKind::Unknown,
            quiet: false,
        }
    }
}

/// Per-source transition messages collected during one evaluation tick.
#[derive(Default)]
struct SourceOutcomes {
    fired: Vec<AlertLogMessage>,
    resolved: Vec<AlertLogMessage>,
    errors: Vec<AlertLogMessage>,
    /// Messages for sources currently Active; only collected when a silence-expiry
    /// catch-up, shutdown re-arm, or repeat notification could consume them.
    out_of_range: Vec<AlertLogMessage>,
    /// Only collected when the Error catch-up could consume them.
    unreadable: Vec<AlertLogMessage>,
    /// A calibration-suppression reset changed a source state this tick.
    suppressed: bool,
}

impl SourceOutcomes {
    fn has_transitions(&self) -> bool {
        (self.fired.is_empty() && self.resolved.is_empty() && self.errors.is_empty()).not()
    }
}

/// Which source transitions produce a user-visible message. Timer-state hops
/// (into `WarmUp`, `WarmUp` back down, `Active`<->`Cooldown`) stay silent.
enum TransitionKind {
    Fired,
    Resolved,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum AlertEventKind {
    Triggered,
    Resolved,
    SourceError,
    /// A silenced episode is still active after the silence lapsed.
    StillActive,
    /// Periodic re-notification while Active; skips the log.
    Repeat,
    /// Every source is readable again; the alert may still be Active.
    ErrorResolved,
}

impl From<AlertEventKind> for AlertLogKind {
    fn from(kind: AlertEventKind) -> Self {
        match kind {
            AlertEventKind::Triggered => AlertLogKind::Triggered,
            AlertEventKind::Resolved => AlertLogKind::Resolved,
            AlertEventKind::SourceError => AlertLogKind::Error,
            AlertEventKind::StillActive | AlertEventKind::Repeat => AlertLogKind::StillActive,
            AlertEventKind::ErrorResolved => AlertLogKind::ErrorResolved,
        }
    }
}

/// One coalesced, fully-decided outcome for an alert on one tick.
/// Built by the evaluation pass; executed verbatim by `send_notifications`.
// Each bool is a separate decision the evaluation pass already made, which
// `send_notifications` then executes verbatim; collapsing them would re-introduce the
// branching this type exists to remove.
#[allow(clippy::struct_excessive_bools)]
struct AlertEvent {
    alert: Alert,
    message: AlertLogMessage,
    kind: AlertEventKind,
    silenced: bool,
    notify_desktop: bool,
    fire_shutdown: bool,
    cancel_shutdown: bool,
    /// Repeat notifications skip the log to keep the ring buffer clean.
    log: bool,
    /// A per-source detail with no alert-level edge: logged, never announced.
    quiet: bool,
}

/// The two independent announcement machines over all source states. Error is
/// separate because "cannot be trusted" is worth announcing while firing.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct MachineState {
    active: bool,
    error: bool,
}

// Shell commands the alert side effects would have run, for assertions in tests.
#[cfg(test)]
thread_local! {
    static FIRED_COMMANDS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

pub struct AlertController {
    all_devices: AllDevices,
    overrides: Rc<OverridesController>,
    diagnosis_registry: Rc<DiagnosisRegistry>,
    alerts: RefCell<IndexMap<UID, Alert>>,
    alert_handle: RefCell<Option<AlertHandle>>,
    notification_handle: RefCell<Option<NotificationHandle>>,
    logs: RefCell<VecDeque<AlertLog>>,
    logs_dirty: Cell<bool>,
    last_log_flush: Cell<Instant>,
}

impl AlertController {
    /// A controller for managing and handling Alerts.
    pub async fn init(
        all_devices: AllDevices,
        overrides: Rc<OverridesController>,
        diagnosis_registry: Rc<DiagnosisRegistry>,
    ) -> Result<Self> {
        let alert_controller = Self {
            all_devices,
            overrides,
            diagnosis_registry,
            alerts: RefCell::new(IndexMap::new()),
            alert_handle: RefCell::new(None),
            notification_handle: RefCell::new(None),
            logs: RefCell::new(VecDeque::with_capacity(LOG_BUFFER_SIZE)),
            logs_dirty: Cell::new(false),
            // Set to a past time so the first state change always flushes immediately.
            #[allow(clippy::unchecked_time_subtraction)]
            last_log_flush: Cell::new(Instant::now() - LOG_FLUSH_COOLDOWN),
        };
        alert_controller.load_data_from_alert_config_file().await?;
        Ok(alert_controller)
    }

    /// Watches for shutdown and saves the current Alert data to the Alert configuration file.
    pub fn watch_for_shutdown<'s>(
        controller: &Rc<AlertController>,
        cancellation_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) {
        let alert_controller = controller.clone();
        main_scope.spawn(async move {
            cancellation_token.cancelled().await;
            trace!("Shutting down Alert Controller");
            let _ = alert_controller.save_alert_data_to_config().await;
            let _ = alert_controller.save_alert_logs().await;
        });
    }

    /// Sets the `AlertHandle` for the `AlertController`.
    ///
    /// The `AlertHandle` is used to broadcast notifications when an `Alert` state changes.
    pub fn set_alert_handle(&self, alert_handle: AlertHandle) {
        self.alert_handle.replace(Some(alert_handle));
    }

    /// Sets the `NotificationHandle` for broadcasting desktop notification
    /// events to connected SSE clients.
    pub fn set_notification_handle(&self, handle: NotificationHandle) {
        self.notification_handle.replace(Some(handle));
    }

    /// Reads the Alert configuration file and fills the alert map and log buffer.
    async fn load_data_from_alert_config_file(&self) -> Result<()> {
        let config_dir = paths::config_dir();
        if !config_dir.exists() {
            info!(
                "config directory doesn't exist. Attempting to create it: {}",
                config_dir.display()
            );
            cc_fs::create_dir_all(config_dir).await?;
        }
        let path = paths::alert_config_file().to_path_buf();
        let config_contents = if let Ok(contents) = cc_fs::read_txt(&path).await {
            contents
        } else {
            info!("Writing a new Alerts configuration file");
            let default_json = serde_json::to_string(&AlertConfigFile {
                alerts: Vec::new(),
                logs: Vec::new(),
            })?;
            cc_fs::write_string(&path, default_json)
                .await
                .map_err(|err| {
                    anyhow!("Writing new configuration file: {} - {err}", path.display())
                })?;
            cc_fs::read_txt(&path)
                .await
                .map_err(|err| anyhow!("Reading configuration file {} - {err}", path.display()))?
        };
        let alert_config: AlertConfigFile =
            serde_json::from_str(&config_contents).map_err(|err| {
                anyhow!(
                    "Parsing Alert configuration file {} - {err}",
                    path.display()
                )
            })?;
        {
            let mut alerts_lock = self.alerts.borrow_mut();
            alerts_lock.clear();
            for mut alert in alert_config.alerts {
                Self::reset_saved_alert_state(&mut alert);
                alerts_lock.insert(alert.uid.clone(), alert);
            }
        }
        let logs = Self::load_logs_from_data_dir(alert_config.logs).await;
        {
            let mut logs_lock = self.logs.borrow_mut();
            logs_lock.clear();
            logs_lock.extend(logs);
        }
        Ok(())
    }

    /// Loads alert logs from the data-dir file, migrating from the legacy combined
    /// config file on the first run after upgrade if the data-dir file does not yet exist.
    async fn load_logs_from_data_dir(legacy_logs: Vec<AlertLog>) -> Vec<AlertLog> {
        let path = paths::alert_logs_file();
        if path.exists() {
            let contents = cc_fs::read_txt(path).await.unwrap_or_default();
            return serde_json::from_str::<AlertLogsFile>(&contents)
                .map(|f| f.logs)
                .unwrap_or_default();
        }
        if legacy_logs.is_empty() {
            return Vec::new();
        }
        info!(
            "Migrating {} alert log(s) to {}",
            legacy_logs.len(),
            path.display()
        );
        match serde_json::to_string(&AlertLogsFile {
            logs: legacy_logs.clone(),
        }) {
            Ok(json) => {
                if let Err(err) = cc_fs::write_string(path, json).await {
                    warn!("Failed to write migrated alert logs: {err}");
                }
            }
            Err(err) => warn!("Failed to serialize migrated alert logs: {err}"),
        }
        legacy_logs
    }

    /// Resets all runtime state of an alert to Inactive.
    /// We want to re-evaluate the state of all Alerts on startup, so we reset the saved state.
    /// Note that we are still serializing the states properly for the Alert Logs.
    fn reset_saved_alert_state(alert: &mut Alert) {
        alert.state = AlertState::Inactive;
        alert.normalize_sources();
        alert.source_states.fill(AlertState::Inactive);
        alert.notified = false;
        alert.error_notified = false;
        alert.last_notified = None;
        alert.shutdown_scheduled = false;
    }

    /// Saves alert configuration (thresholds, settings) to `/etc/coolercontrol/alerts.json`.
    /// Logs are intentionally excluded; use `save_alert_logs` for those.
    async fn save_alert_data_to_config(&self) -> Result<()> {
        let alert_config = AlertConfigFile {
            alerts: self.alerts.borrow().values().cloned().collect(),
            logs: Vec::new(),
        };
        let alert_config_json = serde_json::to_string(&alert_config)?;
        cc_fs::write_string(paths::alert_config_file(), alert_config_json)
            .await
            .map_err(|err| anyhow!("Writing Alert Configuration File - {err}"))
    }

    /// Saves the in-memory alert log buffer to `/var/lib/coolercontrol/alert-logs.json`.
    async fn save_alert_logs(&self) -> Result<()> {
        let logs: Vec<AlertLog> = self.logs.borrow().iter().cloned().collect();
        let json = serde_json::to_string(&AlertLogsFile { logs })?;
        cc_fs::write_string(paths::alert_logs_file(), json)
            .await
            .map_err(|err| anyhow!("Writing Alert Logs File - {err}"))
    }

    /// Returns a tuple of all available Alerts and logs: (alerts, logs)
    pub fn get_all(&self) -> (Vec<Alert>, Vec<AlertLog>) {
        let alerts = self.alerts.borrow().values().cloned().collect();
        let logs = self.logs.borrow().iter().cloned().collect();
        (alerts, logs)
    }

    /// Creates a new Alert
    pub async fn create(&self, mut alert: Alert) -> Result<()> {
        alert.normalize_sources();
        if self.alerts.borrow().contains_key(&alert.uid) {
            return Err(CCError::UserError {
                msg: format!("Alert with uid {} already exists", alert.uid),
            }
            .into());
        }
        self.alerts.borrow_mut().insert(alert.uid.clone(), alert);
        self.save_alert_data_to_config().await
    }

    /// Runtime state carries over per source, so an edit never re-arms a kept source.
    /// An edge crossed by the edit is announced here because no later tick would.
    pub async fn update(&self, mut alert: Alert) -> Result<()> {
        alert.normalize_sources();
        let announcement = {
            let mut alerts_lock = self.alerts.borrow_mut();
            let Some(existing_alert) = alerts_lock.get(&alert.uid) else {
                return Err(CCError::NotFound {
                    msg: format!("Alert with uid {} does not exist", alert.uid),
                }
                .into());
            };
            let before = existing_alert.machine_state();
            if alert.channel_sources == existing_alert.channel_sources {
                Self::carry_runtime_state(&mut alert, existing_alert);
            } else {
                Self::carry_surviving_source_states(&mut alert, existing_alert);
                Self::cancel_shutdown_on_source_change(&alert, existing_alert);
            }
            Self::cancel_shutdown_if_unwanted(&mut alert);
            if alert.enabled.not() {
                Self::reset_saved_alert_state(&mut alert);
            }
            let announcement = Self::build_edit_event(&mut alert, before);
            alerts_lock.insert(alert.uid.clone(), alert);
            announcement
        };
        if let Some(event) = announcement {
            self.dispatch_event(&event);
        }
        self.save_alert_data_to_config().await
    }

    /// An edit that crossed an edge, using a tick's machines and gates.
    /// A disabled alert stays silent, as it always has.
    /// Which machine the edit moved, so the text names the sensor that left.
    fn edit_message(kind: AlertEventKind) -> &'static str {
        match kind {
            AlertEventKind::ErrorResolved => MESSAGE_UNREADABLE_UNWATCHED,
            _ => MESSAGE_TRIGGER_UNWATCHED,
        }
    }

    fn build_edit_event(alert: &mut Alert, before: MachineState) -> Option<AlertEvent> {
        if alert.enabled.not() {
            return None;
        }
        debug_assert_eq!(alert.channel_sources.len(), alert.source_states.len());
        let after = alert.machine_state();
        let kind = Self::edge_kind(before, after)?;
        let silenced = alert.is_silenced();
        let notify_desktop = Self::apply_edge(alert, kind, silenced, false);
        Self::clear_finished_episodes(alert, after);
        debug_assert!(after.active || alert.notified.not());
        Some(AlertEvent {
            alert: alert.clone(),
            message: Self::edit_message(kind).to_string(),
            kind,
            silenced,
            notify_desktop,
            fire_shutdown: false,
            cancel_shutdown: false,
            log: true,
            quiet: false,
        })
    }

    /// Server-authoritative runtime state that a plain settings edit must not
    /// overwrite. The next tick re-evaluates it against the new thresholds and
    /// announces any transition itself.
    fn carry_runtime_state(alert: &mut Alert, existing_alert: &Alert) {
        alert.state = existing_alert.state;
        alert
            .source_states
            .clone_from(&existing_alert.source_states);
        alert.notified = existing_alert.notified;
        alert.error_notified = existing_alert.error_notified;
        alert.last_notified = existing_alert.last_notified;
        alert.shutdown_scheduled = existing_alert.shutdown_scheduled;
    }

    /// A kept source keeps its state, an added one starts Inactive, and the aggregate
    /// follows the survivors, which is what clears an alert whose firing source went.
    fn carry_surviving_source_states(alert: &mut Alert, existing_alert: &Alert) {
        debug_assert_eq!(
            existing_alert.channel_sources.len(),
            existing_alert.source_states.len()
        );
        debug_assert_eq!(alert.channel_sources.len(), alert.source_states.len());
        for (index, source) in alert.channel_sources.iter().enumerate() {
            let Some(previous) = existing_alert
                .channel_sources
                .iter()
                .position(|kept_source| kept_source == source)
            else {
                continue;
            };
            alert.source_states[index] = existing_alert.source_states[previous];
        }
        alert.state = alert.worst_of_visible();
        alert.last_notified = existing_alert.last_notified;
        alert.notified = existing_alert.notified;
        alert.error_notified = existing_alert.error_notified;
    }

    /// A pending shutdown does not survive a source-set edit: the user gets a fresh
    /// countdown instead of the remainder of one armed for a set they have changed.
    /// `shutdown_scheduled` stays false, so the tick loop re-arms it while a
    /// surviving source still holds the alert active.
    fn cancel_shutdown_on_source_change(alert: &Alert, existing_alert: &Alert) {
        if existing_alert.shutdown_scheduled.not() {
            return;
        }
        Self::fire_command(COMMAND_SHUTDOWN_CANCEL);
        info!(
            "Alert sources changed: {} - pending shutdown cancelled",
            alert.name
        );
    }

    /// True when a pending shutdown is no longer wanted, because the alert was
    /// disabled, silenced, or had its shutdown behaviour turned off while the
    /// countdown was running.
    fn shutdown_pending_unwanted(alert: &Alert) -> bool {
        if alert.shutdown_scheduled.not() {
            return false;
        }
        if alert.enabled.not() {
            return true;
        }
        if alert.is_silenced() {
            return true;
        }
        alert.shutdown_on_activation.not()
    }

    /// A disable, a silence, or a shutdown-behaviour opt-out applied while a shutdown
    /// is pending cancels it, otherwise the machine still halts a minute after the user
    /// acted to stop it. Clearing `shutdown_scheduled` is what keeps a later update from
    /// firing a second cancel.
    fn cancel_shutdown_if_unwanted(alert: &mut Alert) {
        if Self::shutdown_pending_unwanted(alert).not() {
            return;
        }
        Self::fire_command(COMMAND_SHUTDOWN_CANCEL);
        alert.shutdown_scheduled = false;
        info!("Alert quieted: {} - pending shutdown cancelled", alert.name);
    }

    /// Deletes an existing Alert
    pub async fn delete(&self, alert_uid: UID) -> Result<()> {
        let Some(alert) = self.alerts.borrow_mut().shift_remove(&alert_uid) else {
            return Err(CCError::NotFound {
                msg: format!("Alert with uid {alert_uid} does not exist"),
            }
            .into());
        };
        Self::cancel_shutdown_on_delete(&alert);
        self.save_alert_data_to_config().await
    }

    /// A deleted Alert can no longer resolve its own pending shutdown, so removing it
    /// has to cancel one. Deleting an Alert to stop an imminent shutdown is the obvious
    /// reading of the action, and without this the machine halts anyway.
    fn cancel_shutdown_on_delete(alert: &Alert) {
        if alert.shutdown_scheduled.not() {
            return;
        }
        Self::fire_command(COMMAND_SHUTDOWN_CANCEL);
        info!("Alert deleted: {} - pending shutdown cancelled", alert.name);
    }

    /// Processes all Alerts, firing off messages if an alert state has changed.
    /// This function should be called in the main loop.
    pub fn process_alerts(&self) {
        let events = self.process_and_collect_alerts_to_fire();
        for event in &events {
            self.dispatch_event(event);
        }
        self.flush_logs_if_needed();
    }

    /// Flushes alert logs to disk. The first state change after a quiet period
    /// is flushed immediately so a crash-causing event is always persisted.
    /// Subsequent rapid changes are rate-limited by `LOG_FLUSH_COOLDOWN`.
    fn flush_logs_if_needed(&self) {
        if !self.logs_dirty.get() {
            return;
        }
        let elapsed = self.last_log_flush.get().elapsed();
        if elapsed < LOG_FLUSH_COOLDOWN {
            return;
        }
        self.logs_dirty.set(false);
        self.last_log_flush.set(Instant::now());
        rt::spawn({
            let logs: Vec<AlertLog> = self.logs.borrow().iter().cloned().collect();
            async move {
                match serde_json::to_string(&AlertLogsFile { logs }) {
                    Ok(json) => {
                        if let Err(err) = cc_fs::write_string(paths::alert_logs_file(), json).await
                        {
                            warn!("Failed to flush alert logs to disk: {err}");
                        }
                    }
                    Err(err) => warn!("Failed to serialize alert logs: {err}"),
                }
            }
        });
    }

    /// Collects one fully-decided event per alert that needs firing this tick.
    fn process_and_collect_alerts_to_fire(&self) -> Vec<AlertEvent> {
        let mut alerts = self.alerts.borrow_mut();
        // Runs every tick and at most one event per alert is produced, so the exact
        // capacity is known up front.
        let mut events = Vec::with_capacity(alerts.len());
        for alert in alerts.values_mut() {
            if alert.enabled.not() {
                continue;
            }
            let before = alert.machine_state();
            let outcomes = self.evaluate_sources(alert);
            if let Some(event) = Self::process_alert(alert, &outcomes, before) {
                events.push(event);
            }
        }
        events
    }

    /// Refreshes the aggregate state from the source states, then produces this tick's
    /// event, if any. The two belong together: the aggregate must follow the sources on
    /// every tick, including the silent timer hops (`Error` -> `WarmUp`, `WarmUp` ->
    /// `Inactive`) that produce no event. Recomputing inside the event builders instead
    /// stranded `state` at its last announced value in `GET /alerts` and the UI badge.
    fn process_alert(
        alert: &mut Alert,
        outcomes: &SourceOutcomes,
        before: MachineState,
    ) -> Option<AlertEvent> {
        alert.state = alert.worst_of_visible();
        if let Some(event) = Self::build_transition_event(alert, outcomes, before) {
            return Some(event);
        }
        if let Some(event) = Self::build_suppression_event(alert, outcomes) {
            return Some(event);
        }
        Self::build_quiet_event(alert, outcomes)
    }

    /// Advances every source state machine one tick and collects the
    /// per-source transition messages.
    fn evaluate_sources(&self, alert: &mut Alert) -> SourceOutcomes {
        debug_assert_eq!(alert.channel_sources.len(), alert.source_states.len());
        let mut outcomes = SourceOutcomes::default();
        let (min, max) = (alert.min, alert.max);
        let warmup_duration = alert.warmup_duration;
        let cooldown_duration = alert.cooldown_duration;
        // Active-source messages are only consumed by the quiet-tick events;
        // skip collecting them in the steady state to avoid per-tick allocation.
        let collect_active = alert.notified.not()
            || alert.repeat_interval > 0.0
            || (alert.shutdown_on_activation && alert.shutdown_scheduled.not());
        // Likewise for unreadable sources: only the Error catch-up consumes them.
        let collect_unreadable = alert.error_notified.not();
        for (source, state) in alert
            .channel_sources
            .iter()
            .zip(alert.source_states.iter_mut())
        {
            if self.source_calibrating(source) {
                if *state != AlertState::Inactive {
                    trace!(
                        "Alert source {} silently reset: channel under calibration",
                        source.channel_name
                    );
                    *state = AlertState::Inactive;
                    outcomes.suppressed = true;
                }
                continue;
            }
            let value = match self.resolve_source_value(source) {
                Ok(value) => value,
                Err(reason) => {
                    if *state != AlertState::Error {
                        *state = AlertState::Error;
                        let channel_label = self.source_label(source);
                        outcomes.errors.push(format!("{channel_label}: {reason}"));
                    } else if collect_unreadable {
                        let channel_label = self.source_label(source);
                        outcomes
                            .unreadable
                            .push(format!("{channel_label}: {reason}"));
                    }
                    continue;
                }
            };
            let in_range = value >= min && value <= max;
            let old_state = *state;
            *state =
                Alert::transition_source(old_state, in_range, warmup_duration, cooldown_duration);
            self.collect_source_messages(
                &mut outcomes,
                source,
                old_state,
                *state,
                value,
                min,
                max,
                collect_active,
            );
        }
        outcomes
    }

    /// Files the message for one source's tick outcome into the right bucket.
    // Every argument is a distinct part of one tick's outcome. Grouping them into a
    // struct would only move the same fields behind another name for a single call site.
    #[allow(clippy::too_many_arguments)]
    fn collect_source_messages(
        &self,
        outcomes: &mut SourceOutcomes,
        source: &ChannelSource,
        old_state: AlertState,
        new_state: AlertState,
        value: f64,
        min: f64,
        max: f64,
        collect_active: bool,
    ) {
        let strictly_active = new_state == AlertState::Active;
        let transition = Self::transition_kind(old_state, new_state);
        if (strictly_active && collect_active).not() && transition.is_none() {
            return;
        }
        let channel_label = self.source_label(source);
        if strictly_active {
            let message = Self::format_out_of_range_message(&channel_label, value, min, max);
            if matches!(transition, Some(TransitionKind::Fired)) {
                outcomes.fired.push(message.clone());
            }
            if collect_active {
                outcomes.out_of_range.push(message);
            }
        } else if matches!(transition, Some(TransitionKind::Resolved)) {
            outcomes.resolved.push(Self::format_in_range_message(
                &channel_label,
                value,
                min,
                max,
            ));
        }
    }

    /// `true` while the source's channel is being swept by a calibration.
    /// Temp sources stay live; the calibration preflight already guards temps.
    fn source_calibrating(&self, source: &ChannelSource) -> bool {
        if source.channel_metric == ChannelMetric::Temp {
            return false;
        }
        self.diagnosis_registry
            .is_in_flight_parts(&source.device_uid, &source.channel_name)
    }

    /// The user-facing label for an alert source: override > detected > raw.
    fn source_label(&self, source: &ChannelSource) -> String {
        debug_assert!(source.channel_name.is_empty().not());
        let label =
            self.overrides
                .resolve_channel_label(&source.device_uid, &source.channel_name, None);
        debug_assert!(label.is_empty().not());
        label
    }

    /// Reads the source's current metric value from the device status.
    fn resolve_source_value(&self, source: &ChannelSource) -> Result<f64, &'static str> {
        let Some(device) = self.all_devices.get(&source.device_uid) else {
            return Err("Device not found");
        };
        let device_borrow = device.borrow();
        // Read the newest status in place: cloning the full Status per source
        // per tick would be hot-path allocation for a single scalar.
        let Some(most_recent_status) = device_borrow.status_history.back() else {
            return Err("Device has no current status");
        };
        if source.channel_metric == ChannelMetric::Temp {
            return most_recent_status
                .temps
                .iter()
                .find(|temp| temp.name == source.channel_name)
                .map(|temp| temp.temp)
                .ok_or("Device Channel not found");
        }
        let Some(channel_status) = most_recent_status
            .channels
            .iter()
            .find(|channel| channel.name == source.channel_name)
        else {
            return Err("Device Channel not found");
        };
        match source.channel_metric {
            ChannelMetric::Duty => channel_status
                .duty
                .ok_or("Device Channel Duty Metric not found"),
            ChannelMetric::Load => channel_status
                .duty
                .ok_or("Device Channel Load Metric not found"),
            ChannelMetric::RPM => channel_status
                .rpm
                .map(f64::from)
                .ok_or("Device Channel RPM Metric not found"),
            ChannelMetric::Freq => channel_status
                .freq
                .map(f64::from)
                .ok_or("Device Channel Freq Metric not found"),
            // Handled above; a mixed-metric alert is rejected at the API boundary.
            ChannelMetric::Temp => Err("Device Channel not found"),
        }
    }

    /// The alert-level edge this tick crossed. Transitions crossing none are logged
    /// but never announced; simultaneous edges rank worse news first.
    fn edge_kind(before: MachineState, after: MachineState) -> Option<AlertEventKind> {
        let kind = if after.error && before.error.not() {
            Some(AlertEventKind::SourceError)
        } else if after.active && before.active.not() {
            Some(AlertEventKind::Triggered)
        } else if before.error && after.error.not() {
            Some(AlertEventKind::ErrorResolved)
        } else if before.active && after.active.not() {
            Some(AlertEventKind::Resolved)
        } else {
            None
        };
        debug_assert!(kind.is_none() || before != after);
        debug_assert!(before == after || kind.is_some());
        kind
    }

    /// One edge's announcement bookkeeping, shared by a tick and an edit. Returns
    /// whether to notify the desktop; a quiet or silenced edge announces nothing.
    fn apply_edge(alert: &mut Alert, kind: AlertEventKind, silenced: bool, quiet: bool) -> bool {
        debug_assert!(matches!(
            kind,
            AlertEventKind::Triggered
                | AlertEventKind::Resolved
                | AlertEventKind::SourceError
                | AlertEventKind::ErrorResolved
        ));
        let announceable = quiet.not() && silenced.not();
        let notify_desktop = announceable
            && alert.desktop_notify
            && match kind {
                AlertEventKind::Triggered | AlertEventKind::SourceError => true,
                // A recovery only notifies for an episode the user was told about.
                AlertEventKind::Resolved => alert.desktop_notify_recovery && alert.notified,
                AlertEventKind::ErrorResolved => {
                    alert.desktop_notify_recovery && alert.error_notified
                }
                AlertEventKind::StillActive | AlertEventKind::Repeat => false,
            };
        if announceable {
            match kind {
                AlertEventKind::Triggered => alert.notified = true,
                AlertEventKind::SourceError => alert.error_notified = true,
                _ => {}
            }
        }
        if notify_desktop {
            alert.last_notified = Some(Local::now());
        }
        debug_assert!(notify_desktop.not() || announceable);
        debug_assert!(notify_desktop.not() || alert.desktop_notify);
        notify_desktop
    }

    /// An episode that is over no longer counts as announced, so the next one
    /// starts from a clean slate.
    fn clear_finished_episodes(alert: &mut Alert, after: MachineState) {
        if after.active.not() {
            alert.notified = false;
        }
        if after.error.not() {
            alert.error_notified = false;
        }
    }

    /// Describes a transition that crossed no edge, so the log row still says what
    /// happened even though nothing is announced.
    fn detail_kind(outcomes: &SourceOutcomes) -> AlertEventKind {
        debug_assert!(outcomes.has_transitions());
        let kind = if outcomes.fired.is_empty().not() {
            AlertEventKind::Triggered
        } else if outcomes.errors.is_empty().not() {
            AlertEventKind::SourceError
        } else {
            AlertEventKind::Resolved
        };
        debug_assert!(kind != AlertEventKind::Resolved || outcomes.resolved.is_empty().not());
        kind
    }

    /// Folds this tick's per-source transitions into one event, updating the
    /// aggregate state and all notification bookkeeping.
    fn build_transition_event(
        alert: &mut Alert,
        outcomes: &SourceOutcomes,
        before: MachineState,
    ) -> Option<AlertEvent> {
        if outcomes.has_transitions().not() {
            return None;
        }
        let after = alert.machine_state();
        let edge = Self::edge_kind(before, after);
        let kind = edge.unwrap_or_else(|| Self::detail_kind(outcomes));
        let quiet = edge.is_none();
        let silenced = alert.is_silenced();
        let mut fire_shutdown = false;
        let mut cancel_shutdown = false;
        if silenced.not() {
            if alert.shutdown_on_activation && alert.shutdown_scheduled.not() && after.active {
                fire_shutdown = true;
                alert.shutdown_scheduled = true;
            }
            // Deliberate: unreadable sensors do not cancel a countdown. Losing a
            // sensor under thermal stress is not evidence the machine cooled down.
            if alert.shutdown_scheduled && after.active.not() && after.error.not() {
                cancel_shutdown = true;
                alert.shutdown_scheduled = false;
            }
        }
        let notify_desktop = Self::apply_edge(alert, kind, silenced, quiet);
        if fire_shutdown {
            // The shutdown warning is delivered for any event kind, so a fired
            // shutdown always counts as announcing the episode.
            alert.notified = true;
            if alert.desktop_notify {
                alert.last_notified = Some(Local::now());
            }
        }
        Self::clear_finished_episodes(alert, after);
        Some(AlertEvent {
            alert: alert.clone(),
            message: Self::join_messages(outcomes),
            kind,
            silenced,
            notify_desktop,
            fire_shutdown,
            cancel_shutdown,
            log: true,
            quiet,
        })
    }

    /// Handles a tick whose only change was a calibration-suppression reset:
    /// updates the aggregate bookkeeping and cancels a pending shutdown when no
    /// other source holds the alert active. Never logs or notifies; the reset
    /// itself stays silent by design.
    fn build_suppression_event(alert: &mut Alert, outcomes: &SourceOutcomes) -> Option<AlertEvent> {
        if outcomes.suppressed.not() {
            return None;
        }
        if alert.state != AlertState::Inactive {
            return None;
        }
        alert.notified = false;
        alert.error_notified = false;
        if alert.shutdown_scheduled.not() {
            return None;
        }
        alert.shutdown_scheduled = false;
        Some(AlertEvent {
            alert: alert.clone(),
            message: AlertLogMessage::new(),
            kind: AlertEventKind::Resolved,
            silenced: false,
            notify_desktop: false,
            fire_shutdown: false,
            cancel_shutdown: true,
            log: false,
            quiet: false,
        })
    }

    /// Re-announces an Error a lapsed silence would have buried. Deliberately has no
    /// repeat: an Error can be permanent and would notify forever, unlike an Active.
    fn build_error_catch_up(alert: &mut Alert, outcomes: &SourceOutcomes) -> Option<AlertEvent> {
        if alert.any_source_error().not() || alert.error_notified {
            return None;
        }
        // The collect_unreadable gate in evaluate_sources covers exactly this path.
        debug_assert!(outcomes.unreadable.is_empty().not());
        alert.error_notified = true;
        let notify_desktop = alert.desktop_notify;
        if notify_desktop {
            alert.last_notified = Some(Local::now());
        }
        Some(AlertEvent {
            alert: alert.clone(),
            message: outcomes.unreadable.join("; "),
            kind: AlertEventKind::SourceError,
            silenced: false,
            notify_desktop,
            fire_shutdown: false,
            cancel_shutdown: false,
            log: true,
            quiet: false,
        })
    }

    /// Handles ticks with no state transition: the notify-on-silence-expiry catch-up
    /// (including the shutdown re-arm) plus periodic repeat notifications.
    fn build_quiet_event(alert: &mut Alert, outcomes: &SourceOutcomes) -> Option<AlertEvent> {
        if alert.is_silenced() {
            return None;
        }
        if let Some(event) = Self::build_error_catch_up(alert, outcomes) {
            return Some(event);
        }
        let strictly_active = alert.source_states.contains(&AlertState::Active);
        if strictly_active.not() {
            return None;
        }
        let needs_announce = alert.notified.not();
        let needs_shutdown = alert.shutdown_on_activation && alert.shutdown_scheduled.not();
        if needs_announce || needs_shutdown {
            // The collect_active gate in evaluate_sources covers exactly these paths.
            debug_assert!(outcomes.out_of_range.is_empty().not());
            alert.notified = true;
            if needs_shutdown {
                alert.shutdown_scheduled = true;
            }
            let notify_desktop = alert.desktop_notify;
            if notify_desktop {
                alert.last_notified = Some(Local::now());
            }
            return Some(AlertEvent {
                alert: alert.clone(),
                message: outcomes.out_of_range.join("; "),
                kind: AlertEventKind::StillActive,
                silenced: false,
                notify_desktop,
                fire_shutdown: needs_shutdown,
                cancel_shutdown: false,
                log: true,
                quiet: false,
            });
        }
        if alert.repeat_interval > 0.0 && alert.desktop_notify {
            let due = alert.last_notified.is_none_or(|last| {
                Local::now().signed_duration_since(last).as_seconds_f64() >= alert.repeat_interval
            });
            if due {
                debug_assert!(outcomes.out_of_range.is_empty().not());
                alert.last_notified = Some(Local::now());
                return Some(AlertEvent {
                    alert: alert.clone(),
                    message: outcomes.out_of_range.join("; "),
                    kind: AlertEventKind::Repeat,
                    silenced: false,
                    notify_desktop: true,
                    fire_shutdown: false,
                    cancel_shutdown: false,
                    log: false,
                    quiet: false,
                });
            }
        }
        None
    }

    fn join_messages(outcomes: &SourceOutcomes) -> AlertLogMessage {
        let mut parts = Vec::with_capacity(
            outcomes.fired.len() + outcomes.errors.len() + outcomes.resolved.len(),
        );
        parts.extend(outcomes.fired.iter().cloned());
        parts.extend(outcomes.errors.iter().cloned());
        parts.extend(outcomes.resolved.iter().cloned());
        parts.join("; ")
    }

    fn transition_kind(old_state: AlertState, new_state: AlertState) -> Option<TransitionKind> {
        if old_state == new_state {
            return None;
        }
        match (old_state, new_state) {
            (_, AlertState::WarmUp(_))
            | (AlertState::WarmUp(_), AlertState::Inactive)
            | (AlertState::Active, AlertState::Cooldown(_))
            | (AlertState::Cooldown(_), AlertState::Active) => None,
            (_, AlertState::Active) => Some(TransitionKind::Fired),
            (_, AlertState::Inactive) => Some(TransitionKind::Resolved),
            _ => None,
        }
    }

    /// Rounds away from the range so the display clearly shows the violation.
    fn format_out_of_range_message(channel_label: &str, value: f64, min: f64, max: f64) -> String {
        if value > max {
            let value_rounded = (value * 10.).ceil() / 10.;
            format!("{channel_label}: {value_rounded} is greater than allowed maximum: {max}")
        } else {
            let value_rounded = (value * 10.).floor() / 10.;
            format!("{channel_label}: {value_rounded} is less than allowed minimum: {min}")
        }
    }

    fn format_in_range_message(channel_label: &str, value: f64, min: f64, max: f64) -> String {
        let value_rounded = (value * 10.).round() / 10.;
        format!("{channel_label}: {value_rounded} is again within allowed range: {min} - {max}")
    }

    /// The single path out of the alert engine, so the tick and edit paths cannot
    /// drift in wording or wire shape.
    fn dispatch_event(&self, event: &AlertEvent) {
        debug_assert!(event.notify_desktop.not() || event.silenced.not());
        debug_assert!(event.quiet.not() || event.notify_desktop.not());
        self.send_notifications(event);
        if event.log.not() {
            return;
        }
        let log = self.log_alert_state_change(event);
        if let Some(handle) = self.alert_handle.borrow().as_ref() {
            handle.broadcast_alert_state_change(log);
        }
    }

    /// Logs an alert state change to the internal buffer, as well as returning the newly
    /// created log entry.
    fn log_alert_state_change(&self, event: &AlertEvent) -> AlertLog {
        let log = AlertLog {
            uid: event.alert.uid.clone(),
            name: event.alert.name.clone(),
            state: event.alert.state,
            message: event.message.clone(),
            timestamp: Local::now(),
            silenced: event.silenced,
            // DOWNGRADE-COMPAT(added 5.0.0, remove 5.2.0): superseded by `kind`.
            resolved: event.kind == AlertEventKind::Resolved,
            kind: event.kind.into(),
            quiet: event.quiet,
        };
        let mut logs_lock = self.logs.borrow_mut();
        while logs_lock.len() >= LOG_BUFFER_SIZE {
            logs_lock.pop_front();
        }
        logs_lock.push_back(log.clone());
        self.logs_dirty.set(true);
        log
    }

    /// Executes an event's decided side effects: shutdown commands, then the
    /// desktop notification. All decisions were made when the event was built.
    /// A fired shutdown always delivers the shutdown warning, whatever the
    /// event kind that carried it.
    fn send_notifications(&self, event: &AlertEvent) {
        let alert = &event.alert;
        if event.kind == AlertEventKind::SourceError {
            warn!("Alert in Error State: {} = {}", alert.name, event.message);
        }
        if event.cancel_shutdown {
            Self::fire_command(COMMAND_SHUTDOWN_CANCEL);
            info!(
                "Shutdown Alert Resolved: {} - Shutdown cancelled",
                alert.name
            );
        }
        if event.fire_shutdown {
            self.warn_of_shutdown(event);
            return;
        }
        if event.notify_desktop.not() {
            return;
        }
        self.notify_of_event(event);
    }

    /// Commences the shutdown and warns every session, whatever the event kind
    /// that carried it.
    fn warn_of_shutdown(&self, event: &AlertEvent) {
        debug_assert!(event.fire_shutdown);
        let alert = &event.alert;
        Self::fire_command(COMMAND_SHUTDOWN);
        info!(
            "Shutdown Alert Triggered: {} - Shutdown will commence in 1 Minute",
            alert.name
        );
        if alert.desktop_notify.not() {
            return;
        }
        let title = format!("Shutdown Alert Triggered: {}!", alert.name);
        let body = format!("Shutdown will commence in 1 Minute.\n{}", event.message);
        notifier::notify_all_sessions(
            &title,
            &body,
            NotificationIcon::Shutdown,
            alert.desktop_notify_audio,
            Some(2),
            self.notification_handle.borrow().as_ref(),
        );
    }

    /// The desktop notification for an event the evaluation pass decided to announce.
    fn notify_of_event(&self, event: &AlertEvent) {
        debug_assert!(event.notify_desktop);
        debug_assert!(event.fire_shutdown.not());
        let alert = &event.alert;
        let handle_ref = self.notification_handle.borrow();
        let handle = handle_ref.as_ref();
        match event.kind {
            AlertEventKind::Triggered | AlertEventKind::StillActive | AlertEventKind::Repeat => {
                let title = if event.kind == AlertEventKind::Triggered {
                    format!("Alert Triggered: {}!", alert.name)
                } else {
                    format!("Alert Still Active: {}!", alert.name)
                };
                notifier::notify_all_sessions(
                    &title,
                    &event.message,
                    NotificationIcon::Triggered,
                    alert.desktop_notify_audio,
                    None,
                    handle,
                );
            }
            AlertEventKind::Resolved => {
                let title = format!("Alert Resolved: {}", alert.name);
                notifier::notify_all_sessions(
                    &title,
                    &event.message,
                    NotificationIcon::Resolved,
                    false,
                    None,
                    handle,
                );
            }
            AlertEventKind::SourceError => {
                let title = format!("Alert Error: {}", alert.name);
                notifier::notify_all_sessions(
                    &title,
                    &Self::with_resulting_state(alert, &event.message),
                    NotificationIcon::Error,
                    alert.desktop_notify_audio,
                    None,
                    handle,
                );
            }
            AlertEventKind::ErrorResolved => {
                let title = format!("Alert Sensors Readable: {}", alert.name);
                notifier::notify_all_sessions(
                    &title,
                    &Self::with_resulting_state(alert, &event.message),
                    NotificationIcon::Resolved,
                    false,
                    None,
                    handle,
                );
            }
        }
    }

    /// Appends the resulting state to an Error body: the only way a consumed Active
    /// edge reaches the user, and when a pending shutdown is worth mentioning.
    fn with_resulting_state(alert: &Alert, message: &str) -> String {
        let mut body = if message.is_empty() {
            String::new()
        } else {
            format!("{message}\n")
        };
        match alert.state {
            AlertState::Inactive => body.push_str("Alert is Inactive."),
            AlertState::Error => body.push_str("Alert is in an Error state."),
            _ => body.push_str("Alert is still Active."),
        }
        if alert.shutdown_scheduled {
            body.push_str(" Shutdown will commence in 1 Minute.");
        }
        debug_assert!(body.is_empty().not());
        debug_assert!(body.ends_with('.'));
        body
    }

    #[cfg(not(test))]
    fn fire_command(cmd: &str) {
        let cmd = cmd.to_string();
        rt::spawn(async move {
            if let ShellCommandResult::Error(err) =
                ShellCommand::new(&cmd, Duration::from_secs(20)).run().await
            {
                if log::log_enabled!(log::Level::Debug) {
                    warn!("Failed to execute notification command: '{cmd}' - {err}");
                }
            }
        });
    }

    /// Recorded instead of executed under test, so the shutdown paths are assertable
    /// without spawning onto a runtime or halting the machine running the suite.
    #[cfg(test)]
    fn fire_command(cmd: &str) {
        FIRED_COMMANDS.with_borrow_mut(|fired| fired.push(cmd.to_string()));
    }
}

impl CalibrationAlertGate for AlertController {
    /// A visibly Active (incl. Cooldown), enabled, unsilenced, non-Temp source
    /// on the channel blocks calibration. Silenced or disabled alerts do not:
    /// the user has explicitly quieted them.
    fn active_alert_for_channel(&self, device_uid: &str, channel_name: &str) -> Option<String> {
        for alert in self.alerts.borrow().values() {
            if alert.enabled.not() {
                continue;
            }
            if alert.is_silenced() {
                continue;
            }
            for (source, state) in alert.channel_sources.iter().zip(alert.source_states.iter()) {
                if source.channel_metric == ChannelMetric::Temp {
                    continue;
                }
                if source.device_uid != device_uid {
                    continue;
                }
                if source.channel_name != channel_name {
                    continue;
                }
                if state.visible() == AlertState::Active {
                    return Some(alert.name.clone());
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AlertConfigFile {
    alerts: Vec<Alert>,
    /// Legacy field present in config files written before the data-dir split.
    /// Populated on deserialization for one-time migration to `alert-logs.json`;
    /// never serialized so new saves omit it.
    #[serde(default, skip_serializing)]
    logs: Vec<AlertLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AlertLogsFile {
    logs: Vec<AlertLog>,
}

/// Validates that `contents` parse as an alerts config file.
pub fn validate(contents: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    serde_json::from_str::<AlertConfigFile>(contents)
        .context("Parsing alerts configuration")
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    const LEGACY_UID: &str = "8ef08515338cc1f7727415a1d6bc45e84d5a818e94bd2072ac34b9d8d87f4210";
    const CURRENT_UID: &str = "67ada747f6820bb2e4a9b5b639657e0971998a6f5ca5f50480ecf36b46c7ec29";

    async fn write_alert_file(tmp: &tempfile::TempDir, contents: &str) -> std::path::PathBuf {
        let path = tmp.path().join("alerts.json");
        cc_fs::write(&path, contents.as_bytes().to_vec())
            .await
            .unwrap();
        path
    }

    // Goal: an alert watching a drive temp follows the drive onto its wwid-derived UID.
    // Method: write a realistic alerts file naming the legacy UID in both the
    // downgrade-compat `channel_source` and the `channel_sources` array, then migrate.
    #[test]
    #[serial_test::serial]
    fn migrate_device_uid_in_file_repoints_every_reference() {
        crate::rt::test_runtime(async {
            let contents = format!(
                r#"[{{"uid":"a1","name":"Drive hot","channel_source":{{"device_uid":"{LEGACY_UID}","channel_name":"temp1"}},"channel_sources":[{{"device_uid":"{LEGACY_UID}","channel_name":"temp1"}}]}}]"#
            );
            let tmp = tempfile::tempdir().unwrap();
            let path = write_alert_file(&tmp, &contents).await;

            let migrated =
                migrate_device_uid_in(&path, &LEGACY_UID.to_string(), &CURRENT_UID.to_string())
                    .await
                    .unwrap();

            let after = cc_fs::read_txt(&path).await.unwrap();
            assert!(migrated);
            assert!(after.contains(LEGACY_UID).not());
            assert_eq!(after.matches(CURRENT_UID).count(), 2);
            assert!(after.contains(r#""name":"Drive hot""#));
        });
    }

    // Goal: negative space. A file that already names the new UID is left untouched, so a
    // half-applied migration is never merged into an ambiguous state.
    #[test]
    #[serial_test::serial]
    fn migrate_device_uid_in_file_skips_when_current_present() {
        crate::rt::test_runtime(async {
            let contents = format!(
                r#"[{{"uid":"a1","channel_source":{{"device_uid":"{LEGACY_UID}"}}}},{{"uid":"a2","channel_source":{{"device_uid":"{CURRENT_UID}"}}}}]"#
            );
            let tmp = tempfile::tempdir().unwrap();
            let path = write_alert_file(&tmp, &contents).await;

            let migrated =
                migrate_device_uid_in(&path, &LEGACY_UID.to_string(), &CURRENT_UID.to_string())
                    .await
                    .unwrap();

            let after = cc_fs::read_txt(&path).await.unwrap();
            assert!(migrated.not());
            assert_eq!(after, contents);
        });
    }

    // Goal: the overwhelmingly common case. No alerts reference this device, so the file
    // must be left byte-identical rather than rewritten on every boot.
    #[test]
    #[serial_test::serial]
    fn migrate_device_uid_in_file_leaves_unrelated_alerts_alone() {
        crate::rt::test_runtime(async {
            let contents = r#"[{"uid":"a1","channel_source":{"device_uid":"other"}}]"#;
            let tmp = tempfile::tempdir().unwrap();
            let path = write_alert_file(&tmp, contents).await;

            let migrated =
                migrate_device_uid_in(&path, &LEGACY_UID.to_string(), &CURRENT_UID.to_string())
                    .await
                    .unwrap();

            let after = cc_fs::read_txt(&path).await.unwrap();
            assert!(migrated.not());
            assert_eq!(after, contents);
        });
    }

    /// Helper to create a minimal test alert with given uid, min, max, and state.
    fn make_alert(uid: &str, min: f64, max: f64, state: AlertState) -> Alert {
        assert!(min <= max, "min must be <= max for a valid alert range.");
        let channel_source = ChannelSource {
            device_uid: "dev1".to_string(),
            channel_name: "temp1".to_string(),
            channel_metric: ChannelMetric::Temp,
        };
        Alert {
            uid: uid.to_string(),
            name: format!("Alert-{uid}"),
            channel_sources: vec![channel_source.clone()],
            channel_source,
            min,
            max,
            state,
            warmup_duration: 0.0,
            cooldown_duration: 0.0,
            repeat_interval: 0.0,
            enabled: true,
            silenced_until: None,
            desktop_notify: true,
            desktop_notify_recovery: true,
            desktop_notify_audio: false,
            shutdown_on_activation: false,
            source_states: vec![state],
            last_notified: None,
            notified: false,
            error_notified: false,
            shutdown_scheduled: false,
        }
    }

    /// Helper for a second, distinct channel source.
    fn make_source(channel_name: &str, channel_metric: ChannelMetric) -> ChannelSource {
        ChannelSource {
            device_uid: "dev1".to_string(),
            channel_name: channel_name.to_string(),
            channel_metric,
        }
    }

    // -- IndexMap order-preservation tests --

    #[test]
    fn indexmap_preserves_insertion_order() {
        // Goal: verify that IndexMap iterates values in insertion order,
        // which is the invariant we rely on after replacing LinkedHashMap.
        let mut map: IndexMap<String, Alert> = IndexMap::new();
        let a1 = make_alert("uid-1", 10.0, 90.0, AlertState::Inactive);
        let a2 = make_alert("uid-2", 20.0, 80.0, AlertState::Inactive);
        let a3 = make_alert("uid-3", 30.0, 70.0, AlertState::Inactive);
        map.insert(a1.uid.clone(), a1);
        map.insert(a2.uid.clone(), a2);
        map.insert(a3.uid.clone(), a3);

        let uids: Vec<&str> = map.values().map(|a| a.uid.as_str()).collect();
        assert_eq!(uids, vec!["uid-1", "uid-2", "uid-3"]);
    }

    #[test]
    fn indexmap_shift_remove_preserves_remaining_order() {
        // Goal: verify that shift_remove keeps the relative order of
        // remaining entries intact (unlike swap_remove).
        let mut map: IndexMap<String, Alert> = IndexMap::new();
        let a1 = make_alert("uid-1", 10.0, 90.0, AlertState::Inactive);
        let a2 = make_alert("uid-2", 20.0, 80.0, AlertState::Inactive);
        let a3 = make_alert("uid-3", 30.0, 70.0, AlertState::Inactive);
        map.insert(a1.uid.clone(), a1);
        map.insert(a2.uid.clone(), a2);
        map.insert(a3.uid.clone(), a3);
        assert_eq!(map.len(), 3);

        map.shift_remove("uid-2");
        assert_eq!(map.len(), 2);

        let uids: Vec<&str> = map.values().map(|a| a.uid.as_str()).collect();
        assert_eq!(uids, vec!["uid-1", "uid-3"]);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn indexmap_insert_existing_key_replaces_in_place() {
        // Goal: verify that insert() on an existing key updates the value
        // without changing its position in iteration order.
        let mut map: IndexMap<String, Alert> = IndexMap::new();
        let a1 = make_alert("uid-1", 10.0, 90.0, AlertState::Inactive);
        let a2 = make_alert("uid-2", 20.0, 80.0, AlertState::Inactive);
        let a3 = make_alert("uid-3", 30.0, 70.0, AlertState::Inactive);
        map.insert(a1.uid.clone(), a1);
        map.insert(a2.uid.clone(), a2);
        map.insert(a3.uid.clone(), a3);

        // Update uid-2 with a different min/max.
        let updated = make_alert("uid-2", 25.0, 75.0, AlertState::Active);
        map.insert(updated.uid.clone(), updated);

        // Order must be preserved.
        let uids: Vec<&str> = map.values().map(|a| a.uid.as_str()).collect();
        assert_eq!(uids, vec!["uid-1", "uid-2", "uid-3"]);
        // Value must be updated.
        assert_eq!(map["uid-2"].min, 25.0);
        assert_eq!(map["uid-2"].max, 75.0);
        assert_eq!(map["uid-2"].state, AlertState::Active);
    }

    // -- Alert::transition_source tests (core state machine) --

    #[test]
    fn transition_in_range_stays_inactive() {
        // Goal: verify an in-range tick keeps an Inactive source Inactive.
        let new_state = Alert::transition_source(AlertState::Inactive, true, 0.0, 0.0);
        assert_eq!(new_state, AlertState::Inactive);
    }

    #[test]
    fn transition_out_of_range_inactive_to_warmup() {
        // Goal: verify an out-of-range tick moves Inactive into WarmUp.
        let new_state = Alert::transition_source(AlertState::Inactive, false, 5.0, 0.0);
        assert!(matches!(new_state, AlertState::WarmUp(_)));
    }

    #[test]
    fn transition_warmup_to_active_after_duration() {
        // Goal: verify WarmUp becomes Active once the warmup duration elapsed.
        let past = Local::now() - Duration::seconds(2);
        let new_state = Alert::transition_source(AlertState::WarmUp(past), false, 1.0, 0.0);
        assert_eq!(new_state, AlertState::Active);
    }

    #[test]
    fn transition_warmup_stays_warmup_before_duration() {
        // Goal: verify WarmUp does NOT become Active before the duration elapses.
        let now = Local::now();
        let new_state = Alert::transition_source(AlertState::WarmUp(now), false, 9999.0, 0.0);
        assert!(matches!(new_state, AlertState::WarmUp(_)));
    }

    #[test]
    fn transition_warmup_returns_to_inactive_when_in_range() {
        // Goal: verify WarmUp silently returns to Inactive when back in range.
        let past = Local::now() - Duration::seconds(1);
        let new_state = Alert::transition_source(AlertState::WarmUp(past), true, 1.0, 0.0);
        assert_eq!(new_state, AlertState::Inactive);
    }

    #[test]
    fn transition_active_stays_active_out_of_range() {
        // Goal: verify Active stays Active while the value remains out of range.
        let new_state = Alert::transition_source(AlertState::Active, false, 0.0, 0.0);
        assert_eq!(new_state, AlertState::Active);
    }

    #[test]
    fn transition_active_clears_immediately_without_cooldown() {
        // Goal: verify cooldown_duration=0 preserves the previous
        // instant-recovery behavior.
        let new_state = Alert::transition_source(AlertState::Active, true, 0.0, 0.0);
        assert_eq!(new_state, AlertState::Inactive);
    }

    #[test]
    fn transition_active_enters_cooldown_when_configured() {
        // Goal: verify an Active source with a cooldown holds in Cooldown
        // instead of clearing on the first in-range tick.
        let new_state = Alert::transition_source(AlertState::Active, true, 0.0, 5.0);
        assert!(matches!(new_state, AlertState::Cooldown(_)));
    }

    #[test]
    fn transition_cooldown_clears_after_duration() {
        // Goal: verify Cooldown becomes Inactive once the value stayed
        // in range for the full cooldown duration.
        let past = Local::now() - Duration::seconds(2);
        let new_state = Alert::transition_source(AlertState::Cooldown(past), true, 0.0, 1.0);
        assert_eq!(new_state, AlertState::Inactive);
    }

    #[test]
    fn transition_cooldown_holds_before_duration() {
        // Goal: verify Cooldown does NOT clear before the duration elapses.
        let now = Local::now();
        let new_state = Alert::transition_source(AlertState::Cooldown(now), true, 0.0, 9999.0);
        assert!(matches!(new_state, AlertState::Cooldown(_)));
    }

    #[test]
    fn transition_cooldown_returns_to_active_when_out_of_range() {
        // Goal: verify a value flapping back out of range during Cooldown
        // returns the source to Active immediately (it never stopped firing).
        let now = Local::now();
        let new_state = Alert::transition_source(AlertState::Cooldown(now), false, 9999.0, 5.0);
        assert_eq!(new_state, AlertState::Active);
    }

    #[test]
    fn transition_error_to_warmup_when_out_of_range() {
        // Goal: verify Error -> WarmUp when a value arrives but is out of range.
        // Error means the channel was missing; receiving a value means the error
        // resolved but the alert still needs to warm up.
        let new_state = Alert::transition_source(AlertState::Error, false, 5.0, 0.0);
        assert!(matches!(new_state, AlertState::WarmUp(_)));
    }

    #[test]
    fn transition_error_to_inactive_when_in_range() {
        // Goal: verify Error -> Inactive when the value is in range.
        let new_state = Alert::transition_source(AlertState::Error, true, 5.0, 0.0);
        assert_eq!(new_state, AlertState::Inactive);
    }

    #[test]
    fn transition_zero_warmup_two_tick_activation() {
        // Goal: verify warmup_duration=0 still takes two consecutive
        // out-of-range ticks: Inactive -> WarmUp -> Active.
        let first = Alert::transition_source(AlertState::Inactive, false, 0.0, 0.0);
        assert!(matches!(first, AlertState::WarmUp(_)));
        let second = Alert::transition_source(first, false, 0.0, 0.0);
        assert_eq!(second, AlertState::Active);
    }

    // -- transition_kind (message visibility) tests --

    #[test]
    fn transition_kind_timer_hops_are_silent() {
        // Goal: verify no message is produced for internal timer-state hops:
        // into WarmUp, WarmUp back down, and Active<->Cooldown.
        let now = Local::now();
        let silent_pairs = [
            (AlertState::Inactive, AlertState::WarmUp(now)),
            (AlertState::Error, AlertState::WarmUp(now)),
            (AlertState::WarmUp(now), AlertState::Inactive),
            (AlertState::Active, AlertState::Cooldown(now)),
            (AlertState::Cooldown(now), AlertState::Active),
            (AlertState::Active, AlertState::Active),
        ];
        for (old_state, new_state) in silent_pairs {
            assert!(
                AlertController::transition_kind(old_state, new_state).is_none(),
                "{old_state:?} -> {new_state:?} must be silent"
            );
        }
    }

    #[test]
    fn transition_kind_fired_and_resolved() {
        // Goal: verify the message-producing transitions map to the right kind.
        let now = Local::now();
        assert!(matches!(
            AlertController::transition_kind(AlertState::WarmUp(now), AlertState::Active),
            Some(TransitionKind::Fired)
        ));
        assert!(matches!(
            AlertController::transition_kind(AlertState::Active, AlertState::Inactive),
            Some(TransitionKind::Resolved)
        ));
        assert!(matches!(
            AlertController::transition_kind(AlertState::Cooldown(now), AlertState::Inactive),
            Some(TransitionKind::Resolved)
        ));
        assert!(matches!(
            AlertController::transition_kind(AlertState::Error, AlertState::Inactive),
            Some(TransitionKind::Resolved)
        ));
    }

    // -- aggregation and silence helpers --

    #[test]
    fn worst_of_visible_priorities() {
        // Goal: verify the aggregate state is the worst of the visible
        // per-source states: Error > Active > Inactive, with timer states
        // collapsing to their visible equivalents.
        let now = Local::now();
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Inactive);
        alert.source_states = vec![AlertState::WarmUp(now), AlertState::Inactive];
        assert_eq!(alert.worst_of_visible(), AlertState::Inactive);
        alert.source_states = vec![AlertState::Cooldown(now), AlertState::Inactive];
        assert_eq!(alert.worst_of_visible(), AlertState::Active);
        alert.source_states = vec![AlertState::Active, AlertState::Error];
        assert_eq!(alert.worst_of_visible(), AlertState::Error);
    }

    #[test]
    fn is_silenced_only_while_timestamp_in_future() {
        // Goal: verify the silence window: future timestamp silences,
        // past timestamp and None do not.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Inactive);
        assert!(!alert.is_silenced());
        alert.silenced_until = Some(Local::now() + Duration::seconds(60));
        assert!(alert.is_silenced());
        alert.silenced_until = Some(Local::now() - Duration::seconds(60));
        assert!(!alert.is_silenced());
    }

    #[test]
    fn normalize_sources_seeds_from_legacy_field() {
        // Goal: verify a pre-4.4.0 alert (empty channel_sources) is seeded
        // from the legacy channel_source, with parallel runtime states.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Inactive);
        alert.channel_sources.clear();
        alert.source_states.clear();
        alert.normalize_sources();
        assert_eq!(alert.channel_sources.len(), 1);
        assert_eq!(alert.channel_sources[0], alert.channel_source);
        assert_eq!(alert.source_states, vec![AlertState::Inactive]);
    }

    #[test]
    fn normalize_sources_mirrors_first_into_legacy_field() {
        // Goal: verify the legacy channel_source is kept written as
        // channel_sources[0] (the DOWNGRADE-COMPAT invariant).
        let mut alert = make_alert("a", 0.0, 1000.0, AlertState::Inactive);
        alert.channel_sources = vec![
            make_source("fan1", ChannelMetric::RPM),
            make_source("fan2", ChannelMetric::RPM),
        ];
        alert.normalize_sources();
        assert_eq!(alert.channel_source, alert.channel_sources[0]);
        assert_eq!(alert.source_states.len(), 2);
    }

    #[test]
    fn normalize_sources_trims_beyond_the_source_maximum() {
        // Goal: the cap has to hold on the load path, not only at the API. A hand-edited
        // alerts.json never passes the API's check, and every tick walks these sources.
        // Method: hand an alert twice the maximum and assert it is trimmed rather than
        // rejected, since one malformed alert must not stop the rest of the file loading.
        let mut alert = make_alert("a", 0.0, 1000.0, AlertState::Inactive);
        alert.channel_sources = (0..MAX_ALERT_SOURCES * 2)
            .map(|i| make_source(&format!("fan{i}"), ChannelMetric::RPM))
            .collect();

        alert.normalize_sources();

        assert_eq!(alert.channel_sources.len(), MAX_ALERT_SOURCES);
        assert_eq!(alert.source_states.len(), MAX_ALERT_SOURCES);
        assert_eq!(
            alert.channel_source, alert.channel_sources[0],
            "the legacy mirror still tracks the first surviving source"
        );
    }

    /// The machine snapshot a tick would have taken before these transitions.
    fn machines(active: bool, error: bool) -> MachineState {
        MachineState { active, error }
    }

    // -- build_transition_event tests --

    #[test]
    fn build_transition_event_none_without_transitions() {
        // Goal: verify a tick with no per-source transitions produces no event.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Inactive);
        let outcomes = SourceOutcomes::default();
        assert!(AlertController::build_transition_event(
            &mut alert,
            &outcomes,
            machines(false, false)
        )
        .is_none());
    }

    #[test]
    fn build_transition_event_coalesces_multiple_sources() {
        // Goal: verify two sources firing in the same tick produce ONE event
        // whose message names both sensors.
        let mut alert = make_alert("a", 0.0, 1000.0, AlertState::Inactive);
        alert.channel_sources = vec![
            make_source("fan1", ChannelMetric::RPM),
            make_source("fan2", ChannelMetric::RPM),
        ];
        alert.source_states = vec![AlertState::Active, AlertState::Active];
        let outcomes = SourceOutcomes {
            fired: vec![
                "fan1: 0 too low".to_string(),
                "fan2: 10 too low".to_string(),
            ],
            ..Default::default()
        };
        let event =
            AlertController::process_alert(&mut alert, &outcomes, machines(false, false)).unwrap();
        assert!(matches!(event.kind, AlertEventKind::Triggered));
        assert_eq!(event.message, "fan1: 0 too low; fan2: 10 too low");
        assert_eq!(event.alert.state, AlertState::Active);
        assert!(event.notify_desktop);
        assert!(event.log);
        assert!(alert.notified);
    }

    #[test]
    fn build_transition_event_second_source_firing_stays_quiet() {
        // Goal: a second sensor firing crosses no alert-level edge, so it is
        // logged but never announced.
        let mut alert = make_alert("a", 0.0, 1000.0, AlertState::Active);
        alert.channel_sources = vec![
            make_source("fan1", ChannelMetric::RPM),
            make_source("fan2", ChannelMetric::RPM),
        ];
        alert.source_states = vec![AlertState::Active, AlertState::Active];
        alert.notified = true;
        let outcomes = SourceOutcomes {
            fired: vec!["fan2: 10 too low".to_string()],
            ..Default::default()
        };
        let event =
            AlertController::build_transition_event(&mut alert, &outcomes, machines(true, false))
                .unwrap();
        assert!(event.quiet);
        assert!(event.notify_desktop.not());
        assert!(event.log, "the second sensor still reaches the log");
        assert!(
            matches!(event.kind, AlertEventKind::Triggered),
            "the row still says what happened, it just does not interrupt"
        );
    }

    #[test]
    fn build_transition_event_partial_recovery_keeps_active() {
        // Goal: verify one source recovering while another stays Active
        // produces a Resolved event but the aggregate remains Active.
        let mut alert = make_alert("a", 0.0, 1000.0, AlertState::Active);
        alert.channel_sources = vec![
            make_source("fan1", ChannelMetric::RPM),
            make_source("fan2", ChannelMetric::RPM),
        ];
        alert.source_states = vec![AlertState::Active, AlertState::Inactive];
        alert.notified = true;
        let outcomes = SourceOutcomes {
            resolved: vec!["fan2: back in range".to_string()],
            ..Default::default()
        };
        let event =
            AlertController::build_transition_event(&mut alert, &outcomes, machines(true, false))
                .unwrap();
        assert!(matches!(event.kind, AlertEventKind::Resolved));
        assert_eq!(event.alert.state, AlertState::Active);
        assert!(alert.notified, "episode continues until all sources clear");
        assert!(event.quiet);
        assert!(
            event.notify_desktop.not(),
            "a partial recovery must not announce resolved while still firing"
        );
        assert!(event.log, "the per-source recovery still reaches the log");
    }

    #[test]
    fn build_transition_event_shutdown_fires_with_warning_on_resolved_kind() {
        // Goal: verify the silence-expiry edge: the silence lapses on the tick
        // one of two Active sources recovers. The shutdown fires even though
        // the event kind is Resolved, and the episode counts as announced
        // because send_notifications delivers the shutdown warning for any
        // event kind.
        let mut alert = make_alert("a", 0.0, 1000.0, AlertState::Active);
        alert.shutdown_on_activation = true;
        alert.channel_sources = vec![
            make_source("fan1", ChannelMetric::RPM),
            make_source("fan2", ChannelMetric::RPM),
        ];
        alert.source_states = vec![AlertState::Active, AlertState::Inactive];
        alert.notified = false;
        let outcomes = SourceOutcomes {
            resolved: vec!["fan2: back in range".to_string()],
            ..Default::default()
        };
        let event =
            AlertController::build_transition_event(&mut alert, &outcomes, machines(true, false))
                .unwrap();
        assert!(matches!(event.kind, AlertEventKind::Resolved));
        assert!(event.fire_shutdown);
        assert!(alert.shutdown_scheduled);
        assert!(alert.notified, "the shutdown warning announces the episode");
    }

    #[test]
    fn build_transition_event_full_recovery_clears_episode() {
        // Goal: verify full recovery notifies (episode was announced) and
        // resets the notified flag for the next episode.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Active);
        alert.source_states = vec![AlertState::Inactive];
        alert.notified = true;
        let outcomes = SourceOutcomes {
            resolved: vec!["temp1: back in range".to_string()],
            ..Default::default()
        };
        let event =
            AlertController::process_alert(&mut alert, &outcomes, machines(true, false)).unwrap();
        assert!(matches!(event.kind, AlertEventKind::Resolved));
        assert_eq!(event.alert.state, AlertState::Inactive);
        assert!(event.notify_desktop);
        assert!(!alert.notified);
    }

    #[test]
    fn build_transition_event_recovery_without_fired_notification_stays_quiet() {
        // Goal: verify recovery-only-if-fired: an episode the user was never
        // informed about produces no recovery desktop notification.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Active);
        alert.source_states = vec![AlertState::Inactive];
        alert.notified = false;
        let outcomes = SourceOutcomes {
            resolved: vec!["temp1: back in range".to_string()],
            ..Default::default()
        };
        let event =
            AlertController::build_transition_event(&mut alert, &outcomes, machines(true, false))
                .unwrap();
        assert!(!event.notify_desktop);
        assert!(event.log, "the log/toast layer still records the change");
    }

    #[test]
    fn build_transition_event_silenced_suppresses_notification_and_shutdown() {
        // Goal: verify a silenced fire is logged (flagged silenced) but sends
        // no desktop notification and does not schedule a shutdown.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Inactive);
        alert.shutdown_on_activation = true;
        alert.silenced_until = Some(Local::now() + Duration::seconds(600));
        alert.source_states = vec![AlertState::Active];
        let outcomes = SourceOutcomes {
            fired: vec!["temp1: 90 too high".to_string()],
            ..Default::default()
        };
        let event =
            AlertController::build_transition_event(&mut alert, &outcomes, machines(false, false))
                .unwrap();
        assert!(event.silenced);
        assert!(!event.notify_desktop);
        assert!(!event.fire_shutdown);
        assert!(!alert.shutdown_scheduled);
        assert!(
            alert.notified.not(),
            "a silenced fire leaves the catch-up pending"
        );
        assert!(event.log);
    }

    #[test]
    fn build_transition_event_fires_shutdown_once() {
        // Goal: verify the shutdown command fires when a source activates and
        // is not re-fired for subsequent activations of the same episode.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Inactive);
        alert.shutdown_on_activation = true;
        alert.source_states = vec![AlertState::Active];
        let outcomes = SourceOutcomes {
            fired: vec!["temp1: 90 too high".to_string()],
            ..Default::default()
        };
        let event =
            AlertController::process_alert(&mut alert, &outcomes, machines(false, false)).unwrap();
        assert!(event.fire_shutdown);
        assert!(alert.shutdown_scheduled);

        let again = SourceOutcomes {
            fired: vec!["temp1: 95 too high".to_string()],
            ..Default::default()
        };
        let event =
            AlertController::build_transition_event(&mut alert, &again, machines(true, false))
                .unwrap();
        assert!(!event.fire_shutdown, "shutdown must not be re-fired");
    }

    #[test]
    fn build_transition_event_cancels_shutdown_on_full_recovery() {
        // Goal: verify a pending shutdown is cancelled when all sources clear.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Active);
        alert.shutdown_on_activation = true;
        alert.shutdown_scheduled = true;
        alert.notified = true;
        alert.source_states = vec![AlertState::Inactive];
        let outcomes = SourceOutcomes {
            resolved: vec!["temp1: back in range".to_string()],
            ..Default::default()
        };
        let event =
            AlertController::process_alert(&mut alert, &outcomes, machines(true, false)).unwrap();
        assert!(event.cancel_shutdown);
        assert!(!alert.shutdown_scheduled);
    }

    #[test]
    fn build_transition_event_source_error_state() {
        // Goal: verify an unreadable source produces a SourceError event and
        // the aggregate reports Error.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Inactive);
        alert.source_states = vec![AlertState::Error];
        let outcomes = SourceOutcomes {
            errors: vec!["temp1: Device not found".to_string()],
            ..Default::default()
        };
        let event =
            AlertController::process_alert(&mut alert, &outcomes, machines(false, false)).unwrap();
        assert!(matches!(event.kind, AlertEventKind::SourceError));
        assert_eq!(event.alert.state, AlertState::Error);
        assert!(event.notify_desktop);
    }

    #[test]
    fn process_alert_clears_error_state_over_silent_transitions() {
        // Goal: an alert whose source recovers through the silent timer hops must not
        // strand its aggregate state at Error. `transition_kind` classifies both
        // Error -> WarmUp and WarmUp -> Inactive as silent, so neither tick produces an
        // event; the aggregate has to follow the source states regardless. Method: drive
        // the source through the three states with empty outcomes and assert the
        // aggregate tracks each one.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Error);
        alert.source_states = vec![AlertState::Error];
        // Already announced, as it would be on the tick it became unreadable.
        alert.error_notified = true;
        let quiet = SourceOutcomes::default();

        // The unreadable tick: aggregate reports Error, as the source does.
        assert!(
            AlertController::process_alert(&mut alert, &quiet, machines(false, true)).is_none()
        );
        assert_eq!(alert.state, AlertState::Error);

        // Reading resumes out of range: the source starts its warm-up, which is
        // wire-visible as Inactive. Silent hop, no event.
        alert.source_states = vec![AlertState::WarmUp(Local::now())];
        assert!(
            AlertController::process_alert(&mut alert, &quiet, machines(false, true)).is_none()
        );
        assert_eq!(
            alert.state,
            AlertState::Inactive,
            "aggregate stranded at Error across a silent hop"
        );

        // Back in range for good.
        alert.source_states = vec![AlertState::Inactive];
        assert!(
            AlertController::process_alert(&mut alert, &quiet, machines(false, false)).is_none()
        );
        assert_eq!(alert.state, AlertState::Inactive);
    }

    // -- two-machine edge tests --

    /// A two-source alert with both sources in the given states.
    fn make_edge_alert(first: AlertState, second: AlertState) -> Alert {
        let mut alert = make_alert("a", 0.0, 1000.0, AlertState::Inactive);
        alert.channel_sources = vec![
            make_source("fan1", ChannelMetric::RPM),
            make_source("fan2", ChannelMetric::RPM),
        ];
        alert.source_states = vec![first, second];
        alert.state = alert.worst_of_visible();
        alert
    }

    #[test]
    fn edge_kind_ranks_simultaneous_edges_worst_news_first() {
        // Goal: one event per tick, so simultaneous edges rank Error first.
        let quiet_before = machines(false, false);
        assert!(matches!(
            AlertController::edge_kind(quiet_before, machines(true, true)),
            Some(AlertEventKind::SourceError)
        ));
        assert!(matches!(
            AlertController::edge_kind(quiet_before, machines(true, false)),
            Some(AlertEventKind::Triggered)
        ));
        assert!(matches!(
            AlertController::edge_kind(machines(true, true), machines(true, false)),
            Some(AlertEventKind::ErrorResolved)
        ));
        assert!(matches!(
            AlertController::edge_kind(machines(true, false), quiet_before),
            Some(AlertEventKind::Resolved)
        ));
        assert!(
            AlertController::edge_kind(machines(true, false), machines(true, false)).is_none(),
            "no movement in either machine is no announcement"
        );
    }

    #[test]
    fn an_error_beside_a_firing_source_does_not_disturb_the_active_machine() {
        // Goal: the machines are independent, so an Error beside a firing source
        // produces no spurious trigger or recovery.
        let mut alert = make_edge_alert(AlertState::Active, AlertState::Error);
        alert.notified = true;
        let outcomes = SourceOutcomes {
            errors: vec!["fan2: Device not found".to_string()],
            ..Default::default()
        };
        let event =
            AlertController::build_transition_event(&mut alert, &outcomes, machines(true, false))
                .unwrap();
        assert!(matches!(event.kind, AlertEventKind::SourceError));
        assert!(event.notify_desktop);
        assert!(event.quiet.not());
        assert!(alert.notified, "the Active episode is untouched");
        assert!(alert.error_notified);
        assert_eq!(event.alert.state, AlertState::Error);
    }

    #[test]
    fn sensors_becoming_readable_announces_while_still_active() {
        // Goal: the Error machine has its own clearing edge, announced even
        // while the alert is still firing.
        let mut alert = make_edge_alert(AlertState::Active, AlertState::Inactive);
        alert.notified = true;
        alert.error_notified = true;
        let outcomes = SourceOutcomes {
            resolved: vec!["fan2: back in range".to_string()],
            ..Default::default()
        };
        let event =
            AlertController::build_transition_event(&mut alert, &outcomes, machines(true, true))
                .unwrap();
        assert!(matches!(event.kind, AlertEventKind::ErrorResolved));
        assert!(event.notify_desktop);
        assert_eq!(event.alert.state, AlertState::Active);
        assert!(alert.error_notified.not(), "the Error episode is over");
        assert!(alert.notified, "the Active episode continues");
    }

    #[test]
    fn an_error_body_states_the_resulting_alert_state_and_countdown() {
        // Goal: the Error body carries the resulting state and the countdown.
        let mut alert = make_edge_alert(AlertState::Active, AlertState::Error);
        alert.shutdown_scheduled = true;
        let body = AlertController::with_resulting_state(&alert, "fan2: Device not found");
        assert!(body.starts_with("fan2: Device not found\n"));
        assert!(body.contains("Alert is in an Error state."));
        assert!(body.contains("Shutdown will commence in 1 Minute."));

        let mut clear = make_edge_alert(AlertState::Active, AlertState::Inactive);
        clear.state = clear.worst_of_visible();
        let body = AlertController::with_resulting_state(&clear, "fan2: readable");
        assert!(body.contains("Alert is still Active."));
        assert!(body.contains("Shutdown").not());
    }

    #[test]
    fn a_pending_shutdown_survives_every_source_going_unreadable() {
        // Goal: deliberate safety behaviour, the countdown stands.
        let mut alert = make_edge_alert(AlertState::Error, AlertState::Error);
        alert.shutdown_on_activation = true;
        alert.shutdown_scheduled = true;
        alert.notified = true;
        let outcomes = SourceOutcomes {
            errors: vec!["fan1: Device not found".to_string()],
            ..Default::default()
        };
        let event =
            AlertController::build_transition_event(&mut alert, &outcomes, machines(true, false))
                .unwrap();
        assert!(
            event.cancel_shutdown.not(),
            "an unreadable sensor must not cancel a countdown"
        );
        assert!(alert.shutdown_scheduled);
    }

    #[test]
    fn the_error_catch_up_fires_after_silence_but_never_repeats() {
        // Goal: the catch-up announces once and never repeats, because an
        // Error can be permanent.
        let mut alert = make_edge_alert(AlertState::Error, AlertState::Inactive);
        alert.repeat_interval = 60.0;
        alert.last_notified = Some(Local::now() - Duration::seconds(600));
        let outcomes = SourceOutcomes {
            unreadable: vec!["fan1: Device not found".to_string()],
            ..Default::default()
        };
        let event = AlertController::build_quiet_event(&mut alert, &outcomes).unwrap();
        assert!(matches!(event.kind, AlertEventKind::SourceError));
        assert!(event.notify_desktop);
        assert!(alert.error_notified);

        alert.last_notified = Some(Local::now() - Duration::seconds(600));
        assert!(
            AlertController::build_quiet_event(&mut alert, &outcomes).is_none(),
            "a permanent Error must not notify on every repeat interval"
        );
    }

    // -- build_quiet_event tests (silence expiry, shutdown re-arm, repeat) --

    /// Drains the recorded commands, so each test starts from a known state.
    fn take_fired_commands() -> Vec<String> {
        FIRED_COMMANDS.with_borrow_mut(std::mem::take)
    }

    /// Builds an alert mid-countdown: shutdown armed and already issued.
    fn make_alert_with_pending_shutdown() -> Alert {
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Active);
        alert.shutdown_on_activation = true;
        alert.shutdown_scheduled = true;
        alert
    }

    #[test]
    fn cancel_shutdown_if_unwanted_cancels_when_shutdown_behaviour_turned_off() {
        // Goal: unchecking shutdown_on_activation mid-countdown must cancel. The
        // update path carries shutdown_scheduled forward when the source set is
        // unchanged, so without this the machine halts after the user turned the
        // behaviour off.
        let _ = take_fired_commands();
        let mut alert = make_alert_with_pending_shutdown();
        alert.shutdown_on_activation = false;
        AlertController::cancel_shutdown_if_unwanted(&mut alert);
        assert!(alert.shutdown_scheduled.not());
        assert_eq!(
            take_fired_commands(),
            vec![COMMAND_SHUTDOWN_CANCEL.to_string()]
        );
    }

    #[test]
    fn cancel_shutdown_if_unwanted_cancels_on_disable_and_on_silence() {
        // Goal: the two originally-handled quieting actions still cancel, so
        // broadening the condition did not drop the behaviour it replaced.
        let _ = take_fired_commands();
        let mut disabled = make_alert_with_pending_shutdown();
        disabled.enabled = false;
        AlertController::cancel_shutdown_if_unwanted(&mut disabled);
        assert!(disabled.shutdown_scheduled.not());

        let mut silenced = make_alert_with_pending_shutdown();
        silenced.silenced_until = Some(Local::now() + Duration::seconds(600));
        AlertController::cancel_shutdown_if_unwanted(&mut silenced);
        assert!(silenced.shutdown_scheduled.not());

        assert_eq!(take_fired_commands().len(), 2);
    }

    #[test]
    fn cancel_shutdown_if_unwanted_leaves_a_wanted_shutdown_alone() {
        // Goal: negative space. An edit that changes nothing about the alert's
        // quieting must not fire a spurious cancel, and a second pass over an
        // already-cancelled alert must stay silent.
        let _ = take_fired_commands();
        let mut alert = make_alert_with_pending_shutdown();
        AlertController::cancel_shutdown_if_unwanted(&mut alert);
        assert!(alert.shutdown_scheduled, "the shutdown is still wanted");

        alert.enabled = false;
        AlertController::cancel_shutdown_if_unwanted(&mut alert);
        AlertController::cancel_shutdown_if_unwanted(&mut alert);
        assert_eq!(
            take_fired_commands().len(),
            1,
            "clearing the flag prevents a second cancel"
        );
    }

    #[test]
    fn cancel_shutdown_on_delete_cancels_a_pending_shutdown() {
        // Goal: deleting an alert mid-countdown must cancel, since nothing is
        // left to resolve it. An alert with no pending shutdown stays silent.
        let _ = take_fired_commands();
        AlertController::cancel_shutdown_on_delete(&make_alert_with_pending_shutdown());
        assert_eq!(
            take_fired_commands(),
            vec![COMMAND_SHUTDOWN_CANCEL.to_string()]
        );

        let mut idle = make_alert_with_pending_shutdown();
        idle.shutdown_scheduled = false;
        AlertController::cancel_shutdown_on_delete(&idle);
        assert!(take_fired_commands().is_empty());
    }

    // -- Source-set edit tests --

    /// Builds a two-source alert held Active by `temp1` while `fan1` sits clear.
    fn make_two_source_alert() -> Alert {
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Active);
        alert
            .channel_sources
            .push(make_source("fan1", ChannelMetric::Temp));
        alert.source_states = vec![AlertState::Active, AlertState::Inactive];
        alert.notified = true;
        alert
    }

    #[test]
    fn dropping_the_only_firing_source_clears_the_alert() {
        // Goal: the reported bug, over update()'s real sequence. Removing the source
        // holding the alert Active clears it, and the alert's own state changed, so
        // it still notifies even though that source's recovery path is gone.
        let existing_alert = make_two_source_alert();
        let before = existing_alert.machine_state();
        let mut alert = make_two_source_alert();
        alert.channel_sources.remove(0);
        alert.source_states = vec![AlertState::Inactive];
        alert.notified = false;

        AlertController::carry_surviving_source_states(&mut alert, &existing_alert);

        assert_eq!(alert.state, AlertState::Inactive);
        assert_eq!(alert.source_states, vec![AlertState::Inactive]);
        assert!(
            alert.notified,
            "the carry must not clear the episode before the gate reads it"
        );

        let event = AlertController::build_edit_event(&mut alert, before).unwrap();
        assert!(matches!(event.kind, AlertEventKind::Resolved));
        assert!(event.notify_desktop, "the alert's own state changed");
        assert!(
            alert.notified.not(),
            "a cleared alert must be able to announce a later episode"
        );
    }

    #[test]
    fn dropping_a_clear_source_keeps_the_alert_active() {
        // Goal: the negative space of the case above. Removing a source that was
        // not firing must leave the still-firing survivor untouched, so the alarm
        // is neither dropped nor forced through warmup again.
        let existing_alert = make_two_source_alert();
        let mut alert = make_two_source_alert();
        alert.channel_sources.remove(1);
        alert.source_states = vec![AlertState::Inactive];

        AlertController::carry_surviving_source_states(&mut alert, &existing_alert);

        assert_eq!(alert.state, AlertState::Active);
        assert_eq!(alert.source_states, vec![AlertState::Active]);
        assert!(alert.notified, "the episode was already announced");
    }

    #[test]
    fn an_added_source_starts_inactive_and_survivors_keep_their_state() {
        // Goal: adding a source must not disturb the sources already being
        // watched, and the new one must warm up on its own terms.
        let existing_alert = make_two_source_alert();
        let mut alert = make_two_source_alert();
        alert
            .channel_sources
            .push(make_source("fan2", ChannelMetric::Temp));
        alert.source_states = vec![AlertState::Inactive; 3];

        AlertController::carry_surviving_source_states(&mut alert, &existing_alert);

        assert_eq!(
            alert.source_states,
            vec![
                AlertState::Active,
                AlertState::Inactive,
                AlertState::Inactive
            ]
        );
        assert_eq!(alert.state, AlertState::Active);
    }

    #[test]
    fn reordering_sources_carries_state_by_identity_not_position() {
        // Goal: the carry-over matches on the source itself, so moving a source
        // in the list cannot hand its state to a different channel.
        let existing_alert = make_two_source_alert();
        let mut alert = make_two_source_alert();
        alert.channel_sources.swap(0, 1);
        alert.source_states = vec![AlertState::Inactive; 2];

        AlertController::carry_surviving_source_states(&mut alert, &existing_alert);

        assert_eq!(
            alert.source_states,
            vec![AlertState::Inactive, AlertState::Active]
        );
        assert_eq!(alert.state, AlertState::Active);
    }

    #[test]
    fn a_warming_survivor_keeps_its_pending_timer() {
        // Goal: a source part-way through warmup must not restart the clock, or
        // an edit would postpone every alarm by the full warmup duration.
        let started = Local::now() - Duration::seconds(30);
        let mut existing_alert = make_two_source_alert();
        existing_alert.source_states = vec![AlertState::WarmUp(started), AlertState::Inactive];
        existing_alert.state = AlertState::Inactive;
        let mut alert = make_two_source_alert();
        alert.channel_sources.remove(1);
        alert.source_states = vec![AlertState::Inactive];

        AlertController::carry_surviving_source_states(&mut alert, &existing_alert);

        assert_eq!(alert.source_states, vec![AlertState::WarmUp(started)]);
        assert_eq!(
            alert.state,
            AlertState::Inactive,
            "warmup is not yet wire-visible"
        );
    }

    #[test]
    fn an_edit_announces_only_when_it_crosses_an_edge() {
        // Goal: the edit path uses a tick's machines, so only dropping the
        // firing source announces.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Inactive);
        alert.source_states = vec![AlertState::Inactive];
        assert!(
            AlertController::build_edit_event(&mut alert, machines(false, false)).is_none(),
            "an unchanged state has nothing to announce"
        );

        alert.notified = true;
        let event = AlertController::build_edit_event(&mut alert, machines(true, false))
            .expect("dropping the firing source clears the alert");
        assert!(matches!(event.kind, AlertEventKind::Resolved));
        assert_eq!(event.message, MESSAGE_TRIGGER_UNWATCHED);
        assert!(
            event.notify_desktop,
            "the episode was announced, so is its end"
        );
        assert!(event.quiet.not());
        assert!(alert.notified.not(), "the episode is over");

        alert.enabled = false;
        assert!(
            AlertController::build_edit_event(&mut alert, machines(true, false)).is_none(),
            "disabling is reset silently by design"
        );
    }

    #[test]
    fn carry_runtime_state_preserves_the_whole_episode() {
        // Goal: guards the unchanged-sources path, where a min/max edit must keep
        // every runtime field so the next tick announces the transition itself.
        let mut existing_alert = make_two_source_alert();
        existing_alert.shutdown_scheduled = true;
        existing_alert.last_notified = Some(Local::now());
        let mut alert = make_two_source_alert();
        alert.state = AlertState::Inactive;
        alert.source_states = vec![AlertState::Inactive; 2];
        alert.notified = false;
        alert.shutdown_scheduled = false;

        AlertController::carry_runtime_state(&mut alert, &existing_alert);

        assert_eq!(alert.state, existing_alert.state);
        assert_eq!(alert.source_states, existing_alert.source_states);
        assert!(alert.notified);
        assert_eq!(alert.last_notified, existing_alert.last_notified);
        assert!(alert.shutdown_scheduled);
    }

    #[test]
    fn carry_runtime_state_preserves_an_announced_error() {
        // Goal: an unrelated edit while a sensor is unreadable must not re-arm the
        // Error catch-up, which has no repeat by design, nor lose the recovery.
        let mut existing_alert = make_edge_alert(AlertState::Error, AlertState::Inactive);
        existing_alert.error_notified = true;
        let mut alert = make_edge_alert(AlertState::Error, AlertState::Inactive);
        alert.name = "renamed".to_string();
        alert.error_notified = false;

        AlertController::carry_runtime_state(&mut alert, &existing_alert);

        assert!(alert.error_notified);
        let outcomes = SourceOutcomes {
            unreadable: vec!["fan1: Device not found".to_string()],
            ..Default::default()
        };
        assert!(
            AlertController::build_quiet_event(&mut alert, &outcomes).is_none(),
            "an edit must not re-announce an Error the user was already told about"
        );

        let before = alert.machine_state();
        alert.source_states = vec![AlertState::Inactive; 2];
        alert.state = alert.worst_of_visible();
        let event = AlertController::build_edit_event(&mut alert, before).unwrap();
        assert!(matches!(event.kind, AlertEventKind::ErrorResolved));
        assert_eq!(
            event.message, MESSAGE_UNREADABLE_UNWATCHED,
            "the text names the sensor that left, not the one still firing"
        );
        assert!(
            event.notify_desktop,
            "the recovery still announces because the Error was carried"
        );
    }

    #[test]
    fn a_source_edit_cancels_a_pending_shutdown() {
        // Goal: the countdown was armed for a source set the user has just
        // changed, so it is cancelled here; the tick loop re-arms a fresh one
        // while a surviving source still holds the alert active.
        let _ = take_fired_commands();
        let existing_alert = make_alert_with_pending_shutdown();
        let alert = make_alert("a", 20.0, 80.0, AlertState::Active);
        AlertController::cancel_shutdown_on_source_change(&alert, &existing_alert);
        assert_eq!(
            take_fired_commands(),
            vec![COMMAND_SHUTDOWN_CANCEL.to_string()]
        );
        assert!(
            alert.shutdown_scheduled.not(),
            "the tick loop must see an unarmed alert to re-arm it"
        );

        let mut idle = make_alert_with_pending_shutdown();
        idle.shutdown_scheduled = false;
        AlertController::cancel_shutdown_on_source_change(&alert, &idle);
        assert!(take_fired_commands().is_empty());
    }

    #[test]
    fn build_quiet_event_announces_after_silence_lapses() {
        // Goal: verify the notify-on-expiry catch-up: an episode that fired
        // silently is announced on the first unsilenced tick.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Active);
        alert.source_states = vec![AlertState::Active];
        alert.notified = false;
        let outcomes = SourceOutcomes {
            out_of_range: vec!["temp1: 90 too high".to_string()],
            ..Default::default()
        };
        let event = AlertController::build_quiet_event(&mut alert, &outcomes).unwrap();
        assert!(matches!(event.kind, AlertEventKind::StillActive));
        assert!(event.notify_desktop);
        assert!(event.log);
        assert!(alert.notified);
    }

    #[test]
    fn build_quiet_event_rearms_shutdown_after_silence() {
        // Goal: verify a shutdown cancelled by silencing is re-armed once the
        // silence lapses and the alert is still strictly active.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Active);
        alert.shutdown_on_activation = true;
        alert.shutdown_scheduled = false;
        alert.notified = true;
        alert.source_states = vec![AlertState::Active];
        let outcomes = SourceOutcomes {
            out_of_range: vec!["temp1: 90 too high".to_string()],
            ..Default::default()
        };
        let event = AlertController::build_quiet_event(&mut alert, &outcomes).unwrap();
        assert!(matches!(event.kind, AlertEventKind::StillActive));
        assert!(event.fire_shutdown);
        assert!(alert.shutdown_scheduled);
    }

    #[test]
    fn build_quiet_event_none_while_silenced() {
        // Goal: verify no catch-up or repeat happens while still silenced.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Active);
        alert.silenced_until = Some(Local::now() + Duration::seconds(600));
        alert.source_states = vec![AlertState::Active];
        alert.notified = false;
        let outcomes = SourceOutcomes::default();
        assert!(AlertController::build_quiet_event(&mut alert, &outcomes).is_none());
    }

    #[test]
    fn build_quiet_event_none_when_not_strictly_active() {
        // Goal: verify Cooldown (value back in range) and Inactive sources
        // produce no quiet-tick events.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Active);
        alert.notified = false;
        alert.source_states = vec![AlertState::Cooldown(Local::now())];
        let outcomes = SourceOutcomes::default();
        assert!(AlertController::build_quiet_event(&mut alert, &outcomes).is_none());
        alert.source_states = vec![AlertState::Inactive];
        assert!(AlertController::build_quiet_event(&mut alert, &outcomes).is_none());
    }

    #[test]
    fn build_quiet_event_repeat_after_interval() {
        // Goal: verify the repeat notification fires once the interval elapsed
        // and is desktop-only (no log entry), then re-arms the timer.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Active);
        alert.repeat_interval = 60.0;
        alert.notified = true;
        alert.last_notified = Some(Local::now() - Duration::seconds(61));
        alert.source_states = vec![AlertState::Active];
        let outcomes = SourceOutcomes {
            out_of_range: vec!["temp1: 90 too high".to_string()],
            ..Default::default()
        };
        let event = AlertController::build_quiet_event(&mut alert, &outcomes).unwrap();
        assert!(matches!(event.kind, AlertEventKind::Repeat));
        assert!(event.notify_desktop);
        assert!(!event.log, "repeats must not flood the log ring buffer");
        // The timer was just re-armed: no second repeat right away.
        assert!(AlertController::build_quiet_event(&mut alert, &outcomes).is_none());
    }

    #[test]
    fn build_quiet_event_no_repeat_before_interval_or_without_desktop() {
        // Goal: verify repeat obeys the interval and requires desktop_notify.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Active);
        alert.repeat_interval = 60.0;
        alert.notified = true;
        alert.last_notified = Some(Local::now() - Duration::seconds(5));
        alert.source_states = vec![AlertState::Active];
        let outcomes = SourceOutcomes {
            out_of_range: vec!["temp1: 90 too high".to_string()],
            ..Default::default()
        };
        assert!(AlertController::build_quiet_event(&mut alert, &outcomes).is_none());

        alert.last_notified = Some(Local::now() - Duration::seconds(61));
        alert.desktop_notify = false;
        assert!(AlertController::build_quiet_event(&mut alert, &outcomes).is_none());
    }

    // -- AlertState serialization/deserialization tests --

    #[test]
    fn alert_state_serialize_active() {
        // Goal: verify Active serializes to the string "Active".
        let json = serde_json::to_string(&AlertState::Active).unwrap();
        assert_eq!(json, "\"Active\"");
    }

    #[test]
    fn alert_state_serialize_inactive() {
        // Goal: verify Inactive serializes to the string "Inactive".
        let json = serde_json::to_string(&AlertState::Inactive).unwrap();
        assert_eq!(json, "\"Inactive\"");
    }

    #[test]
    fn alert_state_serialize_error() {
        // Goal: verify Error serializes to the string "Error".
        let json = serde_json::to_string(&AlertState::Error).unwrap();
        assert_eq!(json, "\"Error\"");
    }

    #[test]
    fn alert_state_serialize_warmup_as_inactive() {
        // Goal: verify WarmUp serializes as "Inactive" — WarmUp is an
        // internal state that should not be exposed in persisted data.
        let json = serde_json::to_string(&AlertState::WarmUp(Local::now())).unwrap();
        assert_eq!(json, "\"Inactive\"");
    }

    #[test]
    fn alert_state_serialize_cooldown_as_active() {
        // Goal: verify Cooldown serializes as "Active": the source never
        // stopped firing from the outside, and the wire keeps 3 states.
        let json = serde_json::to_string(&AlertState::Cooldown(Local::now())).unwrap();
        assert_eq!(json, "\"Active\"");
    }

    #[test]
    fn alert_state_deserialize_cooldown_maps_to_active() {
        // Goal: verify a defensive mapping for "Cooldown" in JSON, which is
        // never written but must not brick a config file if it appears.
        let state: AlertState = serde_json::from_str("\"Cooldown\"").unwrap();
        assert_eq!(state, AlertState::Active);
    }

    #[test]
    fn alert_state_deserialize_active() {
        // Goal: verify "Active" deserializes to Active.
        let state: AlertState = serde_json::from_str("\"Active\"").unwrap();
        assert_eq!(state, AlertState::Active);
    }

    #[test]
    fn alert_state_deserialize_inactive() {
        // Goal: verify "Inactive" deserializes to Inactive.
        let state: AlertState = serde_json::from_str("\"Inactive\"").unwrap();
        assert_eq!(state, AlertState::Inactive);
    }

    #[test]
    fn alert_state_deserialize_error() {
        // Goal: verify "Error" deserializes to Error.
        let state: AlertState = serde_json::from_str("\"Error\"").unwrap();
        assert_eq!(state, AlertState::Error);
    }

    #[test]
    fn alert_state_deserialize_warmup_maps_to_inactive() {
        // Goal: verify "WarmUp" in JSON deserializes as Inactive,
        // since WarmUp requires a timestamp that isn't persisted.
        let state: AlertState = serde_json::from_str("\"WarmUp\"").unwrap();
        assert_eq!(state, AlertState::Inactive);
    }

    #[test]
    fn alert_state_deserialize_unknown_variant_fails() {
        // Goal: verify unknown state strings are rejected.
        let result = serde_json::from_str::<AlertState>("\"Unknown\"");
        assert!(result.is_err());
    }

    // -- AlertConfigFile and AlertLogsFile serde tests --

    #[test]
    fn alert_config_file_serializes_without_logs() {
        // Goal: verify that AlertConfigFile never writes the `logs` field now
        // that logs live in the separate data-dir file.
        let config = AlertConfigFile {
            alerts: vec![
                make_alert("uid-1", 10.0, 90.0, AlertState::Inactive),
                make_alert("uid-2", 20.0, 80.0, AlertState::Active),
            ],
            logs: vec![AlertLog::default()],
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(
            json.contains("\"logs\"").not(),
            "logs must not appear in config JSON"
        );
        let parsed: AlertConfigFile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.alerts.len(), 2);
        assert!(parsed.logs.is_empty());
        assert_eq!(parsed.alerts[0].uid, "uid-1");
        assert_eq!(parsed.alerts[1].state, AlertState::Active);
    }

    #[test]
    fn alert_config_file_legacy_deserializes_logs_for_migration() {
        // Goal: verify that an old-format alerts.json containing a `logs` array
        // can be deserialized into AlertConfigFile.logs, enabling one-time
        // migration to the data-dir location without data loss.
        let legacy_json = r#"{
            "alerts": [],
            "logs": [
                {"uid":"uid-1","name":"A","state":"Active","message":"over threshold",
                 "timestamp":"2025-01-01T00:00:00+00:00"}
            ]
        }"#;
        let parsed: AlertConfigFile = serde_json::from_str(legacy_json).unwrap();
        assert!(parsed.alerts.is_empty());
        assert_eq!(parsed.logs.len(), 1);
        assert_eq!(parsed.logs[0].uid, "uid-1");
        assert_eq!(parsed.logs[0].message, "over threshold");
    }

    #[test]
    fn alert_logs_file_serde_roundtrip() {
        // Goal: verify that AlertLogsFile survives a JSON round-trip, which is
        // the persistence format for the new data-dir logs file.
        let logs = vec![AlertLog {
            uid: "uid-1".to_string(),
            name: "Alert-uid-1".to_string(),
            state: AlertState::Active,
            message: "Over threshold".to_string(),
            timestamp: Local::now(),
            silenced: false,
            resolved: false,
            kind: AlertLogKind::Triggered,
            quiet: false,
        }];
        let file = AlertLogsFile { logs };
        let json = serde_json::to_string(&file).unwrap();
        assert!(json.contains("\"logs\""));
        let parsed: AlertLogsFile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.logs.len(), 1);
        assert_eq!(parsed.logs[0].uid, "uid-1");
        assert_eq!(parsed.logs[0].message, "Over threshold");
    }

    // -- Alert serde defaults test --

    #[test]
    #[allow(clippy::float_cmp)]
    fn alert_deserialize_with_missing_optional_fields() {
        // Goal: verify that missing optional fields get their serde defaults,
        // which is important for backwards compatibility with older configs.
        let json = r#"{
            "uid": "test-uid",
            "name": "Test Alert",
            "channel_source": {
                "device_uid": "dev",
                "channel_name": "ch",
                "channel_metric": "Temp"
            },
            "min": 10.0,
            "max": 90.0,
            "state": "Inactive"
        }"#;
        let alert: Alert = serde_json::from_str(json).unwrap();
        assert_eq!(alert.warmup_duration, 0.0);
        assert_eq!(alert.cooldown_duration, 0.0);
        assert_eq!(alert.repeat_interval, 0.0);
        assert!(alert.enabled);
        assert!(alert.silenced_until.is_none());
        assert!(
            alert.channel_sources.is_empty(),
            "seeded later by normalize"
        );
        assert!(alert.desktop_notify);
        assert!(alert.desktop_notify_recovery);
        assert!(!alert.desktop_notify_audio);
        assert!(!alert.shutdown_on_activation);
    }

    #[test]
    fn alert_serializes_both_source_fields() {
        // Goal: verify a normalized alert writes BOTH the legacy
        // channel_source and the channel_sources list, so a 4.3.x daemon
        // can still load alerts.json after a downgrade (DOWNGRADE-COMPAT).
        let mut alert = make_alert("uid-1", 0.0, 1000.0, AlertState::Inactive);
        alert.channel_sources = vec![
            make_source("fan1", ChannelMetric::RPM),
            make_source("fan2", ChannelMetric::RPM),
        ];
        alert.normalize_sources();
        let json = serde_json::to_string(&alert).unwrap();
        assert!(json.contains("\"channel_source\""));
        assert!(json.contains("\"channel_sources\""));
        let parsed: Alert = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.channel_source, parsed.channel_sources[0]);
        assert_eq!(parsed.channel_sources.len(), 2);
    }

    // -- AlertLog serde compatibility --

    #[test]
    fn alert_log_deserializes_without_silenced_field() {
        // Goal: verify old log entries (no silenced field) still load,
        // defaulting to not-silenced.
        let json = r#"{"uid":"uid-1","name":"A","state":"Active","message":"m",
            "timestamp":"2025-01-01T00:00:00+00:00"}"#;
        let log: AlertLog = serde_json::from_str(json).unwrap();
        assert!(!log.silenced);
        assert!(!log.resolved);
    }

    // -- calibration suppression tests (controller-level) --

    use crate::device::{ChannelStatus, Device, DeviceInfo, DeviceType, Status, TempStatus};
    use std::collections::HashMap;

    /// A device with a `temp1` sensor plus the given fan channels and rpm values.
    fn make_test_device(fans: &[(&str, u32)], temp: f64) -> Device {
        let mut device = Device::new(
            "alert-test-device".to_string(),
            DeviceType::Hwmon,
            0,
            None,
            DeviceInfo::default(),
            Some("alert-test-id".to_string()),
            1.0,
        );
        device.initialize_status_history_with(
            Status {
                timestamp: Local::now(),
                temps: vec![TempStatus {
                    name: "temp1".to_string(),
                    temp,
                }],
                channels: fans
                    .iter()
                    .map(|(name, rpm)| ChannelStatus {
                        name: (*name).to_string(),
                        duty: Some(50.0),
                        rpm: Some(*rpm),
                        freq: None,
                        watts: None,
                        pwm_mode: None,
                    })
                    .collect(),
            },
            1.0,
        );
        device
    }

    /// A controller with one device and an externally-held diagnosis registry;
    /// bypasses init() so no config files are touched.
    fn make_test_controller(device: Device, registry: &Rc<DiagnosisRegistry>) -> AlertController {
        let mut devices = HashMap::new();
        devices.insert(device.uid.clone(), Rc::new(RefCell::new(device)));
        AlertController {
            all_devices: Rc::new(devices),
            overrides: Rc::new(OverridesController::empty()),
            diagnosis_registry: Rc::clone(registry),
            alerts: RefCell::new(IndexMap::new()),
            alert_handle: RefCell::new(None),
            notification_handle: RefCell::new(None),
            logs: RefCell::new(VecDeque::with_capacity(LOG_BUFFER_SIZE)),
            logs_dirty: Cell::new(false),
            last_log_flush: Cell::new(Instant::now()),
        }
    }

    /// An RPM alert over the given fan channels with warmup 0 (two-tick fire).
    fn rpm_alert(device_uid: &str, channels: &[&str], min: f64, max: f64) -> Alert {
        let mut alert = make_alert("rpm-alert", min, max, AlertState::Inactive);
        alert.channel_sources = channels
            .iter()
            .map(|name| ChannelSource {
                device_uid: device_uid.to_string(),
                channel_name: (*name).to_string(),
                channel_metric: ChannelMetric::RPM,
            })
            .collect();
        alert.normalize_sources();
        alert
    }

    #[test]
    fn dropping_the_firing_source_logs_a_recovery() {
        // Goal: the reported bug, end to end. No later tick can announce this,
        // so update() must, or clients keep showing a cleared alert.
        crate::rt::test_runtime(async {
            let registry = Rc::new(DiagnosisRegistry::new());
            let device = make_test_device(&[("fan1", 0), ("fan2", 1200)], 30.0);
            let controller = make_test_controller(device, &registry);

            let mut alert = rpm_alert("dev1", &["fan1", "fan2"], 600.0, 2000.0);
            alert.source_states = vec![AlertState::Active, AlertState::Inactive];
            alert.state = AlertState::Active;
            alert.notified = true;
            controller.create(alert.clone()).await.unwrap();

            let mut edited = rpm_alert("dev1", &["fan2"], 600.0, 2000.0);
            edited.uid.clone_from(&alert.uid);
            controller.update(edited).await.unwrap();

            let logs = controller.logs.borrow();
            let entry = logs
                .back()
                .expect("the edit must announce the clearing edge");
            assert_eq!(entry.state, AlertState::Inactive);
            assert_eq!(entry.kind, AlertLogKind::Resolved);
            assert_eq!(entry.message, MESSAGE_TRIGGER_UNWATCHED);
            assert!(entry.resolved, "the deprecated mirror still tracks kind");
            assert!(entry.quiet.not(), "an alert-level edge is not a detail row");
        });
    }

    #[test]
    fn suppression_blocks_rpm_alert_during_sweep() {
        // Goal: verify an RPM source on a channel under calibration is never
        // evaluated: rpm 0 during the sweep produces no warmup, no event, no
        // shutdown, and no log entries.
        let registry = Rc::new(DiagnosisRegistry::new());
        let device = make_test_device(&[("fan1", 0)], 30.0);
        let device_uid = device.uid.clone();
        let controller = make_test_controller(device, &registry);
        let mut alert = rpm_alert(&device_uid, &["fan1"], 500.0, 10_000.0);
        alert.shutdown_on_activation = true;
        controller
            .alerts
            .borrow_mut()
            .insert(alert.uid.clone(), alert);
        let _token = registry.register((device_uid, "fan1".to_string()));

        for _ in 0..3 {
            controller.process_alerts();
        }
        let alerts = controller.alerts.borrow();
        let alert = alerts.get("rpm-alert").unwrap();
        assert_eq!(alert.state, AlertState::Inactive);
        assert_eq!(alert.source_states[0], AlertState::Inactive);
        assert!(!alert.shutdown_scheduled);
        assert!(controller.logs.borrow().is_empty());
    }

    #[test]
    fn suppression_leaves_temp_sources_live() {
        // Goal: verify Temp sources are exempt from calibration suppression even
        // when a sweep is registered under the same channel name; the temp alert
        // still warms up and fires.
        let registry = Rc::new(DiagnosisRegistry::new());
        let device = make_test_device(&[("fan1", 1000)], 90.0);
        let device_uid = device.uid.clone();
        let controller = make_test_controller(device, &registry);
        let mut alert = make_alert("temp-alert", 0.0, 80.0, AlertState::Inactive);
        alert.channel_sources = vec![ChannelSource {
            device_uid: device_uid.clone(),
            channel_name: "temp1".to_string(),
            channel_metric: ChannelMetric::Temp,
        }];
        alert.normalize_sources();
        controller
            .alerts
            .borrow_mut()
            .insert(alert.uid.clone(), alert);
        let _token = registry.register((device_uid, "temp1".to_string()));

        // Tick 1: Inactive -> WarmUp, silent.
        assert!(controller.process_and_collect_alerts_to_fire().is_empty());
        // Tick 2: WarmUp -> Active fires despite the registered sweep.
        let events = controller.process_and_collect_alerts_to_fire();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0].kind, AlertEventKind::Triggered));
    }

    #[test]
    fn suppression_only_hits_the_swept_channel() {
        // Goal: verify that with two RPM sources and only one under calibration,
        // the other still fires and the event names only the live channel. This
        // also covers queued-but-not-running batch entries, which are absent
        // from the registry.
        let registry = Rc::new(DiagnosisRegistry::new());
        let device = make_test_device(&[("fan1", 0), ("fan2", 0)], 30.0);
        let device_uid = device.uid.clone();
        let controller = make_test_controller(device, &registry);
        let alert = rpm_alert(&device_uid, &["fan1", "fan2"], 500.0, 10_000.0);
        controller
            .alerts
            .borrow_mut()
            .insert(alert.uid.clone(), alert);
        let _token = registry.register((device_uid, "fan1".to_string()));

        assert!(controller.process_and_collect_alerts_to_fire().is_empty());
        let events = controller.process_and_collect_alerts_to_fire();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0].kind, AlertEventKind::Triggered));
        assert!(events[0].message.contains("fan2"));
        assert!(!events[0].message.contains("fan1"));
        let alerts = controller.alerts.borrow();
        let alert = alerts.get("rpm-alert").unwrap();
        assert_eq!(
            alert.source_states[0],
            AlertState::Inactive,
            "swept channel stays quiet"
        );
        assert_eq!(alert.source_states[1], AlertState::Active);
    }

    #[test]
    fn sweep_start_cancels_pending_shutdown_silently() {
        // Goal: verify a calibration starting on the channel of an already-Active
        // shutdown alert resets the source, cancels the pending shutdown via a
        // non-logging event, and stays quiet on further suppressed ticks.
        let registry = Rc::new(DiagnosisRegistry::new());
        let device = make_test_device(&[("fan1", 0)], 30.0);
        let device_uid = device.uid.clone();
        let controller = make_test_controller(device, &registry);
        let mut alert = rpm_alert(&device_uid, &["fan1"], 500.0, 10_000.0);
        alert.shutdown_on_activation = true;
        controller
            .alerts
            .borrow_mut()
            .insert(alert.uid.clone(), alert);

        // Two ticks: WarmUp, then Active with a shutdown scheduled.
        assert!(controller.process_and_collect_alerts_to_fire().is_empty());
        let events = controller.process_and_collect_alerts_to_fire();
        assert_eq!(events.len(), 1);
        assert!(events[0].fire_shutdown);

        let _token = registry.register((device_uid, "fan1".to_string()));
        let events = controller.process_and_collect_alerts_to_fire();
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert!(event.cancel_shutdown);
        assert!(!event.log);
        assert!(!event.notify_desktop);
        {
            let alerts = controller.alerts.borrow();
            let alert = alerts.get("rpm-alert").unwrap();
            assert_eq!(alert.state, AlertState::Inactive);
            assert!(!alert.shutdown_scheduled);
            assert!(!alert.notified);
        }
        // Further suppressed ticks stay completely quiet.
        assert!(controller.process_and_collect_alerts_to_fire().is_empty());
    }

    // -- calibration preflight gate (active_alert_for_channel) tests --

    #[test]
    fn gate_reports_active_alert_on_channel() {
        // Goal: verify an enabled, unsilenced alert with a visibly Active
        // non-Temp source on the channel blocks calibration (returns its name),
        // including a Cooldown source (the episode is not over), and only for
        // the matching channel.
        let registry = Rc::new(DiagnosisRegistry::new());
        let device = make_test_device(&[("fan1", 0)], 30.0);
        let device_uid = device.uid.clone();
        let controller = make_test_controller(device, &registry);
        let mut alert = rpm_alert(&device_uid, &["fan1"], 500.0, 10_000.0);
        alert.source_states = vec![AlertState::Active];
        controller
            .alerts
            .borrow_mut()
            .insert(alert.uid.clone(), alert);

        assert_eq!(
            controller.active_alert_for_channel(&device_uid, "fan1"),
            Some("Alert-rpm-alert".to_string())
        );
        assert!(controller
            .active_alert_for_channel(&device_uid, "fan2")
            .is_none());
        controller
            .alerts
            .borrow_mut()
            .get_mut("rpm-alert")
            .unwrap()
            .source_states = vec![AlertState::Cooldown(Local::now())];
        assert!(controller
            .active_alert_for_channel(&device_uid, "fan1")
            .is_some());
    }

    #[test]
    fn gate_ignores_quiet_and_irrelevant_alerts() {
        // Goal: verify disabled, silenced, not-yet-Active, and Temp-source
        // alerts do not block calibration.
        let registry = Rc::new(DiagnosisRegistry::new());
        let device = make_test_device(&[("fan1", 0)], 30.0);
        let device_uid = device.uid.clone();
        let controller = make_test_controller(device, &registry);
        let mut alert = rpm_alert(&device_uid, &["fan1"], 500.0, 10_000.0);
        alert.source_states = vec![AlertState::Active];
        alert.enabled = false;
        controller
            .alerts
            .borrow_mut()
            .insert(alert.uid.clone(), alert);
        assert!(controller
            .active_alert_for_channel(&device_uid, "fan1")
            .is_none());

        {
            let mut alerts = controller.alerts.borrow_mut();
            let alert = alerts.get_mut("rpm-alert").unwrap();
            alert.enabled = true;
            alert.silenced_until = Some(Local::now() + Duration::seconds(600));
        }
        assert!(controller
            .active_alert_for_channel(&device_uid, "fan1")
            .is_none());

        {
            let mut alerts = controller.alerts.borrow_mut();
            let alert = alerts.get_mut("rpm-alert").unwrap();
            alert.silenced_until = None;
            alert.source_states = vec![AlertState::WarmUp(Local::now())];
        }
        assert!(controller
            .active_alert_for_channel(&device_uid, "fan1")
            .is_none());

        let mut temp_alert = make_alert("temp-alert", 0.0, 20.0, AlertState::Active);
        temp_alert.channel_sources = vec![ChannelSource {
            device_uid: device_uid.clone(),
            channel_name: "temp1".to_string(),
            channel_metric: ChannelMetric::Temp,
        }];
        temp_alert.normalize_sources();
        temp_alert.source_states = vec![AlertState::Active];
        controller
            .alerts
            .borrow_mut()
            .insert(temp_alert.uid.clone(), temp_alert);
        assert!(controller
            .active_alert_for_channel(&device_uid, "temp1")
            .is_none());
    }

    #[test]
    fn post_sweep_rearm_goes_through_warmup() {
        // Goal: verify that after the sweep ends, a still-failing source re-arms
        // through the normal WarmUp path and fires fresh, with a new shutdown.
        let registry = Rc::new(DiagnosisRegistry::new());
        let device = make_test_device(&[("fan1", 0)], 30.0);
        let device_uid = device.uid.clone();
        let controller = make_test_controller(device, &registry);
        let mut alert = rpm_alert(&device_uid, &["fan1"], 500.0, 10_000.0);
        alert.shutdown_on_activation = true;
        controller
            .alerts
            .borrow_mut()
            .insert(alert.uid.clone(), alert);

        // Fire, then suppress via a sweep (cancels the shutdown).
        assert!(controller.process_and_collect_alerts_to_fire().is_empty());
        assert_eq!(controller.process_and_collect_alerts_to_fire().len(), 1);
        let _token = registry.register((device_uid.clone(), "fan1".to_string()));
        assert_eq!(controller.process_and_collect_alerts_to_fire().len(), 1);

        // Sweep ends; the fan is still failing.
        registry.clear(&(device_uid, "fan1".to_string()));
        // Tick 1 after the sweep: Inactive -> WarmUp, silent.
        assert!(controller.process_and_collect_alerts_to_fire().is_empty());
        // Tick 2: WarmUp -> Active, a fresh Triggered with a new shutdown.
        let events = controller.process_and_collect_alerts_to_fire();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0].kind, AlertEventKind::Triggered));
        assert!(events[0].fire_shutdown);
        assert!(events[0].notify_desktop);
    }
}
