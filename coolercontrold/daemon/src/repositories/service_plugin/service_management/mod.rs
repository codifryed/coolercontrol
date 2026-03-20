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

use crate::repositories::utils::{DirectCommand, ShellCommandResult};
use anyhow::{anyhow, Result};
use std::env;
use std::path::PathBuf;
use std::time::Duration;

pub mod manager;
mod openrc;
pub mod systemd;

pub type ServiceId = String;

pub trait ServiceIdExt {
    fn to_service_name(&self) -> String;
    fn to_description(&self) -> String;
}

impl ServiceIdExt for ServiceId {
    fn to_service_name(&self) -> String {
        format!("cc-plugin-{self}")
    }

    fn to_description(&self) -> String {
        format!("CoolerControl Plugin {self}")
    }
}

const USER_CMD_TIMEOUT: Duration = Duration::from_secs(5);

/// Deletes the plugin user if it exists.
/// Tries `userdel` first (systemd distros, Gentoo, Artix, Void), then falls back
/// to `deluser` for Alpine Linux (`BusyBox`).
pub async fn delete_plugin_user(username: &str) -> Result<()> {
    let userdel_ok = matches!(
        DirectCommand::new("userdel", USER_CMD_TIMEOUT)
            .arg(username)
            .run()
            .await,
        ShellCommandResult::Success { .. }
    );
    if userdel_ok {
        return Ok(());
    }
    // Fall back to `deluser` for Alpine Linux (BusyBox).
    match DirectCommand::new("deluser", USER_CMD_TIMEOUT)
        .arg(username)
        .run()
        .await
    {
        ShellCommandResult::Success { .. } => Ok(()),
        ShellCommandResult::Error(err) => Err(anyhow!("Failed to delete user {username}: {err}")),
    }
}

fn find_on_path(executable: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths).find_map(|dir| {
            let full_path = dir.join(executable);
            if full_path.is_file() {
                Some(full_path)
            } else {
                None
            }
        })
    })
}
