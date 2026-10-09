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
use zbus::fdo::DBusProxy;
use zbus::names::BusName;
use zbus::proxy::{Builder as ProxyBuilder, CacheProperties};
use zbus::zvariant::OwnedValue;
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

impl PowerProfileState {
    /// Swaps in a list of offered profiles. True when it differs from the one held, order
    /// included, since the order is what a client renders.
    fn replace_available(&mut self, available: Vec<String>) -> bool {
        debug_assert!(
            available.iter().all(|profile| profile.is_empty().not()),
            "Blank profile names are dropped while decoding"
        );
        if self.available == available {
            return false;
        }
        self.available = available;
        true
    }
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

    /// Records what a connect found on the bus. A value that could not be read keeps the one
    /// already held: a single failed read must not blank the list, which is how a client decides
    /// to hide the feature. Returns true when the list of offered profiles changed.
    pub fn set_observed(&self, available: Option<Vec<String>>, active: Option<String>) -> bool {
        debug_assert!(
            active
                .as_ref()
                .is_none_or(|profile| profile.is_empty().not()),
            "A blank profile name can never match a real profile"
        );
        let mut state = self.write_state();
        if active.is_some() {
            state.active = active;
        }
        let Some(available) = available else {
            return false;
        };
        state.replace_available(available)
    }

