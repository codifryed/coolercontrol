// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::api::actor::ModeHandle;
use crate::device::UID;
use crate::system_event::{SystemEvent, SystemEventHandle, SystemEventKind};
use crate::ENV_DBUS;
use futures_util::StreamExt;
use log::{error, info, warn};
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::HashMap;
use std::env;
use std::ops::Not;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;
use zbus::proxy::{Builder as ProxyBuilder, CacheProperties};
use zbus::{Connection, Proxy};

/// `power-profiles-daemon` and the `tuned-ppd` shim both publish this interface. The freedesktop
/// name is preferred; `net.hadess` is the pre-rename name that older builds still own.
const PPD_BUS_NAME: &str = "org.freedesktop.UPower.PowerProfiles";
const PPD_OBJECT_PATH: &str = "/org/freedesktop/UPower/PowerProfiles";
const PPD_LEGACY_BUS_NAME: &str = "net.hadess.PowerProfiles";
const PPD_LEGACY_OBJECT_PATH: &str = "/net/hadess/PowerProfiles";
const ACTIVE_PROFILE_PROPERTY: &str = "ActiveProfile";
const PROFILES_PROPERTY: &str = "Profiles";
/// Each entry of the `Profiles` array is a dict; this key holds the profile name.
const PROFILE_NAME_KEY: &str = "Profile";

/// Matches `sleep_listener`: cap the whole handshake so a wedged dbus-broker cannot stall the
/// listener forever. On timeout we retry rather than run deaf for the rest of the session.
const DBUS_SETUP_TIMEOUT_S: u64 = 5;

/// How long to wait before trying the bus again. The power profile daemon can be restarted,
/// installed, or stopped at any time, so absence is never permanent. Long enough that a daemon
/// in a restart loop cannot spin this listener.
const RECONNECT_DELAY_S: u64 = 30;

/// A point-in-time view of the power profile integration, for API consumers.
#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
pub struct PowerProfileSnapshot {
    /// Profiles the system daemon offers. Empty when no power profile daemon has been reached,
    /// which is how a client knows to hide the feature.
    pub available: Vec<String>,
    /// The profile currently in effect, if one has been observed.
    pub active: Option<String>,
    /// Profile name to Mode UID. Profiles absent here are deliberately unmapped.
    pub modes: HashMap<String, UID>,
}

#[derive(Default)]
struct PowerProfileState {
    available: Vec<String>,
    active: Option<String>,
    modes: HashMap<String, UID>,
}

/// Shared state for the power profile integration: what the listener has observed, plus the
/// profile to Mode mapping.
///
/// Shared rather than owned because the listener runs on the Tokio sidecar while the config and
/// the API live on the main thread. That also means a mapping edit takes effect immediately
/// instead of at the next daemon restart.
#[derive(Clone, Default)]
pub struct PowerProfiles {
    state: Arc<RwLock<PowerProfileState>>,
}

impl PowerProfiles {
    pub fn new(modes: HashMap<String, UID>) -> Self {
        debug_assert!(
            modes.keys().all(|profile| profile.is_empty().not()),
            "A blank profile name can never match a real profile"
        );
        debug_assert!(
            modes.values().all(|mode_uid| mode_uid.is_empty().not()),
            "A blank Mode UID can never resolve to a Mode"
        );
        Self {
            state: Arc::new(RwLock::new(PowerProfileState {
                available: Vec::new(),
                active: None,
                modes,
            })),
        }
    }

