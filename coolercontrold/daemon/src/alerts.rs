/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon, Eren Simsek and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use crate::api::actor::AlertHandle;
use crate::api::CCError;
use crate::device::UID;
use crate::notifier::NotificationIcon;
use crate::paths;
use crate::repositories::utils::{sanitize_for_shell, ShellCommand, ShellCommandResult};
use crate::setting::{ChannelMetric, ChannelSource};
use crate::{cc_fs, AllDevices};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Local};
use indexmap::IndexMap;
use log::{error, info, trace, warn};
use moro_local::Scope;
use schemars::JsonSchema;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fmt::{self, Display};
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};
use strum::{Display, EnumString};
use tokio_util::sync::CancellationToken;

const LOG_BUFFER_SIZE: usize = 1000;

/// Minimum interval between consecutive alert-log disk flushes to avoid
/// excessive I/O when alerts are firing rapidly. The very first state change
/// after a quiet period is always flushed immediately.
const LOG_FLUSH_COOLDOWN: Duration = Duration::from_secs(5);
const COMMAND_SHUTDOWN: &str =
    "shutdown +1 \"Critical CoolerControl Alert! System will shutdown in 1 minute.\"";
const COMMAND_SHUTDOWN_CANCEL: &str = "shutdown -c";

pub type AlertName = String;
pub type AlertLogMessage = String;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[allow(clippy::struct_excessive_bools)]
pub struct Alert {
    pub uid: UID,
    pub name: AlertName,
    pub channel_source: ChannelSource,
    pub min: f64,
    pub max: f64,
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

    /// Toggle a desktop notification when this alert enters an `Active` state. (enabled by default)
    #[serde(default = "default_desktop_notify")]
    pub desktop_notify: bool,

    /// Toggle a desktop notification when this alert enters an `Inactive` state. (enabled by default)
    #[serde(default = "default_desktop_notify")]
    pub desktop_notify_recovery: bool,

    /// Toggle whether the desktop notification attempts to play an audio sound
    /// when this alert enters an `Active` state.
    /// Note: only applies when `desktop_notify` is enabled.
    #[serde(default)]
    pub desktop_notify_audio: bool,

    /// Toggle whether to issue a system shutdown when this Alert enters an `Active` state.
    #[serde(default)]
    pub shutdown_on_activation: bool,
}

fn default_desktop_notify() -> bool {
    true
}

impl Alert {
    /// Updates the state based on [`value`] and returns the old state if it changed.
    fn set_state(&mut self, value: f64) -> Option<AlertState> {
        let current = self.state;

        if value >= self.min && value <= self.max {
            self.state = AlertState::Inactive;
        } else {
            // We know we're out of bounds here.
            match self.state {
                AlertState::Active => {}
                AlertState::WarmUp(time) => {
                    if Local::now().signed_duration_since(time).as_seconds_f64()
                        >= self.warmup_duration
                    {
                        self.state = AlertState::Active;
                    }
                }
                // Error state means we could not retrieve the channel value. But if we're here with a
                // channel value it means the errors were resolved e.g. by a daemon restart. Act as
                // usual.
                AlertState::Error | AlertState::Inactive => {
                    self.state = AlertState::WarmUp(Local::now());
                }
            }
        }

        (self.state != current).then_some(current)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Display, EnumString, JsonSchema)]
pub enum AlertState {
    Active,

    /// Alert condition was satisfied at the stored time
    /// but the duration threshold has not been reached.
    WarmUp(DateTime<Local>),

    Inactive,

    /// Represents an error state. e.g. when one of the components in the alert isn't found.
    Error,
}

impl Serialize for AlertState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match *self {
            AlertState::Active => serializer.serialize_str("Active"),
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
                    "Active" => Ok(AlertState::Active),
                    "Error" => Ok(AlertState::Error),
                    "WarmUp" | "Inactive" => Ok(AlertState::Inactive),
                    _ => Err(E::custom(format!("unknown variant: {value}"))),
                }
            }
        }

        deserializer.deserialize_str(AlertStateVisitor)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AlertLog {
    pub uid: UID,
    pub name: AlertName,
    pub state: AlertState,
    pub message: AlertLogMessage,
    pub timestamp: DateTime<Local>,
}

