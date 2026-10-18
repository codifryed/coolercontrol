// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::repositories::utils::DirectCommand;
use anyhow::Result;
use log::{info, warn};
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

/// Checks whether the given system user already exists via the `id` command.
pub async fn plugin_user_exists(username: &str) -> bool {
    DirectCommand::new("id", USER_CMD_TIMEOUT)
        .arg(username)
        .run_with_code()
        .await
        .is_ok_and(|(code, _, _)| code == 0)
}

/// Creates the plugin user if it does not already exist. Logs a warning on failure.
pub async fn ensure_plugin_user(username: &str) {
    if plugin_user_exists(username).await {
        info!("Plugin user '{username}' already exists");
        return;
    }
    info!("Creating plugin user '{username}'");
    if let Err(err) = create_plugin_user(username).await {
        warn!("Failed to create plugin user '{username}': {err}");
    }
}

/// Tries `useradd` first (systemd distros, Gentoo, Artix, Void), then falls back
/// to `adduser` for Alpine Linux (`BusyBox`).
async fn create_plugin_user(username: &str) -> Result<()> {
    let (code, _, _) = DirectCommand::new("useradd", USER_CMD_TIMEOUT)
        .arg("--system")
        .arg("--comment")
        .arg("CoolerControl unprivileged plugin user")
        .arg("--shell")
        .arg("/usr/sbin/nologin")
        .arg(username)
        .run_with_code()
        .await?;
    if code == 0 {
        return Ok(());
    }
    // Fall back to `adduser` for Alpine Linux (BusyBox).
    let (code, _, stderr) = DirectCommand::new("adduser", USER_CMD_TIMEOUT)
        .arg("-S") // system user
        .arg("-D") // no password
        .arg("-H") // no home directory
        .arg("-h")
        .arg("/dev/null")
        .arg("-s")
        .arg("/sbin/nologin")
        .arg(username)
        .run_with_code()
        .await?;
    if code != 0 {
        anyhow::bail!("useradd failed: {stderr}");
    }
    Ok(())
}

/// Deletes the plugin user if it exists.
/// Tries `userdel` first (systemd distros, Gentoo, Artix, Void), then falls back
/// to `deluser` for Alpine Linux (`BusyBox`).
pub async fn delete_plugin_user(username: &str) -> Result<()> {
    let userdel_ok = DirectCommand::new("userdel", USER_CMD_TIMEOUT)
        .arg(username)
        .run_with_code()
        .await
        .is_ok_and(|(code, _, _)| code == 0);
    if userdel_ok {
        return Ok(());
    }
    // Fall back to `deluser` for Alpine Linux (BusyBox).
    let (code, _, stderr) = DirectCommand::new("deluser", USER_CMD_TIMEOUT)
        .arg(username)
        .run_with_code()
        .await?;
    if code != 0 {
        anyhow::bail!("Failed to delete user {username}: {stderr}");
    }
    Ok(())
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

#[cfg(all(test, feature = "gated-tests"))]
mod tests {
    use super::*;

    fn is_root() -> bool {
        nix::unistd::geteuid().is_root()
    }

    #[tokio::test]
    async fn test_plugin_user_exists_returns_true_for_root() {
        // root always exists on Linux
        crate::sidecar::ensure_test_handle();
        assert!(plugin_user_exists("root").await);
    }

    #[tokio::test]
    async fn test_plugin_user_exists_returns_false_for_nonexistent() {
        crate::sidecar::ensure_test_handle();
        assert!(!plugin_user_exists("nonexistent_user_xyz_12345").await);
    }

    #[tokio::test]
    async fn test_ensure_plugin_user_does_not_panic_for_nonexistent() {
        // Should not panic; will log a warning when user creation fails
        // (expected in test environment without root privileges).
        crate::sidecar::ensure_test_handle();
        if is_root() {
            // Skip: as root `useradd` would succeed and leave a real account behind on the
            // build host, and the second run would no longer exercise the failure path at all.
            return;
        }
        ensure_plugin_user("cc-plugin-test-nonexistent").await;
    }
}