    /// Poisoning cannot corrupt this state: the lock guards three plain fields and every critical
    /// section is a clone or a whole-field assignment, so a panic elsewhere cannot leave a broken
    /// invariant behind. Recovering the value is therefore correct, and unlike an ignored `Err` it
    /// never silently drops a write or reports "no mapping" for one that exists.
    fn read_state(&self) -> RwLockReadGuard<'_, PowerProfileState> {
        self.state.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write_state(&self) -> RwLockWriteGuard<'_, PowerProfileState> {
        self.state.write().unwrap_or_else(PoisonError::into_inner)
    }

    /// The Mode to activate for `profile`, or `None` when the profile is unmapped.
    pub fn mode_for(&self, profile: &str) -> Option<UID> {
        let mode_uid = self.read_state().modes.get(profile).cloned();
        debug_assert!(
            mode_uid
                .as_ref()
                .is_none_or(|mode_uid| mode_uid.is_empty().not()),
            "A stored mapping never holds a blank Mode UID"
        );
        mode_uid
    }

    pub fn set_modes(&self, modes: HashMap<String, UID>) {
        debug_assert!(
            modes.keys().all(|profile| profile.is_empty().not()),
            "A blank profile name can never match a real profile"
        );
        debug_assert!(
            modes.values().all(|mode_uid| mode_uid.is_empty().not()),
            "A blank Mode UID can never resolve to a Mode"
        );
        let written = modes.len();
        let mut state = self.write_state();
        state.modes = modes;
        debug_assert_eq!(
            state.modes.len(),
            written,
            "Every submitted mapping must be visible to the listener"
        );
    }

    /// Records what the listener found on the bus. Called on every connect, so a reconnect
    /// refills the list once the power profile daemon comes back.
    pub fn set_observed(&self, available: Vec<String>, active: Option<String>) {
        debug_assert!(
            available.iter().all(|profile| profile.is_empty().not()),
            "Blank profile names are dropped while decoding"
        );
        let mut state = self.write_state();
        state.available = available;
        state.active = active;
    }

    pub fn set_active(&self, active: Option<String>) {
        debug_assert!(
            active
                .as_ref()
                .is_none_or(|profile| profile.is_empty().not()),
            "A blank profile name can never match a real profile"
        );
        self.write_state().active = active;
    }

    pub fn snapshot(&self) -> PowerProfileSnapshot {
        let state = self.read_state();
        PowerProfileSnapshot {
            available: state.available.clone(),
            active: state.active.clone(),
            modes: state.modes.clone(),
        }
    }
}

/// Starts watching the system power profile, if dbus is enabled.
///
/// Fire and forget, like `SleepListener::new`: the connection and signal loop live on the Tokio
/// sidecar (zbus needs a Tokio reactor), and every failure degrades to running deaf rather than
/// failing daemon startup. Nothing is returned because the listener drives the SSE broadcast and
/// Mode activation itself and needs nothing from the main loop tick.
pub fn start(
    system_event_handle: SystemEventHandle,
    mode_handle: ModeHandle,
    profiles: PowerProfiles,
    run_token: CancellationToken,
) {
    if dbus_listener_enabled().not() {
        info!("DBUS power profile listener disabled.");
        return;
    }
    let listener = Listener {
        system_event_handle,
        mode_handle,
        profiles,
        run_token,
        current: None,
        seeded: false,
    };
    crate::sidecar::handle().spawn(move || listener.run());
}

fn dbus_listener_enabled() -> bool {
    dbus_listener_enabled_from(env::var(ENV_DBUS).ok().as_deref())
}

/// Resolve a raw `ENV_DBUS` value, `None` when unset. Split from the read so the branches are
/// testable without writing to the process environment, which is shared with every other test
/// running at the same time.
fn dbus_listener_enabled_from(env_dbus: Option<&str>) -> bool {
    env_dbus
        .and_then(|env_dbus| {
            env_dbus
                .parse::<u8>()
                .ok()
                .map(|enabled| enabled != 0)
                .or_else(|| Some(env_dbus.trim().to_lowercase() != "off"))
        })
        .unwrap_or(true)
}

/// What a fresh connection means for the profile we already hold.
#[derive(Debug, PartialEq, Eq)]
enum Reconnect {
    /// First connect: record the profile only. Activating a Mode here would fight `apply_on_boot`.
    Seed,
    /// The profile is the same one we already acted on, or the daemon would not say.
    Unchanged,
    /// The profile changed while we were deaf, so the change still has to be handled.
    Changed,
}

fn reconnect_action(seeded: bool, current: Option<&str>, observed: Option<&str>) -> Reconnect {
    if seeded.not() {
        return Reconnect::Seed;
    }
    match observed {
        Some(profile) if current != Some(profile) => Reconnect::Changed,
        _ => Reconnect::Unchanged,
    }
}

