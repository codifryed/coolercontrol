// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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
use crate::device::health::DeviceHealthController;
use crate::engine::main::Engine;
use crate::listeners::sleep::SleepListener;
use crate::modes::ModeController;
use crate::rt;
use crate::watchdog::Watchdog;
use crate::Repos;
use anyhow::Result;
use chrono::Local;
use futures_util::future::join_all;
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
    device_health_controller: Rc<DeviceHealthController>,
    status_handle: StatusHandle,
    watchdog: Rc<Watchdog>,
    run_token: CancellationToken,
) -> Result<()> {
    let poll_rate = config.get_settings()?.poll_rate;
    watchdog.warn_if_poll_rate_too_slow(Duration::from_secs_f64(poll_rate));
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
                device_health_controller.process().await;
                // At the foot of the working arm: a heartbeat asserts the tick
                // did its work. The sleep-preparation arm below deliberately
                // does nothing, so it must not claim liveness. The resume arm
                // heartbeats from inside `wake_from_sleep`.
                watchdog.ping();
            } else if sleep_listener.is_resuming() {
                sleep_prepared = false;
                wake_from_sleep(
                    &config,
                    &engine,
                    &mode_controller,
                    &sleep_listener,
                    &watchdog,
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
/// This function spawns one task that runs the `preload_statuses` method of every
/// repository in the given `repos` concurrently. It ensures that all preloads have
/// completed before cancelling the snapshot timeout token to trigger snapshots
/// if completed before the `snapshot_timeout`.
fn fire_preloads<'s>(
    repos: &'s Repos,
    snapshot_timeout_token: CancellationToken,
    scope: &'s Scope<'s, 's, Result<()>>,
) {
    scope.spawn(async move {
        // Waits for every concurrent preload to complete.
        join_all(repos.iter().map(|repo| Rc::clone(repo).preload_statuses())).await;
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
    // One instant for the whole tick, so every device's entry for it carries the same time.
    let timestamp = Local::now();
    for repo in repos.iter() {
        if let Err(err) = repo.update_statuses(timestamp) {
            error!("Error trying to update status: {err}");
        }
    }
    fire_lcd_update(engine, lcd_update_trigger, scope);
    engine.process_scheduled_speeds(scope);
    status_handle.broadcast_status().await;
}

/// This function will fire off the LCD Update job which often takes a long time (>1.0s, <2.0s)
/// due to device communication time currently needed. It runs in its own task; the CPU-bound
/// image generation and PNG write run on the blocking pool via `rt::spawn_blocking` so they do
/// not stall the other jobs in the main loop, and the whole job times out to avoid piling up.
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
    watchdog: &Watchdog,
) -> Result<()> {
    let startup_delay = config
        .get_settings()?
        .startup_delay
        .max(Duration::from_secs(WAKE_PAUSE_MINIMUM_S));
    info!(
        "Waiting {}s before resuming after waking from sleep.",
        startup_delay.as_secs()
    );
    // `startup_delay` reaches 120s and blocks the loop. The daemon is idle here
    // on purpose, which must not read to systemd as a wedged loop.
    watchdog.sleep_with_heartbeat(startup_delay).await;
    if config.get_settings()?.apply_on_boot {
        info!("Re-initializing and re-applying settings after waking from sleep");
        engine.reinitialize_devices().await;
        // Re-init and re-apply are each unbounded and can together outlast the
        // watchdog deadline on a machine with many slow devices. A heartbeat
        // between them keeps that from reading as a wedge. A single step
        // blocking past the deadline is still a restart, which is correct: at
        // that point the daemon really has stopped making progress.
        watchdog.ping();
        moro_local::async_scope!(|scope| mode_controller.apply_all_saved_device_settings(scope))
            .await;
        watchdog.ping();
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
    use crate::device::DeviceType;
    use crate::repositories::repository::{Repositories, Repository, StubRepo};
    use std::cell::Cell;
    use tokio::sync::Barrier;

    /// Run one after another, the first preload would wait for the second forever.
    #[test]
    fn every_repo_preloads_at_the_same_time() {
        let meeting = Rc::new(Barrier::new(2));
        let finished = Rc::new(Cell::new(0));
        let repo = |device_type| -> Option<Rc<dyn Repository>> {
            Some(Rc::new(StubRepo {
                device_type,
                meeting: Some(Rc::clone(&meeting)),
                finished: Rc::clone(&finished),
                failing_writes: Rc::default(),
            }))
        };
        let repos: Repos = Rc::new(Repositories {
            cpu: repo(DeviceType::CPU),
            hwmon: repo(DeviceType::Hwmon),
            ..Default::default()
        });
        let token = CancellationToken::new();
        let preloads = moro_local::async_scope!(|scope| -> Result<()> {
            fire_preloads(&repos, token.clone(), scope);
            Ok(())
        });

        // Run in sequence, the scope never finishes and poll_to_end panics.
        let (outcome, _) = rt::poll_to_end(preloads);

        assert!(outcome.is_ok());
        assert_eq!(finished.get(), 2);
        assert!(token.is_cancelled());
    }

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
