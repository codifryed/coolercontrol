/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
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
use crate::config::Config;
use crate::modes::ModeController;
use crate::processing::settings::SettingsController;
use crate::sleep_listener::SleepListener;
use crate::Repos;
use anyhow::{Context, Result};
use log::{error, info, trace};
use moro_local::Scope;
use std::cell::LazyCell;
use std::ops::Not;
use std::rc::Rc;
use std::time::Duration;
use tokio::time;
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;

static LOOP_TICK_DURATION_MS: u64 = 1000;
static SNAPSHOT_WAIT_MS: u64 = 400;
static WAKE_PAUSE_MINIMUM_S: u64 = 1;
static LCD_TIMEOUT_S: u64 = 2;

/// Run the main loop of the application.
///
/// This involves periodically checking for changes in the configuration, processing all
/// devices, and checking for changes in the sleep state of the system.
///
/// The main loop will exit when the application receives a termination signal.
pub async fn run<'s>(
    config: Rc<Config>,
    repos: Repos,
    settings_controller: Rc<SettingsController>,
    mode_controller: Rc<ModeController>,
    run_token: CancellationToken,
) -> Result<()> {
    tokio::pin! {
        let loop_interval = time::interval(Duration::from_millis(LOOP_TICK_DURATION_MS));
    }
    let snapshot_timeout_duration = LazyCell::new(|| Duration::from_millis(SNAPSHOT_WAIT_MS));
    let mut run_lcd_update = false; // toggle lcd updates every other loop tick
    moro_local::async_scope!(|scope| -> Result<()> {
        let sleep_listener = SleepListener::new(run_token.clone(), scope)
            .await
            .with_context(|| "Creating DBus Sleep Listener")?;
        while run_token.is_cancelled().not() {
            loop_interval.tick().await;
            if sleep_listener.is_not_preparing_to_sleep() {
                let snapshot_timeout_token = CancellationToken::new();
                fire_preloads(&repos, snapshot_timeout_token.clone(), scope);
                tokio::select! {
                    // This ensures that our status snapshots are taken a regular intervals,
                    // regardless of how long a particular device's status preload takes.
                    () = sleep(*snapshot_timeout_duration) => trace!("Snapshot timeout triggered before preload finished"),
                    () = snapshot_timeout_token.cancelled() => trace!("Preload finished before snapshot timeout"),
                }
                fire_snapshots_and_processes(&repos, &settings_controller, run_lcd_update, scope);
                run_lcd_update = !run_lcd_update;
            } else if sleep_listener.is_resuming() {
                wake_from_sleep(
                    &config,
                    &settings_controller,
                    &mode_controller,
                    &sleep_listener,
                )
                .await?;
            }
        }
        Ok(())
    })
    .await
}

/// Initiates the preload process for all repositories.
///
/// This function spawns asynchronous tasks that trigger the `preload_statuses` method
/// for each repository in the given `repos`. It ensures that all preload tasks are
/// completed before sending a signal through the `tx_preload` sender to trigger snapshots
/// if completed before the `snapshot_timeout`.
fn fire_preloads<'s>(
    repos: &'s Repos,
    snapshot_timeout_token: CancellationToken,
    scope: &'s Scope<'s, 's, Result<()>>,
) {
    scope.spawn(async move {
        // This scope ensures that all concurrent preload tasks have completed.
        moro_local::async_scope!(|preload_scope| {
            for repo in repos.iter() {
                let repo = Rc::clone(repo);
                preload_scope.spawn(async move {
                    trace!("STATUS PRELOAD triggered for {} repo", repo.device_type());
                    repo.preload_statuses().await;
                });
            }
        })
        .await;
        snapshot_timeout_token.cancel();
    });
}

/// Fires the status snapshot tasks for all repositories and processes scheduled speeds.
///
/// This function triggers all repository status updates concurrently, ensuring that snapshots
/// for all devices are taken simultaneously. It subsequently calls `fire_lcd_update` to manage
/// LCD updates and `process_scheduled_speeds` to apply any scheduled speed settings.
fn fire_snapshots_and_processes<'s>(
    repos: &'s Repos,
    settings_controller: &'s Rc<SettingsController>,
    run_lcd_update: bool,
    scope: &'s Scope<'s, 's, Result<()>>,
) {
    scope.spawn(async move {
        // snapshots for all devices should be done at the same time. (this is very fast)
        for repo in repos.iter() {
            if let Err(err) = repo.update_statuses().await {
                error!("Error trying to update status: {err}");
            }
        }
        fire_lcd_update(settings_controller, run_lcd_update, scope);
        settings_controller.process_scheduled_speeds().await;
    });
}

/// This function will fire off the LCD Update job which often takes a long time (>1.0s, <2.0s)
/// due to device communication time currently needed. It runs in its own task, and internally CPU
/// bound work runs on its own thread to not affect the other jobs in the main loop, but will also
/// time out to avoid jobs from pilling up.
///
/// Due to the long-running time of this function, it will be called every other loop tick.
fn fire_lcd_update<'s>(
    settings_controller: &Rc<SettingsController>,
    run_lcd_update: bool,
    scope: &'s Scope<'s, 's, Result<()>>,
) {
    if run_lcd_update.not()
        || settings_controller
            .lcd_commander
            .scheduled_settings
            .borrow()
            .is_empty()
    {
        return;
    }
    let lcd_commander = Rc::clone(&settings_controller.lcd_commander);
    scope.spawn(async move {
        if timeout(
            Duration::from_secs(LCD_TIMEOUT_S),
            lcd_commander.update_lcd(),
        )
        .await
        .is_err()
        {
            error!("LCD Scheduler timed out after {LCD_TIMEOUT_S}s");
        };
    });
}

/// Handles the actions needed to properly wake the system from sleep mode.
///
/// This function ensures that the necessary delays are observed to allow hardware components
/// to fully power up before re-initializing and re-applying device settings. It checks if
/// settings should be applied on boot and takes appropriate actions, such as reinitializing
/// devices and applying saved device settings. Additionally, it reinitializes all status
/// histories to maintain sequential data integrity and resets the sleep listener's state
/// flags to indicate that the system is no longer preparing to sleep or resuming.
async fn wake_from_sleep(
    config: &Rc<Config>,
    settings_controller: &Rc<SettingsController>,
    mode_controller: &Rc<ModeController>,
    sleep_listener: &SleepListener,
) -> Result<()> {
    sleep(
        config
            .get_settings()?
            .startup_delay
            .max(Duration::from_secs(WAKE_PAUSE_MINIMUM_S)),
    )
    .await;
    if config.get_settings()?.apply_on_boot {
        info!("Re-initializing and re-applying settings after waking from sleep");
        settings_controller.reinitialize_devices().await;
        mode_controller.apply_all_saved_device_settings().await;
    }
    settings_controller.reinitialize_all_status_histories();
    sleep_listener.resuming(false);
    sleep_listener.preparing_to_sleep(false);
    Ok(())
}
