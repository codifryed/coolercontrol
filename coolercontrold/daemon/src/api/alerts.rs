// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::alerts::{
    Alert, AlertLog, AlertName, AlertState, MAX_ALERT_SOURCES, MIN_REPEAT_INTERVAL_SECONDS,
};
use crate::api::Json;
use crate::api::{handle_error, validate_name_string, AppState, CCError};
use crate::device::UID;
use crate::setting::ChannelSource;
use axum::extract::{Path, State};
use chrono::{DateTime, Local};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::ops::Not;

/// Retrieves the persisted Alert list
pub async fn get_all(
    State(AppState { alert_handle, .. }): State<AppState>,
) -> Result<Json<AlertsDto>, CCError> {
    alert_handle
        .get_all()
        .await
        .map(|all| {
            Json(AlertsDto {
                alerts: all.0.into_iter().map(AlertDto::from).collect(),
                logs: all.1,
            })
        })
        .map_err(handle_error)
}

pub async fn create(
    State(AppState { alert_handle, .. }): State<AppState>,
    Json(alert_dto): Json<AlertDto>,
) -> Result<(), CCError> {
    validate_alert(&alert_dto)?;
    alert_handle
        .create(Alert::from(alert_dto))
        .await
        .map_err(handle_error)
}

pub async fn update(
    State(AppState { alert_handle, .. }): State<AppState>,
    Json(alert_dto): Json<AlertDto>,
) -> Result<(), CCError> {
    validate_alert(&alert_dto)?;
    alert_handle
        .update(Alert::from(alert_dto))
        .await
        .map_err(handle_error)
}