    /// Records the profiles the daemon offers now. Returns true when the list changed.
    pub fn set_available(&self, available: Vec<String>) -> bool {
        self.write_state().replace_available(available)
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
///
/// `boot_settings_applied` is `None` when `apply_on_boot` is off, see `Listener`.
pub fn start(
    system_event_handle: SystemEventHandle,
    mode_handle: ModeHandle,
    profiles: PowerProfiles,
    boot_settings_applied: Option<CancellationToken>,
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
        boot_settings_applied,
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
    /// First connect: record the profile and bring its Mode in line. Not a change, so nothing is
    /// broadcast.
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
    /// Cancelled once the saved settings are back on the devices at boot. `None` when
    /// `apply_on_boot` is off: the daemon then writes nothing at startup, a Mode included.
    boot_settings_applied: Option<CancellationToken>,
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
        // Deliberately not cleared while disconnected: a client hides the feature on an empty
        // list, which would put an existing mapping out of reach for the length of an outage.
        let changed = self
            .profiles
            .set_observed(available_profiles(proxy).await, observed.clone());
        self.log_available(changed);
        let mut active_changes = proxy
            .receive_property_changed::<String>(ACTIVE_PROFILE_PROPERTY)
            .await;
        // A daemon can change what it offers without restarting.
        let mut profiles_changes = proxy
            .receive_property_changed::<OwnedValue>(PROFILES_PROPERTY)
            .await;
        self.catch_up(observed).await;
        loop {
            tokio::select! {
                () = self.run_token.cancelled() => return true,
                Some(change) = active_changes.next() => {
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
                Some(_) = profiles_changes.next() => self.refresh_available(proxy).await,
                else => return false,
            }
        }
    }

    /// Handles a signalled `Profiles`: the daemon offers a different set of profiles now.
    async fn refresh_available(&self, proxy: &Proxy<'static>) {
        let Some(available) = available_profiles(proxy).await else {
            return;
        };
        let changed = self.profiles.set_available(available);
        self.log_available(changed);
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
                self.activate_startup_mode().await;
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

    /// Activates the Mode mapped to the profile found on the first connect, since the profile
    /// may have changed while the daemon was not running. An already active Mode is left as is.
    ///
    /// Waits for the saved settings first: they are applied concurrently at boot and would
    /// overwrite the Mode's channels if they landed after it. There is no timeout because going
    /// ahead early is exactly that race.
    async fn activate_startup_mode(&self) {
        debug_assert!(
            self.seeded,
            "The startup Mode belongs to the first connect, which seeds before it activates"
        );
        let Some(boot_settings_applied) = self.boot_settings_applied.as_ref() else {
            return;
        };
        tokio::select! {
            () = self.run_token.cancelled() => return,
            () = boot_settings_applied.cancelled() => {},
        }
        debug_assert!(
            boot_settings_applied.is_cancelled(),
            "The startup Mode must never be activated ahead of the boot settings"
        );
        let Some(profile) = self.current.as_deref() else {
            return;
        };
        // Read after the wait, so a mapping edited in the meantime is the one that counts.
        let Some(mode_uid) = self.profiles.mode_for(profile) else {
            return;
        };
        if let Err(err) = self.mode_handle.activate(mode_uid.clone()).await {
            error!("Failed to activate Mode {mode_uid} for the startup power profile: {err}");
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

    /// Says so when the offered profiles changed. Filling the list on the first connect is not a
    /// change.
    fn log_available(&self, changed: bool) {
        if changed.not() {
            return;
        }
        if self.seeded.not() {
            return;
        }
        info!(
            "Available power profiles changed: {}",
            self.profiles.snapshot().available.join(", ")
        );
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

/// Whether `bus_name` answers for the power profile interface.
///
/// The bus is asked for an owner first. A call sent to an unowned name makes the bus start the
/// daemon behind it, and this listener only ever observes. The probe itself reads uncached: the
/// default lazy cache makes zbus warn about `GetAll` when a name does not serve the interface.
async fn is_served(
    connection: &Connection,
    bus_name: &'static str,
    object_path: &'static str,
) -> Result<bool, zbus::Error> {
    let bus = DBusProxy::builder(connection)
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    if bus
        .name_has_owner(BusName::try_from(bus_name)?)
        .await?
        .not()
    {
        return Ok(false);
    }
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

/// Reads the profile names the daemon offers, `None` when the list cannot be read. That is not
/// fatal: the mapping still works, and a client keeps whatever list it had.
async fn available_profiles(proxy: &Proxy<'static>) -> Option<Vec<String>> {
    let Ok(profiles) = proxy
        .get_property::<Vec<HashMap<String, OwnedValue>>>(PROFILES_PROPERTY)
        .await
    else {
        warn!("Could not read the list of available power profiles.");
        return None;
    };
    Some(profile_names(&profiles))
}

/// Picks the names out of the `Profiles` array of dicts.
fn profile_names(profiles: &[HashMap<String, OwnedValue>]) -> Vec<String> {
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
    use crate::calibration::{CalibrationStore, FanStateMap};
    use crate::config::Config;
    use crate::engine::main::Engine;
    use crate::modes::ModeController;
    use crate::overrides::OverridesController;
    use crate::paths;
    use crate::repositories::repository::Repositories;
    use crate::AllDevices;
    use serial_test::serial;
    use std::rc::Rc;

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

    /// Goal: the first connect must be told apart from a reconnect, since it is not a profile
    /// change and must not be broadcast as one. Every later connect must still catch a change
    /// that happened while the listener was disconnected, which is the whole point of
    /// reconnecting.
    /// Methodology: run the decision for the seeding connect and for each reconnect case.
    #[test]
    fn the_first_connect_seeds_and_later_ones_catch_up() {
        assert_eq!(
            reconnect_action(false, None, Some("balanced")),
            Reconnect::Seed,
            "The first connect is a seed, not a change"
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

    fn profile_list(names: &[&str]) -> Vec<String> {
        names.iter().map(ToString::to_string).collect()
    }

    /// Goal: one failed read must not blank the list of offered profiles or the active profile.
    /// A client hides the feature on an empty list, so a read that races a restarting daemon
    /// would otherwise put an existing mapping out of reach.
    /// Methodology: record a full observation, then ones where either value could not be read,
    /// and read the snapshot back after each.
    #[test]
    fn an_unreadable_value_keeps_the_one_already_held() {
        let profiles = PowerProfiles::default();
        profiles.set_observed(
            Some(profile_list(&["power-saver", "balanced"])),
            Some("balanced".to_string()),
        );

        assert!(
            profiles.set_observed(None, None).not(),
            "Nothing read means nothing changed"
        );
        let snapshot = profiles.snapshot();
        assert_eq!(snapshot.available, ["power-saver", "balanced"]);
        assert_eq!(snapshot.active.as_deref(), Some("balanced"));

        profiles.set_observed(None, Some("power-saver".to_string()));
        let snapshot = profiles.snapshot();
        assert_eq!(
            snapshot.available,
            ["power-saver", "balanced"],
            "An unreadable list keeps the one held"
        );
        assert_eq!(snapshot.active.as_deref(), Some("power-saver"));

        profiles.set_observed(Some(profile_list(&["balanced"])), None);
        let snapshot = profiles.snapshot();
        assert_eq!(snapshot.available, ["balanced"]);
        assert_eq!(
            snapshot.active.as_deref(),
            Some("power-saver"),
            "An unreadable active profile keeps the one held"
        );
    }

    /// Goal: a power profile daemon restarted with another set of profiles must replace the list
    /// held, and only a real difference counts as a change, since that is what gets reported.
    /// Methodology: record the same list twice, then one more profile, then the same names in
    /// another order, through both setters, and read the answer and the snapshot back.
    #[test]
    fn only_a_different_list_of_profiles_counts_as_a_change() {
        let two = profile_list(&["power-saver", "balanced"]);
        let three = profile_list(&["power-saver", "balanced", "performance"]);
        let reordered = profile_list(&["performance", "balanced", "power-saver"]);
        let profiles = PowerProfiles::default();

        assert!(
            profiles.set_available(two.clone()),
            "Filling an empty list is a change"
        );
        assert!(
            profiles.set_available(two.clone()).not(),
            "The same list again is not"
        );
        assert!(
            profiles.set_observed(Some(two), None).not(),
            "Nor is it when a reconnect reads it"
        );

        assert!(
            profiles.set_observed(Some(three.clone()), None),
            "A daemon back with one more profile is a change"
        );
        assert_eq!(profiles.snapshot().available, three);

        assert!(
            profiles.set_available(reordered.clone()),
            "The order is what a client renders, so it counts"
        );
        assert_eq!(profiles.snapshot().available, reordered);
    }

    /// Goal: `Profiles` is an array of dicts of which only the names are wanted, and one entry
    /// without a usable name must not cost the rest of the list.
    /// Methodology: decode entries shaped like the ones `power-profiles-daemon` sends, mixed
    /// with one that has no name, one whose name is not a string, and one whose name is blank.
    #[test]
    fn profile_names_are_decoded_and_unusable_entries_skipped() {
        fn owned(value: zbus::zvariant::Value<'_>) -> OwnedValue {
            value.try_to_owned().unwrap()
        }
        fn named(name: &str) -> HashMap<String, OwnedValue> {
            HashMap::from([
                (PROFILE_NAME_KEY.to_string(), owned(name.into())),
                ("Driver".to_string(), owned("placeholder".into())),
            ])
        }
        let profiles = [
            named("power-saver"),
            HashMap::from([("Driver".to_string(), owned("placeholder".into()))]),
            HashMap::from([(PROFILE_NAME_KEY.to_string(), owned(7_u32.into()))]),
            named(""),
            named("balanced"),
        ];

        assert_eq!(profile_names(&profiles), ["power-saver", "balanced"]);
        assert!(profile_names(&[]).is_empty());
    }

    const STARTUP_PROFILE: &str = "performance";

    /// A controller with no devices and two Modes, returned as `(controller, idle, active)`.
    /// Nothing reaches hardware, so only the choice of active Mode is exercised.
    async fn controller_with_two_modes(profiles: &PowerProfiles) -> (Rc<ModeController>, UID, UID) {
        let config = Rc::new(Config::init_default_config().unwrap());
        let all_devices: AllDevices = Rc::new(HashMap::new());
        let engine = Rc::new(Engine::new(
            Rc::clone(&all_devices),
            &Rc::new(Repositories::default()),
            Rc::clone(&config),
            Rc::new(CalibrationStore::empty()),
            Rc::new(FanStateMap::new()),
            Rc::new(OverridesController::empty()),
        ));
        // Callers hold the `modes_file` lock; a prior modes test may leave the file unparseable.
        if let Err(err) = std::fs::remove_file(paths::mode_config_file()) {
            assert_eq!(err.kind(), std::io::ErrorKind::NotFound, "{err}");
        }
        let controller = Rc::new(
            ModeController::init(config, all_devices, engine, profiles.clone())
                .await
                .unwrap(),
        );
        // A new Mode becomes the active one, so the last one created is active.
        let idle = controller.create_mode("Idle".to_string()).await.unwrap();
        let active = controller.create_mode("Active".to_string()).await.unwrap();
        assert_eq!(
            controller.get_active_modes().current_mode_uid.as_ref(),
            Some(&active.uid)
        );
        (controller, idle.uid, active.uid)
    }

    fn map_startup_profile_to(profiles: &PowerProfiles, mode_uid: &UID) {
        profiles.set_modes(HashMap::from([(
            STARTUP_PROFILE.to_string(),
            mode_uid.clone(),
        )]));
    }

    fn unconnected_listener(
        mode_handle: ModeHandle,
        profiles: PowerProfiles,
        boot_settings_applied: Option<CancellationToken>,
    ) -> Listener {
        let run_token = CancellationToken::new();
        Listener {
            system_event_handle: SystemEventHandle::new(run_token.clone()),
            mode_handle,
            profiles,
            boot_settings_applied,
            run_token,
            current: None,
            seeded: false,
        }
    }

    /// Goal: a profile changed while the daemon was down must still get its Mode, so the first
    /// connect activates the mapped Mode when another one is active. It is not a change, so
    /// nothing is broadcast.
    /// Methodology: map the profile to the idle Mode, run the first connect with the boot
    /// settings already applied, then read the active Mode and the event channel back.
    #[test]
    #[serial(modes_file)]
    fn the_first_connect_activates_the_mapped_mode() {
        crate::rt::test_runtime(async {
            let profiles = PowerProfiles::default();
            let (controller, idle, _) = controller_with_two_modes(&profiles).await;
            map_startup_profile_to(&profiles, &idle);
            let boot_settings_applied = CancellationToken::new();
            boot_settings_applied.cancel();
            let actor_token = CancellationToken::new();
            moro_local::async_scope!(|scope| -> anyhow::Result<()> {
                let mode_handle =
                    ModeHandle::new(Rc::clone(&controller), actor_token.clone(), scope);
                let mut listener =
                    unconnected_listener(mode_handle, profiles, Some(boot_settings_applied));
                let mut events = listener.system_event_handle.broadcaster().subscribe();

                listener.catch_up(Some(STARTUP_PROFILE.to_string())).await;

                assert!(
                    events.try_recv().is_err(),
                    "A first connect is not a change"
                );
                assert_eq!(listener.current.as_deref(), Some(STARTUP_PROFILE));
                // Stops the actor so the scope can finish.
                actor_token.cancel();
                Ok(())
            })
            .await
            .unwrap();

            assert_eq!(controller.get_active_modes().current_mode_uid, Some(idle));
        });
    }

    /// Goal: the startup Mode must not be activated while the saved settings are still being
    /// applied, or they would overwrite its channels afterwards.
    /// Methodology: start the first connect with no mapping and the boot settings pending, let
    /// it run, and only then add the mapping and release it. A listener that did not wait has
    /// already found no mapping and activates nothing.
    #[test]
    #[serial(modes_file)]
    fn the_startup_mode_waits_for_the_boot_settings() {
        const YIELDS_FOR_THE_LISTENER_TO_RUN: usize = 8;
        crate::rt::test_runtime(async {
            let profiles = PowerProfiles::default();
            let (controller, idle, active) = controller_with_two_modes(&profiles).await;
            let boot_settings_applied = CancellationToken::new();
            let actor_token = CancellationToken::new();
            moro_local::async_scope!(|scope| -> anyhow::Result<()> {
                let mode_handle =
                    ModeHandle::new(Rc::clone(&controller), actor_token.clone(), scope);
                let mut listener = unconnected_listener(
                    mode_handle,
                    profiles.clone(),
                    Some(boot_settings_applied.clone()),
                );
                let first_connect = scope.spawn(async move {
                    listener.catch_up(Some(STARTUP_PROFILE.to_string())).await;
                });
                for _ in 0..YIELDS_FOR_THE_LISTENER_TO_RUN {
                    crate::rt::yield_now().await;
                }
                assert_eq!(
                    controller.get_active_modes().current_mode_uid.as_ref(),
                    Some(&active),
                    "Nothing is activated while the boot settings are pending"
                );

                map_startup_profile_to(&profiles, &idle);
                boot_settings_applied.cancel();
                first_connect.await;

                actor_token.cancel();
                Ok(())
            })
            .await
            .unwrap();

            assert_eq!(controller.get_active_modes().current_mode_uid, Some(idle));
        });
    }

    /// Goal: a shutdown must not be held up by boot settings that never finish applying, and
    /// must not activate the startup Mode on the way out.
    /// Methodology: map the profile to the idle Mode, start the first connect with the boot
    /// settings pending, then cancel the listener's run token and give it a bounded number of
    /// turns. A flag set once the connect returns is read back instead of awaiting the task, so
    /// a listener that keeps waiting fails the test rather than hanging it.
    #[test]
    #[serial(modes_file)]
    fn a_shutdown_ends_the_wait_for_the_boot_settings() {
        const YIELDS_FOR_THE_LISTENER_TO_RUN: usize = 8;
        crate::rt::test_runtime(async {
            let profiles = PowerProfiles::default();
            let (controller, idle, active) = controller_with_two_modes(&profiles).await;
            map_startup_profile_to(&profiles, &idle);
            let boot_settings_applied = CancellationToken::new();
            let actor_token = CancellationToken::new();
            let returned = Rc::new(std::cell::Cell::new(false));
            moro_local::async_scope!(|scope| -> anyhow::Result<()> {
                let mode_handle =
                    ModeHandle::new(Rc::clone(&controller), actor_token.clone(), scope);
                let mut listener = unconnected_listener(
                    mode_handle,
                    profiles.clone(),
                    Some(boot_settings_applied.clone()),
                );
                let run_token = listener.run_token.clone();
                let first_connect_returned = Rc::clone(&returned);
                scope.spawn(async move {
                    listener.catch_up(Some(STARTUP_PROFILE.to_string())).await;
                    first_connect_returned.set(true);
                });
                for _ in 0..YIELDS_FOR_THE_LISTENER_TO_RUN {
                    crate::rt::yield_now().await;
                }
                assert!(
                    returned.get().not(),
                    "The first connect waits while the boot settings are pending"
                );

                run_token.cancel();
                for _ in 0..YIELDS_FOR_THE_LISTENER_TO_RUN {
                    crate::rt::yield_now().await;
                }
                assert!(
                    returned.get(),
                    "A shutdown must end the wait for the boot settings"
                );
                assert!(
                    boot_settings_applied.is_cancelled().not(),
                    "The wait ended on the shutdown, not on the boot settings"
                );

                actor_token.cancel();
                Ok(())
            })
            .await
            .unwrap();

            assert_eq!(
                controller.get_active_modes().current_mode_uid,
                Some(active),
                "Nothing is activated on the way out"
            );
        });
    }

    /// Goal: with `apply_on_boot` off the daemon writes nothing at startup, so the first connect
    /// must leave the active Mode alone while still recording the profile.
    /// Methodology: map the profile to the idle Mode and run the first connect with no boot
    /// signal, which is how `apply_on_boot = false` reaches the listener.
    #[test]
    #[serial(modes_file)]
    fn the_first_connect_activates_nothing_without_apply_on_boot() {
        crate::rt::test_runtime(async {
            let profiles = PowerProfiles::default();
            let (controller, idle, active) = controller_with_two_modes(&profiles).await;
            map_startup_profile_to(&profiles, &idle);
            let actor_token = CancellationToken::new();
            moro_local::async_scope!(|scope| -> anyhow::Result<()> {
                let mode_handle =
                    ModeHandle::new(Rc::clone(&controller), actor_token.clone(), scope);
                let mut listener = unconnected_listener(mode_handle, profiles, None);

                listener.catch_up(Some(STARTUP_PROFILE.to_string())).await;

                assert_eq!(listener.current.as_deref(), Some(STARTUP_PROFILE));
                actor_token.cancel();
                Ok(())
            })
            .await
            .unwrap();

            assert_eq!(controller.get_active_modes().current_mode_uid, Some(active));
        });
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
        let available = available.expect("Profiles must be readable");
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

    /// Goal: probing a bus name nobody owns reports it as not served, without an error.
    /// Methodology: probe a name that cannot exist. Needs a system bus, not a power profile
    /// daemon; without a bus there is nothing to probe.
    #[test]
    fn an_absent_bus_name_is_not_served() {
        const ABSENT_BUS_NAME: &str = "org.coolercontrol.NoSuchPowerProfileDaemon";
        const ABSENT_OBJECT_PATH: &str = "/org/coolercontrol/NoSuchPowerProfileDaemon";

        crate::sidecar::ensure_test_handle();
        let probed = crate::rt::test_runtime(async {
            crate::sidecar::handle()
                .run(|| async {
                    let connection = Connection::system().await.ok()?;
                    let served = is_served(&connection, ABSENT_BUS_NAME, ABSENT_OBJECT_PATH).await;
                    let _ = connection.close().await;
                    Some(served)
                })
                .await
                .expect("sidecar must run the probe")
        });

        let Some(served) = probed else {
            println!("No system bus reachable, nothing was probed.");
            return;
        };
        assert!(
            matches!(served, Ok(false)),
            "A name nobody owns is not served, and asking is not an error: {served:?}"
        );
    }

    /// Goal: the retry runs every 30s for the life of the daemon, so an outage must be reported
    /// once: it stays marked as reported until the connection is back, however it began.
    /// Methodology: drive the outage state through a daemon that was never there, then one that
    /// answered and went away, and read the mark after each step. The log lines themselves are
    /// not asserted.
    #[test]
    fn an_outage_stays_reported_until_the_connection_returns() {
        let mut outage = OutageLog::default();
        assert!(
            outage.reported.not(),
            "Nothing is reported before an outage"
        );

        outage.not_connected(&NoConnection::Absent);
        assert!(
            outage.reported,
            "The first failed connect reports the outage"
        );
        outage.not_connected(&NoConnection::TimedOut);
        assert!(outage.reported, "A retry leaves the outage reported");

        outage.connected();
        assert!(outage.reported.not(), "A connection ends the outage");

        outage.lost();
        assert!(outage.reported, "A lost connection reports a new outage");
        outage.not_connected(&NoConnection::Absent);
        assert!(outage.reported, "A failed reconnect leaves it reported");

        outage.connected();
        assert!(outage.reported.not(), "A reconnect ends that outage too");
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