impl Default for AlertLog {
    fn default() -> Self {
        AlertLog {
            uid: "Unknown".to_string(),
            name: "Unknown".to_string(),
            state: AlertState::Active,
            message: "Unknown".to_string(),
            timestamp: Local::now(),
        }
    }
}

pub struct AlertController {
    all_devices: AllDevices,
    alerts: RefCell<IndexMap<UID, Alert>>,
    alert_handle: RefCell<Option<AlertHandle>>,
    logs: RefCell<VecDeque<AlertLog>>,
    logs_dirty: Cell<bool>,
    last_log_flush: Cell<Instant>,
    bin_path: String,
}

impl AlertController {
    /// A controller for managing and handling Alerts.
    pub async fn init(all_devices: AllDevices, bin_path: String) -> Result<Self> {
        let alert_controller = Self {
            all_devices,
            alerts: RefCell::new(IndexMap::new()),
            alert_handle: RefCell::new(None),
            logs: RefCell::new(VecDeque::with_capacity(LOG_BUFFER_SIZE)),
            logs_dirty: Cell::new(false),
            // Set to a past time so the first state change always flushes immediately.
            #[allow(clippy::unchecked_time_subtraction)]
            last_log_flush: Cell::new(Instant::now() - LOG_FLUSH_COOLDOWN),
            bin_path,
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
                alerts: Vec::with_capacity(0),
                logs: Vec::with_capacity(0),
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

    /// Resets the saved state of an alert to Inactive.
    /// We want to re-evaluate the state of all Alerts on startup, so we reset the saved state to Inactive.
    /// Note that we are still serializing the states properly for the Alert Logs.
    fn reset_saved_alert_state(alert: &mut Alert) {
        alert.state = AlertState::Inactive;
    }

    /// Saves alert configuration (thresholds, settings) to `/etc/coolercontrol/alerts.json`.
    /// Logs are intentionally excluded; use `save_alert_logs` for those.
    async fn save_alert_data_to_config(&self) -> Result<()> {
        let alert_config = AlertConfigFile {
            alerts: self.alerts.borrow().values().cloned().collect(),
            logs: Vec::with_capacity(0),
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
    pub async fn create(&self, alert: Alert) -> Result<()> {
        if self.alerts.borrow().contains_key(&alert.uid) {
            return Err(CCError::UserError {
                msg: format!("Alert with uid {} already exists", alert.uid),
            }
            .into());
        }
        self.alerts.borrow_mut().insert(alert.uid.clone(), alert);
        self.save_alert_data_to_config().await
    }

    /// Updates an existing Alert
    pub async fn update(&self, mut alert: Alert) -> Result<()> {
        {
            let mut alerts_lock = self.alerts.borrow_mut();
            let Some(existing_alert) = alerts_lock.get(&alert.uid) else {
                return Err(CCError::NotFound {
                    msg: format!("Alert with uid {} does not exist", alert.uid),
                }
                .into());
            };
            // don't overwrite state:
            alert.state = existing_alert.state;
            alerts_lock.insert(alert.uid.clone(), alert);
        }
        self.save_alert_data_to_config().await
    }

    /// Deletes an existing Alert
    pub async fn delete(&self, alert_uid: UID) -> Result<()> {
        if self.alerts.borrow().contains_key(&alert_uid).not() {
            return Err(CCError::NotFound {
                msg: format!("Alert with uid {alert_uid} does not exist"),
            }
            .into());
        }
        self.alerts.borrow_mut().shift_remove(&alert_uid);
        self.save_alert_data_to_config().await
    }

    /// Processes all Alerts, firing off messages if an alert state has changed.
    /// This function should be called in the main loop.
    pub fn process_alerts(&self) {
        let alerts_to_fire = self.process_and_collect_alerts_to_fire();
        for (alert, message) in alerts_to_fire {
            self.send_notifications(&alert, &message);
            let log = self.log_alert_state_change(alert.uid, alert.name, alert.state, message);
            if let Some(handle) = self.alert_handle.borrow().as_ref() {
                handle.broadcast_alert_state_change(log);
            }
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
        tokio::task::spawn_local({
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

    /// Collects all Alerts that need firing
    #[allow(clippy::too_many_lines)]
    fn process_and_collect_alerts_to_fire(&self) -> Vec<(Alert, AlertLogMessage)> {
        let mut alerts_to_fire = Vec::new();
        for alert in self.alerts.borrow_mut().values_mut() {
            let Some(device) = self.all_devices.get(&alert.channel_source.device_uid) else {
                Self::activate_alert_with_error(&mut alerts_to_fire, alert, "Device not found");
                continue;
            };
            let Some(most_recent_status) = device.borrow().status_current() else {
                Self::activate_alert_with_error(
                    &mut alerts_to_fire,
                    alert,
                    "Device has no current status",
                );
                continue;
            };
            let channel_value = if alert.channel_source.channel_metric == ChannelMetric::Temp {
                let Some(temp_status) = most_recent_status
                    .temps
                    .iter()
                    .find(|temp| temp.name == alert.channel_source.channel_name)
                else {
                    Self::activate_alert_with_error(
                        &mut alerts_to_fire,
                        alert,
                        "Device Channel not found",
                    );
                    continue;
                };
                temp_status.temp
            } else {
                let Some(channel_status) = most_recent_status
                    .channels
                    .iter()
                    .find(|channel| channel.name == alert.channel_source.channel_name)
                else {
                    Self::activate_alert_with_error(
                        &mut alerts_to_fire,
                        alert,
                        "Device Channel not found",
                    );
                    continue;
                };
                match alert.channel_source.channel_metric {
                    ChannelMetric::Duty => {
                        let Some(duty) = channel_status.duty else {
                            Self::activate_alert_with_error(
                                &mut alerts_to_fire,
                                alert,
                                "Device Channel Duty Metric not found",
                            );
                            continue;
                        };
                        duty
                    }
                    ChannelMetric::Load => {
                        let Some(load) = channel_status.duty else {
                            Self::activate_alert_with_error(
                                &mut alerts_to_fire,
                                alert,
                                "Device Channel Load Metric not found",
                            );
                            continue;
                        };
                        load
                    }
                    ChannelMetric::RPM => {
                        let Some(rpm) = channel_status.rpm else {
                            Self::activate_alert_with_error(
                                &mut alerts_to_fire,
                                alert,
                                "Device Channel RPM Metric not found",
                            );
                            continue;
                        };
                        f64::from(rpm)
                    }
                    ChannelMetric::Freq => {
                        let Some(freq) = channel_status.freq else {
                            Self::activate_alert_with_error(
                                &mut alerts_to_fire,
                                alert,
                                "Device Channel Freq Metric not found",
                            );
                            continue;
                        };
                        f64::from(freq)
                    }
                    ChannelMetric::Temp => {
                        error!(
                            "This should not happen, ChannelMetric::TEMP should already be handled."
                        );
                        continue;
                    }
                }
            };

            // No message if the state didn't change
            let Some(old_state) = alert.set_state(channel_value) else {
                continue;
            };

            // All transitions except these two send a message:
            // - any state -> warmup
            // - warmup -> inactive
            if matches!(
                (old_state, alert.state),
                (_, AlertState::WarmUp(_)) | (AlertState::WarmUp(_), AlertState::Inactive)
            ) {
                continue;
            }

            let channel_name = alert.channel_source.channel_name.clone();
            let min = alert.min;
            let max = alert.max;

            let message = if channel_value > alert.max {
                // round up to clearly display greater than.
                let channel_value_rounded = (channel_value * 10.).ceil() / 10.;
                format!(
                    "{channel_name}: {channel_value_rounded} is greater than allowed maximum: {max}"
                )
            } else if channel_value < alert.min {
                // round down to clearly display less than.
                let channel_value_rounded = (channel_value * 10.).floor() / 10.;
                format!(
                    "{channel_name}: {channel_value_rounded} is less than allowed minimum: {min}"
                )
            } else {
                let channel_value_rounded = (channel_value * 10.).round() / 10.;
                format!(
                    "{channel_name}: {channel_value_rounded} is again within allowed range: {min} - {max}"
                )
            };

            alerts_to_fire.push((alert.clone(), message.clone()));
        }
        alerts_to_fire
    }

    /// Adds an Alert to the list of alerts to fire with error state, if state has changed.
    fn activate_alert_with_error(
        alerts_to_fire: &mut Vec<(Alert, AlertLogMessage)>,
        alert: &mut Alert,
        message: impl Display,
    ) {
        if alert.state == AlertState::Error {
            return; // only fire on state change
        }

        alert.state = AlertState::Error;
        alerts_to_fire.push((alert.clone(), message.to_string()));
    }

    /// Logs an alert state change to the internal buffer, as well as returning the newly
    /// created log entry.
    pub fn log_alert_state_change(
        &self,
        uid: UID,
        name: AlertName,
        state: AlertState,
        message: AlertLogMessage,
    ) -> AlertLog {
        let log = AlertLog {
            uid,
            name,
            state,
            message,
            timestamp: Local::now(),
        };
        let mut logs_lock = self.logs.borrow_mut();
        while logs_lock.len() >= LOG_BUFFER_SIZE {
            logs_lock.pop_front();
        }
        logs_lock.push_back(log.clone());
        self.logs_dirty.set(true);
        log
    }

    /// Handle all notifications and system shutdowns for an alert.
    fn send_notifications(&self, alert: &Alert, message: &str) {
        let sessions = Self::available_session_users();
        match alert.state {
            AlertState::Active => {
                if alert.desktop_notify {
                    for (uid, runtime_dir) in &sessions {
                        if alert.shutdown_on_activation {
                            let title = format!("Shutdown Alert Triggered: {}!", alert.name);
                            let body = format!("Shutdown will commence in 1 Minute.\n{message}");
                            self.fire_notification(
                                *uid,
                                runtime_dir,
                                &title,
                                &body,
                                NotificationIcon::Shutdown,
                                alert.desktop_notify_audio,
                                Some(2),
                            );
                        } else {
                            let title = format!("Alert Triggered: {}!", alert.name);
                            self.fire_notification(
                                *uid,
                                runtime_dir,
                                &title,
                                message,
                                NotificationIcon::Triggered,
                                alert.desktop_notify_audio,
                                None,
                            );
                        }
                    }
                }
                if alert.shutdown_on_activation {
                    Self::fire_command(COMMAND_SHUTDOWN);
                    info!(
                        "Shutdown Alert Triggered: {} - Shutdown will commence in 1 Minute",
                        alert.name
                    );
                }
            }
            AlertState::Inactive => {
                if alert.shutdown_on_activation {
                    Self::fire_command(COMMAND_SHUTDOWN_CANCEL);
                    info!(
                        "Shutdown Alert Resolved: {} - Shutdown cancelled",
                        alert.name
                    );
                }
                if alert.desktop_notify && alert.desktop_notify_recovery {
                    for (uid, runtime_dir) in &sessions {
                        let title = format!("Alert Resolved: {}", alert.name);
                        self.fire_notification(
                            *uid,
                            runtime_dir,
                            &title,
                            message,
                            NotificationIcon::Resolved,
                            false,
                            None,
                        );
                    }
                }
            }
            AlertState::Error => {
                if alert.desktop_notify {
                    for (uid, runtime_dir) in &sessions {
                        let title = format!("Alert Error: {}", alert.name);
                        self.fire_notification(
                            *uid,
                            runtime_dir,
                            &title,
                            message,
                            NotificationIcon::Error,
                            alert.desktop_notify_audio,
                            None,
                        );
                    }
                }
                if alert.desktop_notify || alert.shutdown_on_activation {
                    warn!("Alert in Error State: {} = {}", alert.name, message);
                }
            }
            AlertState::WarmUp(_) => {} // Warmup state is never fired.
        }
    }

    /// Fires a desktop notification with automatic sanitization of title and message.
    /// Sets DBUS_SESSION_BUS_ADDRESS and XDG_RUNTIME_DIR so zbus can find
    /// the user's session bus (sudo strips the environment).
    fn fire_notification(
        &self,
        uid: u32,
        runtime_dir: &Path,
        title: &str,
        message: &str,
        icon: NotificationIcon,
        audio: bool,
        urgency_lvl: Option<u8>,
    ) {
        let safe_title = sanitize_for_shell(title);
        let safe_message = sanitize_for_shell(message);
        let runtime = runtime_dir.display();
        let env_prefix = format!(
            "sudo -u \\#{uid} env \
             DBUS_SESSION_BUS_ADDRESS=unix:path={runtime}/bus \
             XDG_RUNTIME_DIR={runtime}"
        );
        let cmd = match urgency_lvl {
            Some(urgency) => format!(
                "{env_prefix} {} notify \"{}\" \"{}\" {} {} {}",
                self.bin_path, safe_title, safe_message, icon as u8, audio, urgency
            ),
            None => format!(
                "{env_prefix} {} notify \"{}\" \"{}\" {} {}",
                self.bin_path, safe_title, safe_message, icon as u8, audio
            ),
        };
        Self::fire_command(&cmd);
    }

    fn fire_command(cmd: &str) {
        let cmd = cmd.to_string();
        tokio::task::spawn_local(async move {
            if let ShellCommandResult::Error(err) =
                ShellCommand::new(&cmd, Duration::from_secs(20)).run().await
            {
                if log::log_enabled!(log::Level::Debug) {
                    warn!("Failed to execute notification command: '{cmd}' - {err}");
                }
            }
        });
    }

    fn available_session_users() -> Vec<(u32, PathBuf)> {
        let mut sessions = Vec::new();
        // Search for /run/user/*/bus for user IDs with open dbus sessions.
        let mut path = PathBuf::from("/run/user");
        if path.exists().not() {
            path = PathBuf::from("/var/run/user");
        }
        let Ok(entries) = cc_fs::read_dir(path) else {
            return sessions;
        };
        for entry in entries.flatten() {
            let user_dir = entry.path();
            if user_dir.join("bus").exists() {
                if let Some(uid) = user_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|id| id.parse::<u32>().ok())
                {
                    // do not notify root or system users
                    if uid >= 1000 {
                        sessions.push((uid, user_dir));
                    }
                }
            }
        }
        sessions
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    /// Helper to create a minimal test alert with given uid, min, max, and state.
    fn make_alert(uid: &str, min: f64, max: f64, state: AlertState) -> Alert {
        assert!(min <= max, "min must be <= max for a valid alert range.");
        Alert {
            uid: uid.to_string(),
            name: format!("Alert-{uid}"),
            channel_source: ChannelSource {
                device_uid: "dev1".to_string(),
                channel_name: "temp1".to_string(),
                channel_metric: ChannelMetric::Temp,
            },
            min,
            max,
            state,
            warmup_duration: 0.0,
            desktop_notify: true,
            desktop_notify_recovery: true,
            desktop_notify_audio: false,
            shutdown_on_activation: false,
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

    // -- Alert::set_state tests (core state machine) --

    #[test]
    fn set_state_value_in_range_stays_inactive() {
        // Goal: verify that a value within [min, max] keeps state Inactive.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Inactive);
        let changed = alert.set_state(50.0);
        assert!(changed.is_none(), "State should not change.");
        assert_eq!(alert.state, AlertState::Inactive);
    }

    #[test]
    fn set_state_value_at_min_boundary_stays_inactive() {
        // Goal: verify that value exactly at min is within range.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Inactive);
        let changed = alert.set_state(20.0);
        assert!(changed.is_none());
        assert_eq!(alert.state, AlertState::Inactive);
    }

    #[test]
    fn set_state_value_at_max_boundary_stays_inactive() {
        // Goal: verify that value exactly at max is within range.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Inactive);
        let changed = alert.set_state(80.0);
        assert!(changed.is_none());
        assert_eq!(alert.state, AlertState::Inactive);
    }

    #[test]
    fn set_state_value_above_max_transitions_inactive_to_warmup() {
        // Goal: verify out-of-range value from Inactive enters WarmUp.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Inactive);
        let old = alert.set_state(81.0);
        assert_eq!(old, Some(AlertState::Inactive));
        assert!(matches!(alert.state, AlertState::WarmUp(_)));
    }

    #[test]
    fn set_state_value_below_min_transitions_inactive_to_warmup() {
        // Goal: verify out-of-range below min from Inactive enters WarmUp.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Inactive);
        let old = alert.set_state(19.9);
        assert_eq!(old, Some(AlertState::Inactive));
        assert!(matches!(alert.state, AlertState::WarmUp(_)));
    }

    #[test]
    fn set_state_warmup_to_active_after_duration() {
        // Goal: verify that WarmUp transitions to Active once the warmup
        // duration has elapsed.
        let past = Local::now() - Duration::seconds(2);
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::WarmUp(past));
        alert.warmup_duration = 1.0;
        let old = alert.set_state(90.0);
        assert!(matches!(old, Some(AlertState::WarmUp(_))));
        assert_eq!(alert.state, AlertState::Active);
    }

    #[test]
    fn set_state_warmup_stays_warmup_before_duration() {
        // Goal: verify that WarmUp does NOT transition to Active before
        // the warmup duration elapses.
        let now = Local::now();
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::WarmUp(now));
        alert.warmup_duration = 9999.0; // far in the future
        let changed = alert.set_state(90.0);
        assert!(changed.is_none(), "Should stay in WarmUp.");
        assert!(matches!(alert.state, AlertState::WarmUp(_)));
    }

    #[test]
    fn set_state_warmup_returns_to_inactive_when_value_in_range() {
        // Goal: verify WarmUp -> Inactive when value comes back in range.
        let past = Local::now() - Duration::seconds(1);
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::WarmUp(past));
        let old = alert.set_state(50.0);
        assert!(matches!(old, Some(AlertState::WarmUp(_))));
        assert_eq!(alert.state, AlertState::Inactive);
    }

    #[test]
    fn set_state_active_stays_active_when_still_out_of_range() {
        // Goal: verify Active stays Active while value remains out of range.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Active);
        let changed = alert.set_state(90.0);
        assert!(changed.is_none());
        assert_eq!(alert.state, AlertState::Active);
    }

    #[test]
    fn set_state_active_returns_to_inactive_when_value_in_range() {
        // Goal: verify Active -> Inactive when value returns to range.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Active);
        let old = alert.set_state(50.0);
        assert_eq!(old, Some(AlertState::Active));
        assert_eq!(alert.state, AlertState::Inactive);
    }

    #[test]
    fn set_state_error_transitions_to_warmup_when_out_of_range() {
        // Goal: verify Error -> WarmUp when a value arrives but is out of range.
        // Error means the channel was missing; receiving a value means
        // the error resolved but we still need to warm up.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Error);
        let old = alert.set_state(90.0);
        assert_eq!(old, Some(AlertState::Error));
        assert!(matches!(alert.state, AlertState::WarmUp(_)));
    }

    #[test]
    fn set_state_error_transitions_to_inactive_when_in_range() {
        // Goal: verify Error -> Inactive when value is in range.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Error);
        let old = alert.set_state(50.0);
        assert_eq!(old, Some(AlertState::Error));
        assert_eq!(alert.state, AlertState::Inactive);
    }

    #[test]
    fn set_state_zero_warmup_immediately_activates() {
        // Goal: verify that warmup_duration=0 means the alert goes
        // Inactive -> WarmUp -> Active in two consecutive out-of-range calls.
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Inactive);
        alert.warmup_duration = 0.0;

        // First call: Inactive -> WarmUp.
        alert.set_state(90.0);
        assert!(matches!(alert.state, AlertState::WarmUp(_)));

        // Second call: WarmUp -> Active (0s duration already elapsed).
        alert.set_state(90.0);
        assert_eq!(alert.state, AlertState::Active);
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
            !json.contains("\"logs\""),
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
        assert!(alert.desktop_notify);
        assert!(alert.desktop_notify_recovery);
        assert!(!alert.desktop_notify_audio);
        assert!(!alert.shutdown_on_activation);
    }

    // -- activate_alert_with_error tests --

    #[test]
    fn activate_alert_with_error_changes_state_once() {
        // Goal: verify the error activation only fires on state change,
        // not repeatedly for the same error condition.
        let mut alerts_to_fire = Vec::new();
        let mut alert = make_alert("a", 20.0, 80.0, AlertState::Inactive);

        AlertController::activate_alert_with_error(
            &mut alerts_to_fire,
            &mut alert,
            "Device not found",
        );
        assert_eq!(alert.state, AlertState::Error);
        assert_eq!(alerts_to_fire.len(), 1);
        assert_eq!(alerts_to_fire[0].1, "Device not found");

        // Second call with Error state should not fire again.
        AlertController::activate_alert_with_error(
            &mut alerts_to_fire,
            &mut alert,
            "Device not found",
        );
        assert_eq!(alerts_to_fire.len(), 1, "Should not fire twice.");
    }
}
