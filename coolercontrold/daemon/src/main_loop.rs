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

//! Main daemon loop and the timing layers it orchestrates.
//!
//! Each tick has three independent wall-clock budgets, one per
//! invariant. They do *not* derive from each other; changing one
//! does not (and must not) implicitly resize the others.
//!
//! 1. `poll_rate` (configurable, 0.5..=5.0 s, default 1.0 s).
//!    Interval between ticks. Sets how often device state is
//!    sampled and the engine's processors run.
//!
//! 2. `SNAPSHOT_TIMEOUT_MS` (this module, fixed 400 ms). Hard
//!    cap on how long a tick waits for device preloads before
//!    forcing a snapshot. Bounds engine reaction time so a
//!    wedged device cannot delay the engine's response to a
//!    temperature change. Constant on purpose: a user picking
//!    a slow poll for low CPU usage does not also want a slow
//!    engine reaction.
//!
//! 3. Per-device read permit timeout (in each repository,
//!    scales with `poll_rate`). Cap on how long the *next*
//!    tick's preload waits to acquire a device's permit while
//!    the previous tick's reads are still in flight. Allowed
//!    to exceed `SNAPSHOT_TIMEOUT_MS`: a slow read just lands
//!    in the next snapshot. The repo's per-channel staleness
//!    counter ticks while a read is overdue, and after
//!    `MISSING_STATUS_THRESHOLD` consecutive misses the
//!    failsafe layer substitutes safe defaults so the engine
//!    never hands stale values to a processor.
//!
//! Worst-case engine reaction latency to a temperature change
//! is `poll_rate + SNAPSHOT_TIMEOUT_MS` (the change can happen
//! just after a tick fires, the next tick may take up to the
//! snapshot timeout before processors run). With defaults that
//! is 1.4 s; at minimum poll rate it is 0.9 s. Both are well
//! below CPU/GPU thermal time constants, so this latency does
//! not bound control quality.
//!
//! Healthy reads complete in tens of milliseconds, so layer 2
//! almost never triggers in practice; when it does, at least
//! one device is wedged and the engine should not be waiting
//! for it.

use crate::alerts::AlertController;
use crate::api::actor::StatusHandle;
use crate::config::Config;
use crate::engine::main::Engine;
use crate::modes::ModeController;
use crate::rt;
use crate::sleep_listener::SleepListener;
use crate::Repos;
use anyhow::Result;
use log::{debug, error, info, trace, warn};
use moro_local::Scope;
use std::ops::Not;
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio_util::sync::CancellationToken;

/// Hard cap on how long the main loop waits for device preloads
/// before forcing a snapshot. See module-level doc for the full
/// timing model and why this is independent of `poll_rate`.
const SNAPSHOT_TIMEOUT_MS: u64 = 400;
const WAKE_PAUSE_MINIMUM_S: u64 = 1;
// setting (temp) images is pretty quick, <2s, but gifs can take significantly longer >3-4s
const LCD_TIMEOUT_S: u64 = 5;
const LCD_MAX_UPDATE_RATE_S: f64 = 2.0;
const FULL_SECOND_MS: u64 = 1000;

