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
use std::cell::Cell;
use std::ops::Not;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::time;
use tokio::time::{sleep, timeout};

static LOOP_TICK_DURATION: LazyLock<Duration> = LazyLock::new(|| Duration::from_millis(1000));
static SNAPSHOT_WAIT: LazyLock<Duration> = LazyLock::new(|| Duration::from_millis(400));
static WAKE_PAUSE_MINIMUM: LazyLock<Duration> = LazyLock::new(|| Duration::from_secs(1));
static LCD_TIMEOUT: LazyLock<Duration> = LazyLock::new(|| Duration::from_secs(2));

/// Run the main loop of the application.
///
/// This involves periodically checking for changes in the configuration, processing all
/// devices, and checking for changes in the sleep state of the system.
///
/// The main loop will exit when the application receives a termination signal.
pub async fn run<'s>(
    term_signal: Arc<AtomicBool>,
    config: Rc<Config>,
    repos: Repos,
    settings_controller: Rc<SettingsController>,
    mode_controller: Rc<ModeController>,
) -> Result<()> {
    let mut interval = time::interval(*LOOP_TICK_DURATION);
    let mut run_lcd_update = false; // toggle lcd updates every other loop tick
    moro_local::async_scope!(|scope| -> Result<()> {
        let sleep_listener = SleepListener::new(scope)
            .await
            .with_context(|| "Creating DBus Sleep Listener")?;

        while !term_signal.load(Ordering::Relaxed) {
            interval.tick().await;
            if sleep_listener.is_preparing_to_sleep().not() {
                let snapshots_fired = Rc::new(Cell::new(false));
                fire_preloads(
                    &repos,
                    &settings_controller,
                    run_lcd_update,
                    Rc::clone(&snapshots_fired),
                    scope,
                );
                snapshots_and_processes_timeout(
                    &repos,
                    &settings_controller,
                    run_lcd_update,
                    snapshots_fired,
                    scope,
                );
                run_lcd_update = !run_lcd_update;
            } else if sleep_listener.is_resuming() {
                // delay at least a second to allow the hardware to fully wake up:
                sleep(
                    config
                        .get_settings()
                        .await?
                        .startup_delay
                        .max(*WAKE_PAUSE_MINIMUM),
                )
                .await;
                if config.get_settings().await?.apply_on_boot {
                    info!("Re-initializing and re-applying settings after waking from sleep");
                    settings_controller.reinitialize_devices().await;
                    mode_controller.apply_all_saved_device_settings().await;
                }
                settings_controller
                    .reinitialize_all_status_histories()
                    .await;
                sleep_listener.resuming(false);
                sleep_listener.preparing_to_sleep(false);
            }
        }
        Ok(())
    })
    .await
}

/// Runs the status preload task for every repository individually.
/// This allows each repository to handle its own timings for its devices and be independent
/// of the status snapshots that will happen regardless every loop tick.
fn fire_preloads<'s>(
    repos: &'s Repos,
    settings_controller: &'s Rc<SettingsController>,
    run_lcd_update: bool,
    snapshots_fired: Rc<Cell<bool>>,
    scope: &'s Scope<'s, 's, Result<()>>,
) {
    scope.spawn(async move {
        moro_local::async_scope!(|scope| {
            for repo in repos.iter() {
                let repo = Rc::clone(repo);
                scope.spawn(async move {
                    trace!("STATUS PRELOAD triggered for {} repo", repo.device_type());
                    repo.preload_statuses().await;
                });
            }
        })
        .await;
        fire_snapshots_and_processes(
            repos,
            settings_controller,
            run_lcd_update,
            snapshots_fired,
            scope,
        )
        .await;
    });
}

/// This function will fire off the status snapshot tasks for all repositories, and then call
/// the `process_scheduled_speeds` function on the settings controller. This should be run
/// separately to ensure that the status snapshots are independently and consistently taken and
/// used for processing scheduled speeds.
fn snapshots_and_processes_timeout<'s>(
    repos: &'s Repos,
    settings_controller: &'s Rc<SettingsController>,
    run_lcd_update: bool,
    snapshots_fired: Rc<Cell<bool>>,
    scope: &'s Scope<'s, 's, Result<()>>,
) {
    scope.spawn(async move {
        // This sleep timeout makes is so that the snapshots are fired at the latest
        // once this timeout expires. Otherwise, they're fired right after preloading
        // of all statuses has completed.
        sleep(*SNAPSHOT_WAIT).await;
        fire_snapshots_and_processes(
            repos,
            settings_controller,
            run_lcd_update,
            snapshots_fired,
            scope,
        )
        .await;
    });
}

async fn fire_snapshots_and_processes<'s>(
    repos: &'s Repos,
    settings_controller: &Rc<SettingsController>,
    run_lcd_update: bool,
    snapshots_fired: Rc<Cell<bool>>,
    scope: &'s Scope<'s, 's, Result<()>>,
) {
    if snapshots_fired.get() {
        return;
    }
    snapshots_fired.set(true);
    // snapshots for all devices should be done at the same time. (this is very fast)
    for repo in repos.iter() {
        if let Err(err) = repo.update_statuses().await {
            error!("Error trying to update status: {err}");
        }
    }
    fire_lcd_update(settings_controller, run_lcd_update, scope).await;
    settings_controller.process_scheduled_speeds().await;
}

/// This function will fire off the LCD Update job which often takes a long time (>1.0s, <2.0s)
/// due to device communication time currently needed. It runs in its own task, and internally CPU
/// bound work runs on its own thread to not affect the other jobs in the main loop, but will also
/// time out to avoid jobs from pilling up.
///
/// Due to the long-running time of this function, it will be called every other loop tick.
async fn fire_lcd_update<'s>(
    settings_controller: &Rc<SettingsController>,
    run_lcd_update: bool,
    scope: &'s Scope<'s, 's, Result<()>>,
) {
    if run_lcd_update.not()
        || settings_controller
            .lcd_commander
            .scheduled_settings
            .read()
            .await
            .is_empty()
    {
        return;
    }
    let lcd_commander = Rc::clone(&settings_controller.lcd_commander);
    scope.spawn(async move {
        if timeout(*LCD_TIMEOUT, lcd_commander.update_lcd())
            .await
            .is_err()
        {
            error!("LCD Scheduler timed out after {LCD_TIMEOUT:?}");
        };
    });
}