pub async fn delete(
    Path(path): Path<AlertPath>,
    State(AppState { alert_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    alert_handle
        .delete(path.alert_uid)
        .await
        .map_err(handle_error)
}

fn validate_alert(alert: &AlertDto) -> Result<(), CCError> {
    validate_name_string(&alert.name)?;
    if alert.uid.is_empty() {
        return Err(CCError::UserError {
            msg: "uid cannot be empty".to_string(),
        });
    }
    validate_sources(alert)?;
    validate_thresholds(alert)
}

/// The effective source list: `channel_sources` when present, otherwise the
/// legacy single `channel_source`.
fn effective_sources(alert: &AlertDto) -> Vec<ChannelSource> {
    if alert.channel_sources.is_empty().not() {
        return alert.channel_sources.clone();
    }
    alert
        .channel_source
        .clone()
        .map_or_else(Vec::new, |source| vec![source])
}

fn validate_sources(alert: &AlertDto) -> Result<(), CCError> {
    let sources = effective_sources(alert);
    if sources.is_empty() {
        return Err(CCError::UserError {
            msg: "at least one channel source is required".to_string(),
        });
    }
    if sources.len() > MAX_ALERT_SOURCES {
        return Err(CCError::UserError {
            msg: format!("at most {MAX_ALERT_SOURCES} channel sources are allowed"),
        });
    }
    let first_metric = &sources[0].channel_metric;
    for source in &sources {
        if source.device_uid.is_empty() {
            return Err(CCError::UserError {
                msg: "channel_source.device_uid cannot be empty".to_string(),
            });
        }
        if source.channel_name.is_empty() {
            return Err(CCError::UserError {
                msg: "channel_source.channel_name cannot be empty".to_string(),
            });
        }
        if source.channel_metric != *first_metric {
            return Err(CCError::UserError {
                msg: "all channel sources must share the same metric".to_string(),
            });
        }
    }
    Ok(())
}

#[allow(clippy::float_cmp)]
fn validate_thresholds(alert: &AlertDto) -> Result<(), CCError> {
    if alert.max < alert.min {
        return Err(CCError::UserError {
            msg: "max must be greater than min".to_string(),
        });
    }
    if alert.max == alert.min {
        return Err(CCError::UserError {
            msg: "max and min cannot be equal".to_string(),
        });
    }
    if alert.max < 0.0 {
        return Err(CCError::UserError {
            msg: "max cannot be negative".to_string(),
        });
    }
    if alert.min < 0.0 {
        return Err(CCError::UserError {
            msg: "min cannot be negative".to_string(),
        });
    }
    if alert.warmup_duration.is_sign_negative() {
        return Err(CCError::UserError {
            msg: "warmup_duration cannot be negative".to_string(),
        });
    }
    if alert.cooldown_duration.is_sign_negative() {
        return Err(CCError::UserError {
            msg: "cooldown_duration cannot be negative".to_string(),
        });
    }
    if alert.repeat_interval.is_sign_negative() {
        return Err(CCError::UserError {
            msg: "repeat_interval cannot be negative".to_string(),
        });
    }
    if alert.repeat_interval > 0.0 && alert.repeat_interval < MIN_REPEAT_INTERVAL_SECONDS {
        return Err(CCError::UserError {
            msg: format!(
                "repeat_interval must be at least {MIN_REPEAT_INTERVAL_SECONDS} seconds, or 0 to disable"
            ),
        });
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AlertPath {
    alert_uid: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AlertsDto {
    alerts: Vec<AlertDto>,
    logs: Vec<AlertLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[allow(clippy::struct_excessive_bools)]
pub struct AlertDto {
    pub uid: UID,
    pub name: AlertName,

    /// Legacy single source; still accepted on input when `channel_sources`
    /// is empty and always populated on output for older API clients.
    #[serde(default)]
    pub channel_source: Option<ChannelSource>,

    /// All watched sources; entries must share one `ChannelMetric`.
    #[serde(default)]
    pub channel_sources: Vec<ChannelSource>,

    pub min: f64,
    pub max: f64,

    // We send the current state, but we don't receive it when creating or updating:
    #[serde(default)]
    pub state: Option<AlertState>,

    /// Per-source states, parallel to `channel_sources`. Output-only.
    #[serde(default)]
    pub source_states: Vec<SourceStateDto>,

    /// Time in seconds throughout which the alert condition must hold before the alert is activated
    pub warmup_duration: f64,

    /// Time in seconds the value must stay back in range before an Active alert clears.
    #[serde(default)]
    pub cooldown_duration: f64,

    /// Seconds between repeated notifications while Active; 0 disables, minimum 60 otherwise.
    #[serde(default)]
    pub repeat_interval: f64,

    /// A disabled alert is not evaluated at all.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// While set to a future time, notifications and shutdown are suppressed.
    #[serde(default)]
    pub silenced_until: Option<DateTime<Local>>,

    /// Toggle a desktop notification when this alert enters an `Active` state. (true by default)
    #[serde(default = "default_true")]
    pub desktop_notify: bool,

    /// Toggle a desktop notification when this alert enters an `Inactive` state.
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
}

/// The wire-visible state of one watched source.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceStateDto {
    pub channel_source: ChannelSource,
    pub state: AlertState,
}

impl From<Alert> for AlertDto {
    fn from(alert: Alert) -> Self {
        let source_states = alert
            .channel_sources
            .iter()
            .zip(alert.source_states.iter())
            .map(|(channel_source, state)| SourceStateDto {
                channel_source: channel_source.clone(),
                state: *state,
            })
            .collect();
        AlertDto {
            uid: alert.uid,
            name: alert.name,
            channel_source: Some(alert.channel_source),
            channel_sources: alert.channel_sources,
            min: alert.min,
            max: alert.max,
            state: Some(alert.state),
            source_states,
            warmup_duration: alert.warmup_duration,
            cooldown_duration: alert.cooldown_duration,
            repeat_interval: alert.repeat_interval,
            enabled: alert.enabled,
            silenced_until: alert.silenced_until,
            desktop_notify: alert.desktop_notify,
            desktop_notify_recovery: alert.desktop_notify_recovery,
            desktop_notify_audio: alert.desktop_notify_audio,
            shutdown_on_activation: alert.shutdown_on_activation,
        }
    }
}

impl From<AlertDto> for Alert {
    fn from(alert_dto: AlertDto) -> Self {
        let channel_sources = effective_sources(&alert_dto);
        // Guaranteed by validate_alert at the API boundary.
        assert!(channel_sources.is_empty().not());
        let source_count = channel_sources.len();
        Alert {
            uid: alert_dto.uid,
            name: alert_dto.name,
            channel_source: channel_sources[0].clone(),
            channel_sources,
            min: alert_dto.min,
            max: alert_dto.max,
            state: AlertState::Inactive,
            warmup_duration: alert_dto.warmup_duration,
            cooldown_duration: alert_dto.cooldown_duration,
            repeat_interval: alert_dto.repeat_interval,
            enabled: alert_dto.enabled,
            silenced_until: alert_dto.silenced_until,
            desktop_notify: alert_dto.desktop_notify,
            desktop_notify_recovery: alert_dto.desktop_notify_recovery,
            desktop_notify_audio: alert_dto.desktop_notify_audio,
            shutdown_on_activation: alert_dto.shutdown_on_activation,
            source_states: vec![AlertState::Inactive; source_count],
            last_notified: None,
            notified: false,
            error_notified: false,
            shutdown_scheduled: false,
        }
    }
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::{effective_sources, validate_alert, validate_sources, validate_thresholds};
    use crate::alerts::{MAX_ALERT_SOURCES, MIN_REPEAT_INTERVAL_SECONDS};
    use crate::api::alerts::AlertDto;
    use crate::setting::{ChannelMetric, ChannelSource};

    fn source(channel_name: &str, channel_metric: ChannelMetric) -> ChannelSource {
        ChannelSource {
            device_uid: "dev1".to_string(),
            channel_name: channel_name.to_string(),
            channel_metric,
        }
    }

    /// A DTO that passes every check, so each test can break exactly one thing.
    fn valid_dto() -> AlertDto {
        AlertDto {
            uid: "alert-uid".to_string(),
            name: "Fan Stall".to_string(),
            channel_source: None,
            channel_sources: vec![source("fan1", ChannelMetric::RPM)],
            min: 300.0,
            max: 2000.0,
            state: None,
            source_states: Vec::new(),
            warmup_duration: 0.0,
            cooldown_duration: 0.0,
            repeat_interval: 0.0,
            enabled: true,
            silenced_until: None,
            desktop_notify: true,
            desktop_notify_recovery: true,
            desktop_notify_audio: false,
            shutdown_on_activation: false,
        }
    }

    #[test]
    fn the_valid_baseline_passes() {
        // Goal: pin the fixture itself, so a later failure names the field a test broke
        // rather than an unnoticed defect in the baseline.
        assert!(validate_alert(&valid_dto()).is_ok());
    }

    #[test]
    fn effective_sources_prefers_the_multi_source_list() {
        // Goal: `channel_sources` is authoritative; the legacy single source is only a
        // fallback for an older client that sends nothing else.
        let mut dto = valid_dto();
        dto.channel_source = Some(source("legacy", ChannelMetric::RPM));
        assert_eq!(effective_sources(&dto), dto.channel_sources);

        dto.channel_sources = Vec::new();
        assert_eq!(effective_sources(&dto).len(), 1);
        assert_eq!(effective_sources(&dto)[0].channel_name, "legacy");

        dto.channel_source = None;
        assert!(effective_sources(&dto).is_empty());
    }

    #[test]
    fn an_alert_needs_a_uid_and_a_name() {
        let mut dto = valid_dto();
        dto.uid = String::new();
        assert!(validate_alert(&dto).is_err());

        let mut dto = valid_dto();
        dto.name = String::new();
        assert!(validate_alert(&dto).is_err());
    }

    #[test]
    fn an_alert_needs_at_least_one_source() {
        let mut dto = valid_dto();
        dto.channel_sources = Vec::new();
        dto.channel_source = None;
        assert!(validate_sources(&dto).is_err());
    }

    #[test]
    fn the_source_maximum_is_the_boundary_not_beyond_it() {
        // Goal: cover both sides of the cap, since an off-by-one here either rejects a
        // legitimate alert or lets an unbounded list through to the per-tick walk.
        let mut dto = valid_dto();
        dto.channel_sources = (0..MAX_ALERT_SOURCES)
            .map(|i| source(&format!("fan{i}"), ChannelMetric::RPM))
            .collect();
        assert!(
            validate_sources(&dto).is_ok(),
            "the maximum itself is legal"
        );

        dto.channel_sources
            .push(source("one_too_many", ChannelMetric::RPM));
        assert!(validate_sources(&dto).is_err());
    }

    #[test]
    fn every_source_needs_a_device_and_a_channel() {
        let mut dto = valid_dto();
        dto.channel_sources[0].device_uid = String::new();
        assert!(validate_sources(&dto).is_err());

        let mut dto = valid_dto();
        dto.channel_sources[0].channel_name = String::new();
        assert!(validate_sources(&dto).is_err());
    }

    #[test]
    fn sources_must_share_one_metric() {
        // Goal: a mixed-metric alert has no single meaningful threshold, and the
        // evaluator's Temp arm assumes the rejection happened here.
        let mut dto = valid_dto();
        dto.channel_sources
            .push(source("temp1", ChannelMetric::Temp));
        assert!(validate_sources(&dto).is_err());
    }

    #[test]
    fn thresholds_must_form_a_non_empty_positive_range() {
        for (min, max) in [(2000.0, 300.0), (300.0, 300.0), (-1.0, 100.0)] {
            let mut dto = valid_dto();
            dto.min = min;
            dto.max = max;
            assert!(
                validate_thresholds(&dto).is_err(),
                "min {min} / max {max} must be rejected"
            );
        }
    }

    #[test]
    fn durations_cannot_be_negative() {
        let mut dto = valid_dto();
        dto.warmup_duration = -1.0;
        assert!(validate_thresholds(&dto).is_err());

        let mut dto = valid_dto();
        dto.cooldown_duration = -1.0;
        assert!(validate_thresholds(&dto).is_err());

        let mut dto = valid_dto();
        dto.repeat_interval = -1.0;
        assert!(validate_thresholds(&dto).is_err());
    }

    #[test]
    fn a_repeat_interval_is_zero_or_at_least_the_floor() {
        // Goal: 0 disables repeats, and anything positive must clear the floor, so a
        // typo like 1 cannot notify on every tick.
        let mut dto = valid_dto();
        dto.repeat_interval = 0.0;
        assert!(validate_thresholds(&dto).is_ok());

        dto.repeat_interval = MIN_REPEAT_INTERVAL_SECONDS - 1.0;
        assert!(validate_thresholds(&dto).is_err());

        dto.repeat_interval = MIN_REPEAT_INTERVAL_SECONDS;
        assert!(validate_thresholds(&dto).is_ok());
    }
}