/// Run the main loop of the application.
///
/// This involves periodically checking for changes in the configuration, processing all
/// devices, and checking for changes in the sleep state of the system.
///
/// The main loop will exit when the application receives a termination signal.
pub async fn run(
    config: Rc<Config>,
    repos: Repos,
    engine: Rc<Engine>,
    mode_controller: Rc<ModeController>,
    alert_controller: Rc<AlertController>,
    status_handle: StatusHandle,
    run_token: CancellationToken,
) -> Result<()> {
    let poll_rate = config.get_settings()?.poll_rate;
    let snapshot_timeout_duration = Duration::from_millis(SNAPSHOT_TIMEOUT_MS);
    let mut lcd_update_trigger = LCDUpdateTrigger::new(poll_rate);
    moro_local::async_scope!(|scope| -> Result<()> {
        let sleep_listener = SleepListener::new(run_token.clone());
        align_loop_timing_with_clock().await;
        // The sub-second position is set on interval creation:
        let mut loop_interval = rt::interval(Duration::from_secs_f64(poll_rate));
        let mut sleep_prepared = false;
        while run_token.is_cancelled().not() {
            loop_interval.tick().await;
            lcd_update_trigger.tick();
            if sleep_listener.is_not_preparing_to_sleep() {
                let snapshot_timeout_token = CancellationToken::new();
                fire_preloads(&repos, snapshot_timeout_token.clone(), scope);
                tokio::select! {
                    // Bounds the engine's reaction time. If the
                    // sleep arm wins, at least one device is
                    // taking longer than the snapshot budget and
                    // we proceed with whatever the cache holds so
                    // the snapshot/processor phase fires on time.
                    // Late-arriving reads complete in the
                    // background and land in the next tick's
                    // snapshot. See the module-level doc for the
                    // full timing model.
                    () = rt::sleep(snapshot_timeout_duration) => trace!("Snapshot timeout triggered before preload finished"),
                    () = snapshot_timeout_token.cancelled() => trace!("Preload finished before snapshot timeout"),
                }
                fire_snapshots_and_processes(&repos, &engine, &mut lcd_update_trigger, &status_handle, scope).await;
                alert_controller.process_alerts();
            } else if sleep_listener.is_resuming() {
                sleep_prepared = false;
                wake_from_sleep(
                    &config,
                    &engine,
                    &mode_controller,
                    &sleep_listener,
                )
                .await?;
            } else {
                if sleep_prepared.not() {
                    for repo in repos.iter() {
                        repo.prepare_for_sleep().await;
                    }
                    sleep_prepared = true;
                }
                debug!("Skipping polling loop operations while system is entering/leaving sleep mode.");
            }
        }
        // run_token has been cancelled. Signal each repo to abort
        // in-flight queued work so the moro scope's spawned preload
        // tasks self-bail instead of holding the scope open while we
        // wait. Critical for hwmon: a slow device's mid-tick preload
        // would otherwise keep the scope alive long enough for
        // liqctld's watch_child grace window to fire and force-kill
        // the python service before liquidctl_repo::shutdown could
        // /quit it cleanly.
        for repo in repos.iter() {
            repo.abort_pending().await;
        }
        engine.apply_lcd_shutdown_images().await;
        Ok(())
    })
    .await
}

/// Aligns the main loop's timing with the system clock.
///
/// This function calculates the current time in milliseconds since the last full second
/// and determines how long to wait before the next full second mark. This ensures that
/// the main loop ticks at a consistent sub-second position, which helps Frontends maintain
/// consistent timestamps without random start-timing fluctuation.
async fn align_loop_timing_with_clock() {
    let current_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_millis();
    let wait_duration = FULL_SECOND_MS - u64::from(current_millis);
    rt::sleep(Duration::from_millis(wait_duration)).await;
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
async fn fire_snapshots_and_processes<'s>(
    repos: &'s Repos,
    engine: &'s Rc<Engine>,
    lcd_update_trigger: &mut LCDUpdateTrigger,
    status_handle: &'s StatusHandle,
    scope: &'s Scope<'s, 's, Result<()>>,
) {
    // snapshots for all devices should be done at the same time. (this is very fast)
    for repo in repos.iter() {
        if let Err(err) = repo.update_statuses().await {
            error!("Error trying to update status: {err}");
        }
    }
    fire_lcd_update(engine, lcd_update_trigger, scope);
    engine.process_scheduled_speeds(scope);
    status_handle.broadcast_status().await;
}

