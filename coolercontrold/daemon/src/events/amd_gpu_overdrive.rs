// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The AMD GPU overdrive setup state, served by `GET /amd-gpu-overdrive` and pushed as the
//! `amd-gpu-overdrive` SSE event whenever it changes.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

/// Setup runs once per user action, so a small buffer covers every transition.
const CHANNEL_CAPACITY: usize = 8;

/// One state for the whole system: `ppfeaturemask` is a module-wide parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OverdriveState {
    NotConfigured,
    /// The boot image is rebuilding, or the kernel argument is being set.
    Applying,
    /// Configured but not yet booted with. A reboot applies it.
    RebootRequired,
    Failed,
    Active,
}

/// What a setup run does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OverdriveAction {
    Enable,
    Disable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AmdGpuOverdriveStatus {
    pub state: OverdriveState,
    /// The tool that applies the setting, such as `limine-mkinitcpio` or `rpm-ostree`.
    pub tool: Option<String>,
    /// What to run as root when the rebuild failed or was skipped.
    pub manual_command: Option<String>,
    /// The failure line, or the last lines of output, when `failed`.
    pub error: Option<String>,
    /// No rebuild ran, so the boot image does not reflect the change: the tooling was not
    /// recognized, or the user chose to only write the config.
    pub rebuild_skipped: bool,
    /// What the current or last run since the daemon started does, absent before any. After a
    /// disable, `reboot_required` means a reboot turns overdrive off.
    pub action: Option<OverdriveAction>,
    /// The modprobe conf the daemon writes is in place, so a disable can remove it.
    pub removable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OverdriveRoute {
    /// Writes or removes the module option, then rebuilds the boot image with `command`.
    Rebuild,
    /// Sets a kernel argument with `command`.
    KernelArgument,
    /// Writes or removes the module option only: no known tool rebuilds the boot image here.
    ConfigOnly,
}

/// What enabling or disabling would do on this system, for the user to confirm first. Served by
/// `GET /amd-gpu-overdrive/plan`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AmdGpuOverdrivePlan {
    /// Absent when `problem` says setup cannot run.
    pub route: Option<OverdriveRoute>,
    pub tool: Option<String>,
    /// The command the daemon runs as root.
    pub command: Option<String>,
    /// The files the daemon writes, or removes when disabling.
    pub files: Vec<String>,
    /// Why setup cannot run now, and what to do instead.
    pub problem: Option<String>,
}

/// Broadcast-only handle for publishing the setup state to SSE subscribers, like
/// `SystemEventHandle`.
#[derive(Clone)]
pub struct AmdGpuOverdriveHandle {
    broadcaster: broadcast::Sender<AmdGpuOverdriveStatus>,
    cancel_token: CancellationToken,
}

impl AmdGpuOverdriveHandle {
    pub fn new(cancel_token: CancellationToken) -> Self {
        let (broadcaster, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            broadcaster,
            cancel_token,
        }
    }

    /// No-op if there are no active listeners.
    pub fn broadcast(&self, status: AmdGpuOverdriveStatus) {
        if self.broadcaster.receiver_count() > 0 {
            let _ = self.broadcaster.send(status);
        }
    }

    pub fn broadcaster(&self) -> &broadcast::Sender<AmdGpuOverdriveStatus> {
        &self.broadcaster
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verify the state names clients match on are stable snake_case strings.
    #[test]
    fn states_serialize_to_snake_case() {
        let names: Vec<String> = [
            OverdriveState::NotConfigured,
            OverdriveState::Applying,
            OverdriveState::RebootRequired,
            OverdriveState::Failed,
            OverdriveState::Active,
        ]
        .iter()
        .map(|state| serde_json::to_string(state).unwrap())
        .collect();
        assert_eq!(
            names,
            [
                "\"not_configured\"",
                "\"applying\"",
                "\"reboot_required\"",
                "\"failed\"",
                "\"active\""
            ]
        );
    }

    // Verify a status reaches a subscriber, and that broadcasting with none is harmless.
    #[test]
    fn a_broadcast_reaches_subscribers() {
        let handle = AmdGpuOverdriveHandle::new(CancellationToken::new());
        let status = AmdGpuOverdriveStatus {
            state: OverdriveState::Applying,
            tool: Some("mkinitcpio".to_owned()),
            manual_command: None,
            error: None,
            rebuild_skipped: false,
            action: Some(OverdriveAction::Enable),
            removable: false,
        };
        handle.broadcast(status.clone());
        let mut receiver = handle.broadcaster().subscribe();
        handle.broadcast(status.clone());
        assert_eq!(receiver.try_recv().unwrap(), status);
    }
}