/// Runs on the sidecar: keeps a connection to the power profile daemon and reacts to
/// `ActiveProfile` changes until shutdown.
struct Listener {
    system_event_handle: SystemEventHandle,
    mode_handle: ModeHandle,
    profiles: PowerProfiles,
    run_token: CancellationToken,
    /// The last profile we know of. Carried across reconnects so a change missed while the
    /// daemon was away is still visible as a change.
    current: Option<String>,
    /// False until the first successful connect has recorded `current`.
    seeded: bool,
}

impl Listener {
    /// Connects, watches, and reconnects until the daemon shuts down. A power profile daemon that
    /// is absent, restarting, or installed later is normal, so no outcome is terminal.
    async fn run(mut self) {
        let mut outage = OutageLog::default();
        loop {
            match connect().await {
                ConnectOutcome::Connected(connection, proxy) => {
                    outage.connected();
                    let shutting_down = self.watch(&proxy).await;
                    let _ = connection.close().await;
                    if shutting_down {
                        return;
                    }
                    outage.lost();
                }
                ConnectOutcome::NotConnected(reason) => outage.not_connected(&reason),
            }
            tokio::select! {
                () = self.run_token.cancelled() => return,
                () = sleep(Duration::from_secs(RECONNECT_DELAY_S)) => {},
            }
        }
    }

    /// Watches `ActiveProfile` on an established connection. Returns true when the daemon is
    /// shutting down, false when the connection dropped and has to be re-established.
    async fn watch(&mut self, proxy: &Proxy<'static>) -> bool {
        let observed = proxy
            .get_property::<String>(ACTIVE_PROFILE_PROPERTY)
            .await
            .ok();
        // Refilled on every connect. Deliberately not cleared while disconnected: a client hides
        // the feature on an empty list, which would put an existing mapping out of reach for the
        // length of a power profile daemon restart.
        self.profiles
            .set_observed(available_profiles(proxy).await, observed.clone());
        let mut changes = proxy
            .receive_property_changed::<String>(ACTIVE_PROFILE_PROPERTY)
            .await;
        self.catch_up(observed).await;
        loop {
            tokio::select! {
                () = self.run_token.cancelled() => return true,
                Some(change) = changes.next() => {
                    let Ok(profile) = change.get().await else {
                        warn!("Failed to read the changed ActiveProfile value.");
                        continue;
                    };
                    // The property cache can replay the value we already hold.
                    if self.current.as_deref() == Some(profile.as_str()) {
                        continue;
                    }
                    self.apply(profile).await;
                },
                else => return false,
            }
        }
    }

    /// Reconciles what the daemon reports on connect with what we last acted on.
    async fn catch_up(&mut self, observed: Option<String>) {
        let action = reconnect_action(self.seeded, self.current.as_deref(), observed.as_deref());
        let reported = observed.clone().unwrap_or_else(|| "unknown".to_string());
        match action {
            Reconnect::Seed => {
                self.seeded = true;
                self.current = observed;
                info!("DBUS power profile listener connected. Active profile: {reported}");
            }
            Reconnect::Unchanged => {
                info!("DBUS power profile listener reconnected. Active profile: {reported}");
            }
            Reconnect::Changed => {
                info!("DBUS power profile listener reconnected. Active profile: {reported}");
                let Some(profile) = observed else {
                    debug_assert!(false, "Changed is only reachable with an observed profile");
                    return;
                };
                self.apply(profile).await;
            }
        }
    }