/// This function will fire off the LCD Update job which often takes a long time (>1.0s, <2.0s)
/// due to device communication time currently needed. It runs in its own task, and internally CPU
/// bound work runs on its own thread to not affect the other jobs in the main loop, but will also
/// time out to avoid jobs from pilling up.
///
/// Due to the long-running time of this function, it will be called every other loop tick.
fn fire_lcd_update<'s>(
    engine: &Rc<Engine>,
    lcd_update_trigger: &mut LCDUpdateTrigger,
    scope: &'s Scope<'s, 's, Result<()>>,
) {
    if lcd_update_trigger.not_triggered()
        || engine.lcd_commander.scheduled_settings.borrow().is_empty()
    {
        return;
    }
    let lcd_commander = Rc::clone(&engine.lcd_commander);
    scope.spawn(async move {
        if rt::timeout(
            Duration::from_secs(LCD_TIMEOUT_S),
            lcd_commander.update_lcd(),
        )
        .await
        .is_err()
        {
            warn!(
                "LCD Scheduler timed out after {LCD_TIMEOUT_S}s. \
                 LCD communication is taking longer than expected"
            );
        }
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
    engine: &Rc<Engine>,
    mode_controller: &Rc<ModeController>,
    sleep_listener: &SleepListener,
) -> Result<()> {
    let startup_delay = config
        .get_settings()?
        .startup_delay
        .max(Duration::from_secs(WAKE_PAUSE_MINIMUM_S));
    info!(
        "Waiting {}s before resuming after waking from sleep.",
        startup_delay.as_secs()
    );
    rt::sleep(startup_delay).await;
    if config.get_settings()?.apply_on_boot {
        info!("Re-initializing and re-applying settings after waking from sleep");
        engine.reinitialize_devices().await;
        moro_local::async_scope!(|scope| mode_controller.apply_all_saved_device_settings(scope))
            .await;
    }
    engine.reinitialize_all_status_histories()?;
    sleep_listener.resuming(false);
    sleep_listener.preparing_to_sleep(false);
    Ok(())
}

/// A helper struct used to limit LCD updates to a maximum frequency.
///
/// This is needed because the current LCD driver implementation requires us to send a complete
/// image to the device on every update, which takes a significant amount of time.
///
/// `LCDUpdateTrigger` is used to manage the rate at which LCD updates are performed. It ensures
/// that LCD updates are not performed too frequently by maintaining a count of main loop
/// iterations and comparing it to a calculated threshold. The threshold is calculated by
/// dividing the maximum allowed LCD update rate (2.0 seconds) by the configured poll rate.
struct LCDUpdateTrigger {
    loop_count: usize,
    trigger_count: usize,
}

impl LCDUpdateTrigger {
    /// Creates a new `LCDUpdateTrigger` with the given poll rate in seconds.
    ///
    /// The `loop_count` is initialized to the calculated `trigger_count` so that the first
    /// iteration of the main loop will trigger an LCD update.
    fn new(poll_rate_secs: f64) -> Self {
        let trigger_count = Self::calc_lcd_update_rate(poll_rate_secs);
        Self {
            loop_count: trigger_count,
            trigger_count,
        }
    }

    /// Calculates the number of main loop ticks between LCD updates.
    ///
    /// This function returns the number of main loop ticks between LCD updates based on the
    /// configured poll rate. The calculated value is the ceiling of the division of the
    /// maximum allowed LCD update rate (2.0 seconds) by the configured poll rate.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn calc_lcd_update_rate(poll_rate: f64) -> usize {
        let lcd_update_rate = LCD_MAX_UPDATE_RATE_S / poll_rate;
        lcd_update_rate.ceil() as usize
    }

    fn tick(&mut self) {
        self.loop_count += 1;
    }

    fn not_triggered(&mut self) -> bool {
        if self.loop_count >= self.trigger_count {
            self.loop_count = 0;
            false
        } else {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: the snapshot timeout must stay at the historical
    /// 400 ms value. It bounds engine reaction latency to a
    /// temperature change to `poll_rate + SNAPSHOT_TIMEOUT_MS` and
    /// is what guarantees a wedged device cannot push that out.
    /// See the module-level doc for the full rationale.
    #[test]
    fn snapshot_timeout_holds_at_legacy_400ms() {
        assert_eq!(SNAPSHOT_TIMEOUT_MS, 400);
    }

    /// Sanity bound on worst-case engine reaction latency at the
    /// minimum poll rate. If either the snapshot timeout or the
    /// minimum supported poll rate is changed, this bound forces
    /// a deliberate review: thermal control quality depends on
    /// this latency staying well below CPU/GPU thermal time
    /// constants.
    #[test]
    fn worst_case_reaction_latency_at_min_poll_rate_under_one_second() {
        let min_poll_rate = Duration::from_millis(500);
        let worst_case = min_poll_rate + Duration::from_millis(SNAPSHOT_TIMEOUT_MS);
        assert!(
            worst_case < Duration::from_secs(1),
            "worst-case latency {worst_case:?} must stay sub-second \
             at the minimum poll rate"
        );
    }
}