    /// Records the new profile, broadcasts it, then activates the mapped Mode if there is one.
    ///
    /// The broadcast is unconditional so external consumers see every change even when the user
    /// has mapped no Modes at all.
    async fn apply(&mut self, profile: String) {
        debug_assert!(
            profile.is_empty().not(),
            "A blank profile name can never match a mapping"
        );
        debug_assert!(
            self.current.as_deref() != Some(profile.as_str()),
            "An unchanged profile must be filtered out before it reaches apply"
        );
        let previous = self.current.replace(profile.clone());
        debug_assert_eq!(
            self.current.as_deref(),
            Some(profile.as_str()),
            "The new profile must be what a later change compares against"
        );
        info!(
            "System power profile changed to '{profile}' from '{}'.",
            previous.as_deref().unwrap_or("unknown")
        );
        self.profiles.set_active(Some(profile.clone()));
        let mode_uid = self.profiles.mode_for(&profile);
        self.system_event_handle.broadcast(SystemEvent {
            kind: SystemEventKind::PowerProfile,
            value: profile,
            previous,
        });
        let Some(mode_uid) = mode_uid else {
            return;
        };
        if let Err(err) = self.mode_handle.activate(mode_uid.clone()).await {
            error!("Failed to activate Mode {mode_uid} for the new power profile: {err}");
        }
    }
}

enum ConnectOutcome {
    Connected(Connection, Proxy<'static>),
    NotConnected(NoConnection),
}

/// Why a connect attempt produced no proxy.
enum NoConnection {
    /// No power profile daemon owns either bus name.
    Absent,
    Failed(zbus::Error),
    TimedOut,
}

impl NoConnection {
    /// Info, not a warning: a system with no power profile daemon installed is a normal system,
    /// and the line exists so a user looking for the feature can see why it is missing.
    fn log(&self) {
        match self {
            Self::Absent => info!(
                "No power profile daemon found on DBUS. Mode switching on power profile changes \
                 is unavailable until one appears."
            ),
            Self::Failed(err) => info!(
                "Could not connect to DBUS, the power profile listener is retrying every \
                 {RECONNECT_DELAY_S}s: {err}"
            ),
            Self::TimedOut => info!(
                "DBUS power profile listener setup timed out after {DBUS_SETUP_TIMEOUT_S}s, \
                 retrying every {RECONNECT_DELAY_S}s."
            ),
        }
    }
}

/// Reports an outage once, then stays quiet until the connection is back.
///
/// The retry runs every `RECONNECT_DELAY_S` for the life of the daemon, so a line per attempt
/// would be a line per 30s forever.
#[derive(Default)]
struct OutageLog {
    /// True once the current outage has been reported.
    reported: bool,
}

impl OutageLog {
    fn connected(&mut self) {
        self.reported = false;
    }

    /// A daemon that answered and then went away is a change the user did not ask for, so unlike
    /// a first connect that never worked, this warns.
    fn lost(&mut self) {
        if self.reported {
            return;
        }
        warn!(
            "Lost the connection to the power profile daemon. Retrying every \
             {RECONNECT_DELAY_S}s."
        );
        self.reported = true;
    }

    fn not_connected(&mut self, reason: &NoConnection) {
        if self.reported {
            return;
        }
        reason.log();
        self.reported = true;
    }
}

/// Connects and returns a proxy for whichever bus name is actually served.
async fn connect() -> ConnectOutcome {
    let setup = async {
        let connection = Connection::system().await?;
        for (bus_name, object_path) in [
            (PPD_BUS_NAME, PPD_OBJECT_PATH),
            (PPD_LEGACY_BUS_NAME, PPD_LEGACY_OBJECT_PATH),
        ] {
            if is_served(&connection, bus_name, object_path).await?.not() {
                continue;
            }
            // Only a name that just answered gets a cached proxy, which
            // `receive_property_changed` needs to produce values.
            let proxy = Proxy::new(&connection, bus_name, object_path, bus_name).await?;
            return Ok::<_, zbus::Error>(Some((connection, proxy)));
        }
        Ok(None)
    };
    match timeout(Duration::from_secs(DBUS_SETUP_TIMEOUT_S), setup).await {
        Ok(Ok(Some((connection, proxy)))) => ConnectOutcome::Connected(connection, proxy),
        Ok(Ok(None)) => ConnectOutcome::NotConnected(NoConnection::Absent),
        Ok(Err(err)) => ConnectOutcome::NotConnected(NoConnection::Failed(err)),
        Err(_) => ConnectOutcome::NotConnected(NoConnection::TimedOut),
    }
}

/// Whether `bus_name` answers for the power profile interface. Caching is off so an unowned name
/// fails on a plain `Get`: the default lazy cache makes zbus warn about `GetAll` on every retry.
async fn is_served(
    connection: &Connection,
    bus_name: &'static str,
    object_path: &'static str,
) -> Result<bool, zbus::Error> {
    // The interface name matches the bus name for both variants.
    let probe: Proxy<'static> = ProxyBuilder::new(connection)
        .destination(bus_name)?
        .path(object_path)?
        .interface(bus_name)?
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    Ok(probe
        .get_property::<String>(ACTIVE_PROFILE_PROPERTY)
        .await
        .is_ok())
}

/// Reads the profile names the daemon offers. An unreadable list is not fatal: the mapping still
/// works, a client just has nothing to populate a picker with.
async fn available_profiles(proxy: &Proxy<'static>) -> Vec<String> {
    let Ok(profiles) = proxy
        .get_property::<Vec<HashMap<String, zbus::zvariant::OwnedValue>>>(PROFILES_PROPERTY)
        .await
    else {
        warn!("Could not read the list of available power profiles.");
        return Vec::new();
    };
    profiles
        .iter()
        .filter_map(|entry| entry.get(PROFILE_NAME_KEY))
        .filter_map(|value| String::try_from(value.clone()).ok())
        // A blank name can never match a mapping, and would only show up as an empty picker row.
        .filter(|profile| profile.is_empty().not())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::sync::Mutex;

    /// Goal: an unmapped profile must resolve to nothing, so an unconfigured system never
    /// activates a Mode by accident.
    /// Methodology: look up a profile that was never mapped.
    #[test]
    fn unmapped_profile_resolves_to_no_mode() {
        let profiles = PowerProfiles::new(HashMap::from([(
            "performance".to_string(),
            "mode-uid-1".to_string(),
        )]));

        assert_eq!(
            profiles.mode_for("performance").as_deref(),
            Some("mode-uid-1")
        );
        assert_eq!(profiles.mode_for("balanced"), None);
        assert_eq!(profiles.mode_for(""), None);
    }

    /// Goal: an empty mapping is the default and must resolve nothing rather than panic.
    /// Methodology: query the default value.
    #[test]
    fn default_profiles_resolve_nothing() {
        let profiles = PowerProfiles::default();

        assert_eq!(profiles.mode_for("balanced"), None);
        assert!(profiles.snapshot().modes.is_empty());
    }

    /// Goal: a mapping edit from the API must be visible to the listener without a restart,
    /// which is the reason the map is shared rather than moved into the sidecar task.
    /// Methodology: clone the handle (as the listener does), then replace through the original.
    #[test]
    fn replaced_modes_are_visible_through_an_existing_clone() {
        let profiles = PowerProfiles::default();
        let listener_view = profiles.clone();

        profiles.set_modes(HashMap::from([(
            "power-saver".to_string(),
            "quiet-mode".to_string(),
        )]));

        assert_eq!(
            listener_view.mode_for("power-saver").as_deref(),
            Some("quiet-mode"),
            "The listener must see mapping edits made through the API"
        );
    }

    /// Goal: the first connect must never activate a Mode, or the listener would fight
    /// `apply_on_boot` at startup. Every later connect must still catch a change that happened
    /// while the listener was disconnected, which is the whole point of reconnecting.
    /// Methodology: run the decision for the seeding connect and for each reconnect case.
    #[test]
    fn the_first_connect_seeds_and_later_ones_catch_up() {
        assert_eq!(
            reconnect_action(false, None, Some("balanced")),
            Reconnect::Seed,
            "The first connect only records the profile"
        );
        assert_eq!(
            reconnect_action(false, None, None),
            Reconnect::Seed,
            "An unreadable profile still counts as seeded, so the next connect is a reconnect"
        );

        assert_eq!(
            reconnect_action(true, Some("balanced"), Some("performance")),
            Reconnect::Changed,
            "A profile changed while disconnected must still be handled"
        );
        assert_eq!(
            reconnect_action(true, None, Some("performance")),
            Reconnect::Changed,
            "A profile that was unreadable at seed time and readable now is a change"
        );

        assert_eq!(
            reconnect_action(true, Some("balanced"), Some("balanced")),
            Reconnect::Unchanged,
            "A reconnect onto the same profile must not re-activate its Mode"
        );
        assert_eq!(
            reconnect_action(true, Some("balanced"), None),
            Reconnect::Unchanged,
            "A daemon that will not report its profile must not look like a change"
        );
    }

    /// Goal: a panic elsewhere must not turn the mapping into a silent "no mapping", which would
    /// stop Mode switching for the rest of the run with nothing in the log.
    /// Methodology: poison the lock from a panicking thread, then read and write through it.
    #[test]
    fn a_poisoned_lock_still_reads_and_writes() {
        let profiles = PowerProfiles::new(HashMap::from([(
            "performance".to_string(),
            "mode-uid-1".to_string(),
        )]));

        let poisoner = profiles.clone();
        let panicked = std::thread::spawn(move || {
            let _guard = poisoner.state.write().unwrap();
            panic!("poison the lock");
        })
        .join();
        assert!(panicked.is_err(), "The helper thread must have panicked");
        assert!(profiles.state.is_poisoned(), "The lock must be poisoned");

        assert_eq!(
            profiles.mode_for("performance").as_deref(),
            Some("mode-uid-1"),
            "A poisoned lock must not hide an existing mapping"
        );
        profiles.set_modes(HashMap::from([(
            "balanced".to_string(),
            "mode-uid-2".to_string(),
        )]));
        assert_eq!(
            profiles.mode_for("balanced").as_deref(),
            Some("mode-uid-2"),
            "A poisoned lock must not silently drop a write"
        );
        assert_eq!(profiles.snapshot().modes.len(), 1);
    }

    /// Goal: prove the real connect path works against a live bus: the bus-name fallback, the
    /// `ActiveProfile` read, and the `Profiles` decode. None of this is exercised by the pure
    /// tests, and the decode of the `a{sv}` array is easy to get wrong.
    /// Methodology: needs a real system bus with power-profiles-daemon or tuned-ppd running, so
    /// it cannot run in CI. Run with `cargo test -- --ignored power_profile`.
    #[test]
    #[ignore = "requires a system D-Bus with a power profile daemon"]
    fn connects_to_a_live_power_profile_daemon() {
        crate::sidecar::ensure_test_handle();
        let observed = crate::rt::test_runtime(async {
            crate::sidecar::handle()
                .run(|| async {
                    let ConnectOutcome::Connected(connection, proxy) = connect().await else {
                        return None;
                    };
                    let active = proxy
                        .get_property::<String>(ACTIVE_PROFILE_PROPERTY)
                        .await
                        .ok();
                    let available = available_profiles(&proxy).await;
                    let _ = connection.close().await;
                    Some((active, available))
                })
                .await
                .expect("sidecar must run the probe")
        });

        let Some((active, available)) = observed else {
            panic!("No power profile daemon answered on the system bus.");
        };
        let active = active.expect("ActiveProfile must be readable");
        assert!(
            available.contains(&active),
            "The active profile '{active}' must appear in the available list {available:?}"
        );
        assert!(
            available.iter().all(|profile| profile.is_empty().not()),
            "Decoded profile names must not be empty: {available:?}"
        );
        println!("active: {active}, available: {available:?}");
    }

    /// Captures log records so a test can assert what was logged, and at which level.
    struct CapturingLogger;

    static CAPTURED_LOGS: Mutex<Vec<(log::Level, String)>> = Mutex::new(Vec::new());

    impl log::Log for CapturingLogger {
        fn enabled(&self, _metadata: &log::Metadata) -> bool {
            true
        }

        fn log(&self, record: &log::Record) {
            CAPTURED_LOGS
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((record.level(), record.args().to_string()));
        }

        fn flush(&self) {}
    }

    /// Installs the capturing logger and empties it, so `captured` returns only what follows.
    fn capture_logs() {
        // Failure means a logger is already installed, which still captures nothing of ours.
        let _ = log::set_boxed_logger(Box::new(CapturingLogger));
        log::set_max_level(log::LevelFilter::Info);
        CAPTURED_LOGS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    fn captured() -> Vec<(log::Level, String)> {
        CAPTURED_LOGS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Goal: probing a bus name nobody owns must stay silent, where a lazily cached proxy made
    /// zbus warn on every retry.
    /// Methodology: probe a name that cannot exist while capturing the log. Needs a system bus,
    /// not a power profile daemon; without a bus there is nothing to probe.
    #[test]
    #[serial]
    fn probing_an_absent_bus_name_stays_silent() {
        const ABSENT_BUS_NAME: &str = "org.coolercontrol.NoSuchPowerProfileDaemon";
        const ABSENT_OBJECT_PATH: &str = "/org/coolercontrol/NoSuchPowerProfileDaemon";

        capture_logs();
        crate::sidecar::ensure_test_handle();
        let probed = crate::rt::test_runtime(async {
            crate::sidecar::handle()
                .run(|| async {
                    let connection = Connection::system().await.ok()?;
                    let served = is_served(&connection, ABSENT_BUS_NAME, ABSENT_OBJECT_PATH).await;
                    let _ = connection.close().await;
                    Some(served.is_ok_and(|served| served))
                })
                .await
                .expect("sidecar must run the probe")
        });

        let logs = captured();
        let Some(served) = probed else {
            println!("No system bus reachable, nothing was probed.");
            return;
        };
        assert!(served.not(), "A name nobody owns can never be served");
        assert!(
            logs.iter()
                .all(|(_, line)| line.contains("properties cache").not()),
            "Probing an absent bus name must not warn about the property cache: {logs:?}"
        );
    }

    /// Goal: a daemon that was never there is expected and must only inform, while a daemon that
    /// answered and then went away must warn. Either way the retry runs every 30s for the life of
    /// the daemon, so an outage must cost exactly one line.
    /// Methodology: drive the outage reporting through both sequences with the log captured.
    #[test]
    #[serial]
    fn an_outage_is_reported_once_at_the_level_its_history_earns() {
        capture_logs();
        let mut outage = OutageLog::default();

        outage.not_connected(&NoConnection::Absent);
        outage.not_connected(&NoConnection::Absent);
        outage.not_connected(&NoConnection::TimedOut);
        let never_connected = captured();
        assert_eq!(
            never_connected.len(),
            1,
            "A retry must not repeat the line: {never_connected:?}"
        );
        assert_eq!(
            never_connected[0].0,
            log::Level::Info,
            "A power profile daemon that was never there is not a fault"
        );

        capture_logs();
        outage.connected();
        outage.lost();
        outage.not_connected(&NoConnection::Absent);
        let after_a_loss = captured();
        assert_eq!(
            after_a_loss.len(),
            1,
            "A failed reconnect must not repeat the outage line: {after_a_loss:?}"
        );
        assert_eq!(
            after_a_loss[0].0,
            log::Level::Warn,
            "A daemon that answered and then went away is worth a warning"
        );
        assert!(
            after_a_loss[0].1.contains("Lost the connection"),
            "The warning must say what happened: {after_a_loss:?}"
        );
    }

    /// Goal: `CC_DBUS` gates this listener the same way it gates the sleep listener, so one
    /// switch disables all dbus use.
    /// Methodology: resolve each documented form and read the gate back. Setting the variable
    /// for real would write to the process environment while the rest of the suite reads it
    /// from other threads, so the resolver is driven directly.
    #[test]
    fn dbus_env_var_gates_the_listener() {
        assert_eq!(ENV_DBUS, "CC_DBUS");
        assert!(dbus_listener_enabled_from(None), "Absent means enabled");

        for enabled in ["1", "ON", "on", "anything-else"] {
            assert!(
                dbus_listener_enabled_from(Some(enabled)),
                "'{enabled}' must enable"
            );
        }
        for disabled in ["0", "OFF", "off"] {
            assert!(
                dbus_listener_enabled_from(Some(disabled)).not(),
                "'{disabled}' must disable"
            );
        }
    }
}
