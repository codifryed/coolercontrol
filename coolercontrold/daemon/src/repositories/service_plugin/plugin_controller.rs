// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::api::CCError;
use crate::cc_fs;
use crate::config::Config;
use crate::paths;
use crate::repositories::service_plugin::service_management::manager::{
    Manager, ServiceManager, ServiceStatus,
};
use crate::repositories::service_plugin::service_management::{ServiceId, ServiceIdExt};
use crate::repositories::service_plugin::service_manifest::{ServiceManifest, ServiceType};
use crate::repositories::service_plugin::service_plugin_repo::{
    ServicePluginRepo, CC_PLUGIN_USER, SERVICE_MANIFEST_FILE_NAME,
};
use crate::repositories::service_plugin::trust;
use crate::repositories::utils::{DirectCommand, ShellCommandResult};
use crate::rt::sleep;
use anyhow::{anyhow, Context, Result};
use log::{debug, error, info, warn};
use nix::libc;
use nix::unistd::{Group, User};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions, Permissions};
use std::ops::Not;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

pub const PLUGIN_CONFIG_FILE_NAME: &str = "config.json";
const PLUGIN_UI_DIR_NAME: &str = "ui";
const PLUGIN_CONFIG_FILE_PERMISSIONS: u32 = 0o600;
/// The manifest is root-owned and not plugin-writable. See `secure_plugin_folder`.
const PLUGIN_MANIFEST_PERMISSIONS: u32 = 0o644;
/// The daemon's outbound token and TLS pin: root-owned and readable by root alone.
const PLUGIN_CREDENTIAL_PERMISSIONS: u32 = 0o600;
/// Group-writable with the sticky bit. See `secure_folder_entries`.
const PLUGIN_FOLDER_PERMISSIONS: u32 = 0o1775;
const ROOT_USER: &str = "root";
const CHOWN_BIN: &str = "chown";
const CHOWN_TIMEOUT: Duration = Duration::from_secs(5);
/// Root's entries in a plugin folder, which the handover to the plugin user leaves out.
const ROOT_ENTRY_NAMES: [&str; 3] = [
    SERVICE_MANIFEST_FILE_NAME,
    trust::TOKEN_FILE_NAME,
    trust::PIN_FILE_NAME,
];
/// The most entries of one plugin folder that are handed over. The plugin fills its own
/// folder, so the count is its to choose, and the command line that names them has a limit.
const HANDOVER_ENTRIES_MAX: usize = 1024;
/// How often, and how many times, a just-started plugin is checked before its start counts
/// as a success. Under systemd a plugin that exits stays down for `RestartSec` (1s) before
/// it is restarted, so checks this close together cannot miss it.
#[cfg(not(test))]
const START_SETTLE_INTERVAL: Duration = Duration::from_millis(250);
#[cfg(test)]
const START_SETTLE_INTERVAL: Duration = Duration::from_millis(1);
/// The most rescan problems remembered as reported. Past it, a problem is not reported.
const REPORTED_PROBLEMS_MAX: usize = 64;
const START_SETTLE_CHECKS: u8 = 4;
const _: () = assert!(START_SETTLE_CHECKS > 0);

/// Devices are registered once, when the daemon starts.
const RESTART_TO_LOAD_DEVICES: &str = "Restart the daemon to load this plugin's devices";

#[derive(Clone, Copy)]
enum StartAction {
    Start,
    Restart,
}

/// Generic over the init system only so that tests can script one.
pub struct PluginController<M: ServiceManager = Manager> {
    plugins: RefCell<HashMap<ServiceId, ServiceManifest>>,
    /// The plugins found after startup, and those whose manifest gained a service since.
    /// Shared with the repository, which knows neither and so would not stop their services
    /// on shutdown.
    runtime_plugins: Rc<RefCell<Vec<ServiceManifest>>>,
    /// What the last rescan could not load, so that each problem is logged as an error
    /// once and not every time the plugins are listed.
    reported_problems: RefCell<HashSet<String>>,
    config: Option<Rc<Config>>,
    service_manager: M,
    is_systemd: bool,
    is_open_rc: bool,
}

impl PluginController {
    pub fn new(
        service_plugin_repo: &ServicePluginRepo,
        config: Rc<Config>,
        service_manager: Manager,
        is_systemd: bool,
        is_open_rc: bool,
    ) -> Self {
        Self {
            plugins: RefCell::new(service_plugin_repo.get_plugins()),
            runtime_plugins: service_plugin_repo.runtime_plugins(),
            reported_problems: RefCell::new(HashSet::new()),
            config: Some(config),
            service_manager,
            is_systemd,
            is_open_rc,
        }
    }

    /// Create a disabled controller with no plugins and no service manager.
    /// Used when the service plugin repo fails to initialize.
    pub fn new_disabled() -> Self {
        Self {
            plugins: RefCell::new(HashMap::new()),
            runtime_plugins: Rc::new(RefCell::new(Vec::new())),
            reported_problems: RefCell::new(HashSet::new()),
            config: None,
            service_manager: Manager::Disabled,
            is_systemd: false,
            is_open_rc: false,
        }
    }
}

impl<M: ServiceManager> PluginController<M> {
    /// Registers plugins whose folder appeared after the daemon started.
    ///
    /// A known plugin is left alone here: its manifest is re-read when it is started.
    pub async fn discover_plugins(&self) {
        // Without a config the plugin system failed to initialize.
        if self.config.is_none() {
            return;
        }
        self.discover_plugins_in(paths::plugins_dir()).await;
    }

    async fn discover_plugins_in(&self, plugins_dir: &Path) {
        let (found, problems) = ServicePluginRepo::scan_service_manifests_in(plugins_dir).await;
        for problem in self.unreported_problems(problems) {
            error!("{problem}");
        }
        for (service_id, manifest) in found {
            debug_assert_eq!(service_id, manifest.id, "a scan keys a plugin by its id");
            if self.plugins.borrow().contains_key(&service_id) {
                continue;
            }
            info!("Found new plugin: {service_id}");
            self.runtime_plugins.borrow_mut().push(manifest.clone());
            self.register(manifest);
        }
    }

    /// The problems of a rescan that the one before it did not have, which makes them news.
    ///
    /// The plugins are listed on every page load and after every plugin action, so a broken
    /// manifest would otherwise be logged as an error each time. One that was fixed and
    /// broke again is news again.
    fn unreported_problems(&self, mut problems: Vec<String>) -> Vec<String> {
        // Sorted, so that with too many of them the same ones are kept each time.
        problems.sort_unstable();
        for problem in problems.iter().skip(REPORTED_PROBLEMS_MAX) {
            debug!("{problem}");
        }
        problems.truncate(REPORTED_PROBLEMS_MAX);
        let mut reported = self.reported_problems.borrow_mut();
        let mut unreported = Vec::with_capacity(problems.len());
        for problem in &problems {
            if reported.contains(problem) {
                debug!("{problem}");
            } else {
                unreported.push(problem.clone());
            }
        }
        *reported = problems.into_iter().collect();
        assert!(reported.len() <= REPORTED_PROBLEMS_MAX);
        unreported
    }

    fn is_runtime_plugin(&self, plugin_id: &str) -> bool {
        self.runtime_plugins
            .borrow()
            .iter()
            .any(|manifest| manifest.id == plugin_id)
    }

    /// Re-reads an integration plugin's manifest, so that starting it applies what is on
    /// disk now rather than what was there when the daemon started.
    ///
    /// The file can be trusted this late because only root can change or replace it: see
    /// `secure_folder_entries`. A device plugin is skipped, since its devices are registered
    /// once at startup and a changed manifest could not be applied to them.
    async fn reload_manifest(&self, plugin_id: &str) -> Result<()> {
        let registered = self.manifest(plugin_id)?;
        if registered.service_type != ServiceType::Integration {
            return Ok(());
        }
        let reloaded = ServicePluginRepo::read_manifest(&registered.path).await?;
        ensure_same_plugin(&registered, &reloaded)?;
        if registered.is_managed() && reloaded.is_managed().not() {
            // A plugin registered without a service has none that can be stopped, so the
            // one it has now would be left running with nothing to control it. A failure
            // returns before anything is registered, which keeps the plugin controllable.
            self.service_manager
                .remove(&registered.id)
                .await
                .context("Removing the service of a plugin whose manifest lost its executable")?;
        }
        self.track_service(&registered, &reloaded);
        self.register(reloaded);
        Ok(())
    }

    /// Keeps `runtime_plugins` holding every service the repository would not stop.
    fn track_service(&self, registered: &ServiceManifest, reloaded: &ServiceManifest) {
        // `ensure_same_plugin` has passed: tracking another plugin's service under this
        // one would stop the wrong service on shutdown.
        assert_eq!(registered.id, reloaded.id);
        let mut runtime_plugins = self.runtime_plugins.borrow_mut();
        let tracked_index = runtime_plugins
            .iter()
            .position(|manifest| manifest.id == reloaded.id);
        match tracked_index {
            Some(index) if reloaded.is_managed() => runtime_plugins[index] = reloaded.clone(),
            // Its service is gone, see `reload_manifest`.
            Some(index) => {
                runtime_plugins.swap_remove(index);
            }
            // The repository registered this plugin without a service, so it will not
            // stop one.
            None if registered.is_managed().not() && reloaded.is_managed() => {
                runtime_plugins.push(reloaded.clone());
            }
            None => {}
        }
    }

    pub fn register(&self, manifest: ServiceManifest) {
        self.plugins
            .borrow_mut()
            .insert(manifest.id.clone(), manifest);
    }

    pub fn manifests(&self) -> Vec<ServiceManifest> {
        self.plugins.borrow().values().cloned().collect()
    }

    /// A copy, so that no borrow of the plugin list is held across an await.
    fn manifest(&self, plugin_id: &str) -> Result<ServiceManifest> {
        let manifest = self.plugins.borrow().get(plugin_id).cloned();
        Ok(manifest.ok_or_else(|| CCError::NotFound {
            msg: "Plugin not found".to_string(),
        })?)
    }

    pub async fn load_plugin_config_file(&self, plugin_id: &str) -> Result<String> {
        let manifest = self.manifest(plugin_id)?;
        let config_path = manifest.path.join(PLUGIN_CONFIG_FILE_NAME);
        let config_result = cc_fs::read_txt(&config_path).await.with_context(|| {
            format!(
                "Loading Plugin configuration file {}",
                config_path.display()
            )
        });
        match config_result {
            Ok(config) => Ok(config),
            Err(err) => {
                for cause in err.chain() {
                    if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
                        if io_err.kind() == std::io::ErrorKind::NotFound {
                            debug!(
                                "Plugin Config file for {plugin_id} not found. Using empty config file."
                            );
                            return Ok(String::new());
                        }
                    }
                }
                error!(
                    "Error reading Plugin configuration file: {} - {err}",
                    config_path.display()
                );
                Err(err)
            }
        }
    }

    pub async fn save_plugin_config_file(&self, plugin_id: &str, config: String) -> Result<()> {
        let manifest = self.manifest(plugin_id)?;
        let config_path = manifest.path.join(PLUGIN_CONFIG_FILE_NAME);
        cc_fs::write_string(&config_path, config)
            .await
            .with_context(|| {
                format!(
                    "Saving Plugin configuration file: {}",
                    config_path.display()
                )
            })?;
        if manifest.is_managed().not() {
            return Ok(());
        }
        let owner = self.owner(&manifest).map(Owner::resolve);
        if let Err(err) = secure_config_file(&config_path, owner.as_ref()) {
            warn!(
                "Failed to secure plugin config file {}: {err}",
                config_path.display()
            );
        }
        Ok(())
    }

    pub fn get_plugin_ui_dir(&self, plugin_id: &str) -> Result<PathBuf> {
        let ui_dir = self.manifest(plugin_id)?.path.join(PLUGIN_UI_DIR_NAME);
        if ui_dir.exists().not() {
            return Err(CCError::NotFound {
                msg: "Plugin doesn't contain a UI directory".to_string(),
            }
            .into());
        }
        Ok(ui_dir)
    }

    /// Returns the proxy port for a plugin that has `[proxy]` configured, or `None` if not set.
    pub fn get_proxy_port(&self, plugin_id: &str) -> Result<Option<u16>> {
        let manifest = self.manifest(plugin_id)?;
        Ok(manifest.proxy.as_ref().map(|p| p.port))
    }

    /// Start a managed integration plugin's service.
    pub async fn start_plugin(&self, plugin_id: &str) -> Result<()> {
        self.reload_and_bring_up(plugin_id, StartAction::Start)
            .await
            .with_context(|| format!("Starting plugin service: {plugin_id}"))
    }

    /// Stop a managed integration plugin's service.
    pub async fn stop_plugin(&self, plugin_id: &str) -> Result<()> {
        let service_id = self.get_integration_manifest(plugin_id)?.id;
        self.service_manager
            .stop(&service_id)
            .await
            .with_context(|| format!("Stopping plugin service: {plugin_id}"))
    }

    /// Restart a managed integration plugin's service.
    ///
    /// Handed to the init system as one operation. Doing it as a stop then a start leaves a
    /// window where the old process has not gone yet, and starting into that window is what
    /// leaves two of them running.
    pub async fn restart_plugin(&self, plugin_id: &str) -> Result<()> {
        self.reload_and_bring_up(plugin_id, StartAction::Restart)
            .await
            .with_context(|| format!("Restarting plugin service: {plugin_id}"))
    }

    /// Re-reads the manifest of a plugin that has no service to restart.
    ///
    /// A managed integration plugin is refused: restarting it is what re-reads its manifest,
    /// and the only way its service and its manifest stay in step. A device plugin's
    /// manifest is checked but not applied, since its devices were registered at startup.
    pub async fn reload_plugin(&self, plugin_id: &str) -> Result<()> {
        let registered = self.manifest(plugin_id)?;
        if registered.service_type == ServiceType::Device {
            let reloaded = ServicePluginRepo::read_manifest(&registered.path).await?;
            return ensure_same_plugin(&registered, &reloaded);
        }
        if registered.is_managed() {
            return Err(CCError::UserError {
                msg: "Restart this plugin to apply its manifest".to_string(),
            }
            .into());
        }
        self.reload_manifest(plugin_id).await
    }

    /// Get the status of a plugin's service.
    pub async fn get_plugin_status(&self, plugin_id: &str) -> Result<ServiceStatus> {
        let manifest = self.manifest(plugin_id)?;
        if manifest.service_type == ServiceType::Device && self.is_runtime_plugin(plugin_id) {
            return Ok(ServiceStatus::Stopped(Some(
                RESTART_TO_LOAD_DEVICES.to_string(),
            )));
        }
        if manifest.is_managed().not() {
            return Ok(ServiceStatus::Unmanaged);
        }
        let status = self
            .service_manager
            .status(&manifest.id)
            .await
            .with_context(|| format!("Getting plugin service status: {plugin_id}"))?;
        Ok(reported_status(&manifest, status))
    }

    /// Disable a plugin persistently.
    /// Integration plugins have their service stopped immediately.
    /// Device plugins require a daemon restart.
    pub async fn disable_plugin(&self, plugin_id: &str) -> Result<()> {
        let manifest = self.manifest(plugin_id)?;
        let config = self.config.as_ref().ok_or_else(|| CCError::InternalError {
            msg: "No config available".to_string(),
        })?;
        let mut disabled = config.get_disabled_plugins();
        if disabled.contains(&plugin_id.to_string()).not() {
            disabled.push(plugin_id.to_string());
            config.set_disabled_plugins(&disabled);
            config.save_config_file().await?;
        }
        // Stop integration plugins immediately
        if manifest.service_type == ServiceType::Integration && manifest.is_managed() {
            if let Err(err) = self.stop_plugin(plugin_id).await {
                info!("Could not stop plugin service on disable: {err}");
            }
        }
        Ok(())
    }

    /// Enable a previously disabled plugin.
    /// Integration plugins have their service started immediately.
    /// Device plugins require a daemon restart.
    pub async fn enable_plugin(&self, plugin_id: &str) -> Result<()> {
        let privileged_before = self.manifest(plugin_id)?.privileged;
        let config = self.config.as_ref().ok_or_else(|| CCError::InternalError {
            msg: "No config available".to_string(),
        })?;
        let mut disabled = config.get_disabled_plugins();
        if let Some(pos) = disabled.iter().position(|id| id == plugin_id) {
            disabled.swap_remove(pos);
            config.set_disabled_plugins(&disabled);
            config.save_config_file().await?;
        }
        self.reload_manifest(plugin_id).await.with_context(|| {
            format!("Plugin {plugin_id} is enabled, but its manifest cannot be used")
        })?;
        let manifest = self.manifest(plugin_id)?;
        // Start integration plugins immediately.
        if manifest.service_type != ServiceType::Integration {
            return Ok(());
        }
        if manifest.is_managed().not() {
            return Ok(());
        }
        // Reported rather than swallowed: the plugin is enabled, but it is not running.
        self.bring_up(plugin_id, StartAction::Restart, privileged_before)
            .await
            .with_context(|| {
                format!("Plugin {plugin_id} is enabled, but its service did not start")
            })
    }

    async fn reload_and_bring_up(&self, plugin_id: &str, action: StartAction) -> Result<()> {
        let privileged_before = self.manifest(plugin_id)?.privileged;
        self.reload_manifest(plugin_id).await?;
        self.bring_up(plugin_id, action, privileged_before).await
    }

    /// `privileged_before` is what the plugin was registered with before its manifest was
    /// re-read, which tells whether the user it runs as has changed.
    async fn bring_up(
        &self,
        plugin_id: &str,
        action: StartAction,
        privileged_before: bool,
    ) -> Result<()> {
        let manifest = self.get_integration_manifest(plugin_id)?;
        bring_up_service(
            &self.service_manager,
            &manifest,
            self.owner(&manifest),
            action,
            manifest.privileged != privileged_before,
        )
        .await
    }

    /// The user a managed plugin's files belong to, or `None` where no init system runs it.
    fn owner(&self, manifest: &ServiceManifest) -> Option<&'static str> {
        (self.is_systemd || self.is_open_rc).then_some(if manifest.privileged {
            ROOT_USER
        } else {
            CC_PLUGIN_USER
        })
    }

    /// Check if a plugin is disabled in config.
    pub fn is_plugin_disabled(&self, plugin_id: &str) -> bool {
        self.config
            .as_ref()
            .is_some_and(|c| c.get_disabled_plugins().contains(&plugin_id.to_string()))
    }

    /// Validate that a plugin is a managed integration type and return its manifest.
    fn get_integration_manifest(&self, plugin_id: &str) -> Result<ServiceManifest> {
        let manifest = self.manifest(plugin_id)?;
        if manifest.service_type != ServiceType::Integration {
            return Err(CCError::UserError {
                msg: "Lifecycle control is only available for integration plugins".to_string(),
            }
            .into());
        }
        if manifest.is_managed().not() {
            return Err(CCError::UserError {
                msg: "Plugin is not managed by the service manager".to_string(),
            }
            .into());
        }
        Ok(manifest)
    }
}

/// A re-read manifest may change how a plugin runs, but not which plugin it is: the id
/// names its service, and the type decides whether it has devices registered at startup.
fn ensure_same_plugin(registered: &ServiceManifest, reloaded: &ServiceManifest) -> Result<()> {
    if reloaded.id != registered.id {
        return Err(CCError::UserError {
            msg: format!(
                "The manifest of plugin {} now names it {}. Restart the daemon to load it.",
                registered.id, reloaded.id
            ),
        }
        .into());
    }
    if reloaded.service_type != registered.service_type {
        return Err(CCError::UserError {
            msg: format!(
                "The manifest of plugin {} changed its type. Restart the daemon to apply it.",
                registered.id
            ),
        }
        .into());
    }
    Ok(())
}

/// The status shown for a managed plugin, given what the init system reports.
fn reported_status(manifest: &ServiceManifest, status: ServiceStatus) -> ServiceStatus {
    match status {
        // A managed plugin whose service is not installed is stopped, not unmanaged:
        // starting it installs it.
        ServiceStatus::Unmanaged if manifest.service_type == ServiceType::Integration => {
            ServiceStatus::Stopped(None)
        }
        status => status,
    }
}

/// Stops and removes the services of plugins found while the daemon was running.
pub async fn remove_runtime_services(
    manager: &impl ServiceManager,
    runtime_plugins: &[ServiceManifest],
) {
    for manifest in runtime_plugins {
        if manifest.is_managed().not() {
            continue;
        }
        // Found but never started: no service was installed, so none is reported stopped.
        if let Ok(ServiceStatus::Unmanaged) = manager.status(&manifest.id).await {
            continue;
        }
        remove_service(manager, &manifest.id).await;
    }
}

/// Stops and removes one plugin's service as the daemon shuts down, and says how that went.
pub async fn remove_service(manager: &impl ServiceManager, service_id: &ServiceId) {
    match manager.remove(service_id).await {
        Ok(()) => info!("Plugin Service {service_id} stopped."),
        Err(err) => warn!("Plugin Service {service_id} could not be stopped: {err:#}"),
    }
}

/// Brings a managed plugin's service up from whatever state it is in.
///
/// Does everything a daemon restart does for a plugin, so that restarting the daemon is
/// never the fix: the service definition is installed, the plugin's folder is handed to the
/// user it runs as, and the service has to still be up a moment later.
///
/// `owner_changed` says the plugin now runs as another user than the one its folder was
/// handed to, so a running plugin is stopped for a handover it would otherwise not get.
async fn bring_up_service(
    manager: &impl ServiceManager,
    manifest: &ServiceManifest,
    owner: Option<&str>,
    action: StartAction,
    owner_changed: bool,
) -> Result<()> {
    // `get_integration_manifest` has passed: a device plugin is never started from here.
    debug_assert!(manifest.is_managed());
    debug_assert_eq!(manifest.service_type, ServiceType::Integration);
    let service_id = manifest.id.clone();
    let definition =
        ServicePluginRepo::service_definition(&service_id, manifest).ok_or_else(|| {
            CCError::UserError {
                msg: "Plugin manifest has no executable to manage".to_string(),
            }
        })?;
    // A status that cannot be read counts as running, which only skips the handover.
    let is_down = matches!(
        manager.status(&service_id).await,
        Ok(ServiceStatus::Stopped(_) | ServiceStatus::Unmanaged)
    );
    manager
        .add(definition)
        .await
        .context("Installing the plugin service")?;
    if is_down {
        // Down is not idle: a plugin that keeps exiting is down between two attempts, and
        // its init system would start it again in the middle of the handover. Stopping it
        // cancels that. A unit that is not loaded yet has nothing to cancel.
        if let Err(err) = manager.stop(&service_id).await {
            debug!("Nothing to stop before handing over the folder of {service_id}: {err:#}");
        }
    } else if owner_changed {
        // Left running, the plugin would come back up as a user that cannot use its files.
        manager
            .stop(&service_id)
            .await
            .context("Stopping the plugin to hand its folder to the user it now runs as")?;
    }
    if is_down || owner_changed {
        // Never under a running plugin, see `secure_plugin_folder`.
        secure_plugin_files(manifest, owner).await;
    }
    match action {
        StartAction::Start => manager.start(&service_id).await?,
        StartAction::Restart => manager.restart(&service_id).await?,
    }
    await_running(manager, &service_id).await
}

/// Confirms a service that was just started is still up a moment later.
///
/// An init system reports a start once the process is spawned, so a plugin that exited
/// straight away looked like a success while its supervisor restarted it in a loop.
async fn await_running(manager: &impl ServiceManager, service_id: &ServiceId) -> Result<()> {
    debug_assert!(service_id.is_empty().not(), "a manifest id is never empty");
    for _ in 0..START_SETTLE_CHECKS {
        sleep(START_SETTLE_INTERVAL).await;
        let reason = match manager.status(service_id).await? {
            ServiceStatus::Running => continue,
            ServiceStatus::Stopped(reason) => reason,
            ServiceStatus::Unmanaged => None,
        };
        let reason = reason.map_or_else(String::new, |reason| format!(" ({reason})"));
        return Err(anyhow!(
            "{} stopped right after starting{reason}. Its service log has the cause.",
            service_id.to_service_name()
        ));
    }
    Ok(())
}

/// Hands a managed plugin's folder and config file to `owner`.
///
/// A failure is logged and the plugin is started regardless: one that cannot use its own
/// files says so in its service log.
pub async fn secure_plugin_files(manifest: &ServiceManifest, owner: Option<&str>) {
    let owner = owner.map(Owner::resolve);
    let owner = owner.as_ref();
    if let Err(err) = secure_plugin_folder(&manifest.path, owner).await {
        warn!(
            "Failed to secure plugin folder {}: {err}",
            manifest.path.display()
        );
    }
    let config_path = manifest.path.join(PLUGIN_CONFIG_FILE_NAME);
    // The entry itself: a link the plugin planted counts, wherever it points.
    if config_path.symlink_metadata().is_err() {
        return;
    }
    if let Err(err) = secure_config_file(&config_path, owner) {
        warn!(
            "Failed to secure plugin config file {}: {err}",
            config_path.display()
        );
    }
}

/// Hands what is in the plugin folder to `owner` so the plugin can manage its own runtime
/// files, while `manifest.toml`, the daemon's credentials and the folder itself are root's.
///
/// The manifest declares `privileged`, which decides whether the generated service unit omits
/// `User=` and therefore runs the plugin as root. An unprivileged plugin that owned its own
/// manifest could set that flag and gain root on its next start, so that one file must not
/// follow the rest of the folder, and neither may the right to replace it: see
/// `secure_folder_entries`.
///
/// Neither may be the plugin user's even for a moment. That this plugin is down is no
/// guarantee that nothing runs as its user: every unprivileged plugin runs as the same one,
/// and an init system restarts a plugin that keeps exiting whenever it sees fit. So root's
/// entries are never handed over and taken back, they are left out of the handover.
async fn secure_plugin_folder(path: &Path, owner: Option<&Owner<'_>>) -> Result<()> {
    let Some(owner) = owner else {
        return Ok(());
    };
    // Every step runs even when an earlier one fails, and the first error is reported at
    // the end. A previous run may have left the manifest plugin-owned, which is the exact
    // state this guards against, and the caller only warns on error and starts the plugin
    // anyway: a step that is skipped is a file left readable by the plugin user.
    //
    // The folder first: under a folder that is root's and sticky, nothing handed over
    // below can be moved over one of root's files.
    let entries = secure_folder_entries(path, owner).await;
    let manifest = secure_manifest(path);
    let credentials = secure_daemon_credentials(path);
    let handover = hand_over_entries(path, owner.name).await;
    entries.and(manifest).and(credentials).and(handover)
}

/// Makes the folder itself root's, while the plugin's group can still write in it.
///
/// The owner of a directory can rename or delete any entry in it, whoever owns the entry, so
/// a plugin that owned its folder could swap the root-owned manifest or TLS pin for a file
/// of its own. Under a root-owned folder with the sticky bit, it can only replace what it owns.
async fn secure_folder_entries(plugin_dir: &Path, owner: &Owner<'_>) -> Result<()> {
    // The mode first: a root-owned folder without the group write bit would lock the
    // plugin out of its own files, so a failure here leaves the folder as it was.
    cc_fs::set_permissions(
        plugin_dir,
        Permissions::from_mode(PLUGIN_FOLDER_PERMISSIONS),
    )
    .await?;
    let ids = owner.ids();
    // Root takes the folder even when the plugin's group cannot be found.
    let gid = ids.as_ref().ok().map(|ids| ids.gid);
    std::os::unix::fs::chown(plugin_dir, Some(ROOT_IDS.uid), gid)
        .with_context(|| format!("Taking plugin folder {}", plugin_dir.display()))?;
    ids.map(|_| ())
}

/// Gives `owner` every entry of the plugin folder that is not root's, and what is under it.
async fn hand_over_entries(plugin_dir: &Path, owner: &str) -> Result<()> {
    let (paths, skipped_count) = handover_paths(plugin_dir)?;
    let handover = if paths.is_empty() {
        Ok(())
    } else {
        chown(&paths, owner).await
    };
    if skipped_count > 0 {
        return Err(anyhow!(
            "{skipped_count} entries of {} were not handed to {owner}: the folder holds more \
             than {HANDOVER_ENTRIES_MAX}, or a name is not valid UTF-8",
            plugin_dir.display()
        ));
    }
    handover
}

/// The entries of a plugin folder to hand to the plugin user, and how many were left out
/// because they cannot be named to `chown`. Never the folder itself, nor one of root's entries.
fn handover_paths(plugin_dir: &Path) -> Result<(Vec<String>, usize)> {
    let mut paths = Vec::new();
    let mut skipped_count = 0;
    for entry in cc_fs::read_dir(plugin_dir)? {
        let entry = entry?;
        let file_name = entry.file_name();
        if ROOT_ENTRY_NAMES.iter().any(|name| file_name == *name) {
            continue;
        }
        let path = entry.path();
        match path.to_str() {
            Some(path) if paths.len() < HANDOVER_ENTRIES_MAX => paths.push(path.to_string()),
            _ => skipped_count += 1,
        }
    }
    assert!(paths.len() <= HANDOVER_ENTRIES_MAX);
    Ok((paths, skipped_count))
}

/// Returns the daemon's own credentials for this plugin to root, readable by nobody else.
///
/// The handover gives the rest of the plugin directory to the plugin user, but the
/// token and TLS pin are the *daemon's* credentials for talking to a remote device
/// service, not the plugin's. A plugin has no business reading the bearer token that
/// authenticates this daemon to another machine, and it must not be able to rewrite the
/// pin that decides which certificate is trusted.
fn secure_daemon_credentials(plugin_dir: &Path) -> Result<()> {
    // A failure on one file must not skip hardening the other.
    let mut outcome = Ok(());
    for file_name in [trust::TOKEN_FILE_NAME, trust::PIN_FILE_NAME] {
        let path = plugin_dir.join(file_name);
        let secured = secure_file(&path, PLUGIN_CREDENTIAL_PERMISSIONS, Some(&Owner::ROOT));
        let result = match secured {
            Ok(Entry::File(_) | Entry::Absent) => Ok(()),
            // Left in place, the daemon would read and write its credentials through it. A
            // second name counts: whoever holds the other one reads and rewrites the file.
            Ok(Entry::NotAFile | Entry::HardLinked) => remove_planted(&path),
            Err(err) => Err(err),
        };
        if let Err(err) = result {
            outcome = Err(err);
        }
    }
    outcome
}

/// Returns `manifest.toml` to root and drops any group or world write bit left on it.
fn secure_manifest(plugin_dir: &Path) -> Result<()> {
    let manifest_path = plugin_dir.join(SERVICE_MANIFEST_FILE_NAME);
    let secured = secure_file(
        &manifest_path,
        PLUGIN_MANIFEST_PERMISSIONS,
        Some(&Owner::ROOT),
    )?;
    match secured {
        Entry::File(_) | Entry::Absent => Ok(()),
        Entry::NotAFile => Err(anyhow!("{} is not a regular file", manifest_path.display())),
        Entry::HardLinked => Err(refused_hard_link(&manifest_path)),
    }
}

fn secure_config_file(path: &Path, owner: Option<&Owner<'_>>) -> Result<()> {
    match secure_file(path, PLUGIN_CONFIG_FILE_PERMISSIONS, owner)? {
        Entry::File(_) => Ok(()),
        Entry::Absent => Err(anyhow!("{} does not exist", path.display())),
        Entry::NotAFile => remove_planted(path),
        Entry::HardLinked => Err(refused_hard_link(path)),
    }
}

fn refused_hard_link(path: &Path) -> anyhow::Error {
    anyhow!("{} is a hard link, and is left as it is", path.display())
}

/// What stands at a path in a plugin folder.
enum Entry {
    /// A regular file with no other name, opened without following a link.
    File(File),
    Absent,
    /// A symlink or anything else that is not a regular file, and left untouched.
    NotAFile,
    /// A regular file that has another name somewhere, and left untouched.
    HardLinked,
}

/// Opens the regular file at `path` as `options` ask, and never through a link.
///
/// The plugin can create entries in its own folder, so any name in it may be a link the
/// plugin planted to have root work on another file: the manifest, which decides whether
/// it runs as root, or any file on the system. The entry is therefore opened without
/// following a link and checked through that one descriptor. Working on through the same
/// descriptor leaves nothing that can be swapped in between the check and the work.
fn open_entry(path: &Path, options: &mut OpenOptions) -> Result<Entry> {
    let opened = options
        // NOFOLLOW refuses a symlink. NONBLOCK keeps a planted FIFO from hanging the open.
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path);
    let file = match opened {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Entry::Absent),
        Err(err) if err.raw_os_error() == Some(libc::ELOOP) => return Ok(Entry::NotAFile),
        Err(err) => return Err(err).with_context(|| format!("Opening {}", path.display())),
    };
    let metadata = file.metadata()?;
    if metadata.is_file().not() {
        return Ok(Entry::NotAFile);
    }
    // A second name for a file elsewhere: changing this one would change that one.
    if metadata.nlink() != 1 {
        return Ok(Entry::HardLinked);
    }
    Ok(Entry::File(file))
}

/// Sets the mode of the regular file at `path` and, given an owner, its ownership.
fn secure_file(path: &Path, mode: u32, owner: Option<&Owner<'_>>) -> Result<Entry> {
    let entry = open_entry(path, OpenOptions::new().read(true))?;
    if let Entry::File(file) = &entry {
        set_mode_and_owner(file, mode, owner)
            .with_context(|| format!("Securing {}", path.display()))?;
    }
    Ok(entry)
}

fn set_mode_and_owner(file: &File, mode: u32, owner: Option<&Owner<'_>>) -> Result<()> {
    // The mode first, as it needs no owner: a missing plugin user must not leave the
    // file open to others.
    file.set_permissions(Permissions::from_mode(mode))?;
    let Some(owner) = owner else {
        return Ok(());
    };
    let ids = owner.ids()?;
    std::os::unix::fs::fchown(file, Some(ids.uid), Some(ids.gid))?;
    Ok(())
}

/// A user and the group that carries its name, as the numbers ownership is set with.
#[derive(Clone, Copy)]
struct OwnerIds {
    uid: u32,
    gid: u32,
}

const ROOT_IDS: OwnerIds = OwnerIds { uid: 0, gid: 0 };

/// Who a plugin's files are handed to.
///
/// Looked up once for a whole handover and passed along: the lookup reads the system's user
/// database, which can block, and a handover runs on the runtime thread.
struct Owner<'a> {
    name: &'a str,
    /// An error where the system has no such user or group. Kept, so that each step fails
    /// with it only after doing what needs no ids.
    ids: Result<OwnerIds>,
}

impl<'a> Owner<'a> {
    /// Root's ids are fixed, so what is root's is secured without a lookup.
    const ROOT: Owner<'static> = Owner {
        name: ROOT_USER,
        ids: Ok(ROOT_IDS),
    };

    fn resolve(name: &'a str) -> Self {
        assert!(name.is_empty().not(), "ownership needs a user name");
        if name == ROOT_USER {
            return Self::ROOT;
        }
        Self {
            name,
            ids: owner_ids(name),
        }
    }

    fn ids(&self) -> Result<OwnerIds> {
        match &self.ids {
            Ok(ids) => Ok(*ids),
            Err(err) => Err(anyhow!("{err:#}")),
        }
    }
}

/// The uid of `owner` and the gid of the group that carries its name.
fn owner_ids(owner: &str) -> Result<OwnerIds> {
    let user = User::from_name(owner)?.ok_or_else(|| anyhow!("There is no user {owner}"))?;
    let group = Group::from_name(owner)?.ok_or_else(|| anyhow!("There is no group {owner}"))?;
    Ok(OwnerIds {
        uid: user.uid.as_raw(),
        gid: group.gid.as_raw(),
    })
}

/// Removes an entry that stands where a plugin file belongs but is a link, or not a regular
/// file at all.
///
/// Only the entry goes: unlinking a link, symbolic or hard, never touches what it leads to.
fn remove_planted(path: &Path) -> Result<()> {
    std::fs::remove_file(path).with_context(|| {
        format!(
            "Removing {}, which is not a file of its own",
            path.display()
        )
    })?;
    warn!(
        "Removed {}: it was a link or not a regular file, and securing it would have changed \
         whatever it led to.",
        path.display()
    );
    Ok(())
}

/// Builds the `chown` argument vector. Extracted from the I/O so the argument boundaries can be
/// asserted directly: each path must stay a single argument no matter what characters it holds.
fn chown_args(paths: &[String], owner: &str) -> Vec<String> {
    assert!(
        paths.is_empty().not(),
        "chown must never be run without a path"
    );
    assert!(
        owner.is_empty().not(),
        "chown must never be run without an owner"
    );
    let mut args = Vec::with_capacity(paths.len() + 3);
    // -R: recursive, -h: do not follow symlinks (set ownership on the link itself)
    args.push("-Rh".to_string());
    args.push(format!("{owner}:{owner}"));
    // A plugin names its own files, and one that starts with a dash is not an option.
    args.push("--".to_string());
    args.extend(paths.iter().cloned());
    assert_eq!(
        args.len(),
        paths.len() + 3,
        "Each path must stay one argument, whatever characters it holds"
    );
    args
}

/// Runs `chown` as a direct binary, never through a shell.
///
/// Each path is passed as its own argument, so a file name containing shell metacharacters
/// cannot inject a command into this root-run process. The paths come from a directory scan
/// rather than the validated manifest `id`, so they are not otherwise constrained.
async fn chown(paths: &[String], owner: &str) -> Result<()> {
    let mut command = DirectCommand::new(CHOWN_BIN, CHOWN_TIMEOUT);
    for arg in chown_args(paths, owner) {
        command = command.arg(arg);
    }
    match command.run().await {
        ShellCommandResult::Success { .. } => Ok(()),
        ShellCommandResult::Error(stderr) => Err(anyhow!("chown failed: {stderr}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::service_plugin::service_management::manager::ServiceDefinition;
    use crate::repositories::service_plugin::service_manifest::{ConnectionType, EnvVar};
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::os::unix::fs::MetadataExt;

    fn is_root() -> bool {
        nix::unistd::geteuid().is_root()
    }

    /// Answers `status` from a script, repeating its last entry, and records every other call.
    struct FakeManager {
        statuses: RefCell<VecDeque<ServiceStatus>>,
        calls: RefCell<Vec<&'static str>>,
        fails_to_remove: bool,
        /// A file whose mode is noted in `mode_at_stop` when a stop is asked for.
        watched_file: Option<PathBuf>,
        mode_at_stop: RefCell<Option<u32>>,
    }

    impl FakeManager {
        fn new(statuses: impl IntoIterator<Item = ServiceStatus>) -> Self {
            let statuses: VecDeque<ServiceStatus> = statuses.into_iter().collect();
            assert!(statuses.is_empty().not(), "a status script needs an entry");
            Self {
                statuses: RefCell::new(statuses),
                calls: RefCell::new(Vec::new()),
                fails_to_remove: false,
                watched_file: None,
                mode_at_stop: RefCell::new(None),
            }
        }

        fn record(&self, call: &'static str) -> Result<()> {
            self.calls.borrow_mut().push(call);
            Ok(())
        }
    }

    impl ServiceManager for FakeManager {
        async fn add(&self, _service_definition: ServiceDefinition) -> Result<()> {
            self.record("add")
        }

        async fn remove(&self, _service_id: &ServiceId) -> Result<()> {
            if self.fails_to_remove {
                return Err(anyhow!("the service did not stop"));
            }
            self.record("remove")
        }

        async fn start(&self, _service_id: &ServiceId) -> Result<()> {
            self.record("start")
        }

        async fn stop(&self, _service_id: &ServiceId) -> Result<()> {
            if let Some(watched_file) = &self.watched_file {
                let mode = std::fs::metadata(watched_file)?.permissions().mode();
                *self.mode_at_stop.borrow_mut() = Some(mode & 0o777);
            }
            self.record("stop")
        }

        async fn restart(&self, _service_id: &ServiceId) -> Result<()> {
            self.record("restart")
        }

        async fn status(&self, _service_id: &ServiceId) -> Result<ServiceStatus> {
            let mut statuses = self.statuses.borrow_mut();
            if statuses.len() > 1 {
                return Ok(statuses.pop_front().expect("more than one entry"));
            }
            Ok(statuses.front().cloned().expect("never emptied"))
        }
    }

    fn managed_manifest(path: PathBuf) -> ServiceManifest {
        ServiceManifest {
            id: "test-plugin".to_string(),
            service_type: ServiceType::Integration,
            description: None,
            version: None,
            url: None,
            executable: Some(PathBuf::from("/usr/bin/test-plugin")),
            args: Vec::new(),
            envs: Vec::new(),
            address: ConnectionType::None,
            tls: None,
            privileged: false,
            proxy: None,
            path,
        }
    }

    fn write_manifest(plugins_dir: &Path, id: &str, kind: &str, description: &str) {
        let folder = plugins_dir.join(id);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join(SERVICE_MANIFEST_FILE_NAME),
            format!("id = \"{id}\"\ntype = \"{kind}\"\ndescription = \"{description}\"\n"),
        )
        .unwrap();
    }

    /// A controller with a config, whose init system is the given fake.
    fn controller_with(config: Rc<Config>, manager: FakeManager) -> PluginController<FakeManager> {
        PluginController {
            plugins: RefCell::new(HashMap::new()),
            runtime_plugins: Rc::new(RefCell::new(Vec::new())),
            reported_problems: RefCell::new(HashSet::new()),
            config: Some(config),
            service_manager: manager,
            is_systemd: false,
            is_open_rc: false,
        }
    }

    /// A controller that knows one integration plugin, registered as `registered` describes
    /// it, whose folder holds a manifest with a different description and an executable.
    fn controller_with_edited_manifest(
        plugins_dir: &Path,
        registered: impl FnOnce(&mut ServiceManifest),
    ) -> PluginController {
        let controller = PluginController::new_disabled();
        controller.register(edited_manifest(plugins_dir, registered));
        controller
    }

    /// One integration plugin as `registered` describes it, whose folder holds a manifest
    /// with a different description and an executable.
    fn edited_manifest(
        plugins_dir: &Path,
        registered: impl FnOnce(&mut ServiceManifest),
    ) -> ServiceManifest {
        let folder = plugins_dir.join("test-plugin");
        write_manifest(plugins_dir, "test-plugin", "integration", "edited on disk");
        let manifest_file = folder.join(SERVICE_MANIFEST_FILE_NAME);
        let mut content = std::fs::read_to_string(&manifest_file).unwrap();
        content.push_str("executable = \"/usr/bin/test-plugin\"\n");
        std::fs::write(&manifest_file, content).unwrap();
        let mut manifest = managed_manifest(folder);
        manifest.description = Some("as registered".to_string());
        registered(&mut manifest);
        manifest
    }

    /// Goal: enabling a plugin starts it, and a start that fails must come back as an error
    /// instead of an enabled plugin that looks fine and is not running. The plugin still
    /// counts as enabled, which is what the error says.
    /// Method: a disabled plugin and a fake init system that reports it up once, then down.
    #[test]
    fn enable_reports_a_plugin_that_does_not_stay_up() {
        crate::rt::test_runtime(async {
            let plugins_dir = tempfile::tempdir().unwrap();
            let config = Rc::new(Config::init_default_config().unwrap());
            config.set_disabled_plugins(&["test-plugin".to_string()]);
            let manager = FakeManager::new([
                ServiceStatus::Stopped(None),
                ServiceStatus::Running,
                ServiceStatus::Stopped(Some("exit-code".to_string())),
            ]);
            let controller = controller_with(config, manager);
            controller.register(edited_manifest(plugins_dir.path(), |_| {}));
            assert!(controller.is_plugin_disabled("test-plugin"));

            let result = controller.enable_plugin("test-plugin").await;

            let message = format!("{:#}", result.expect_err("the plugin is not running"));
            assert!(message.contains("its service did not start"), "{message}");
            assert!(message.contains("exit-code"), "{message}");
            assert!(controller.is_plugin_disabled("test-plugin").not());
            assert_eq!(
                *controller.service_manager.calls.borrow(),
                ["add", "stop", "restart"]
            );
        });
    }

    /// Goal: a manifest edited under a running daemon has to take effect when its plugin is
    /// started, instead of needing a daemon restart.
    /// Method: a plugin registered with one description and a manifest on disk with another.
    #[test]
    fn a_start_applies_the_manifest_on_disk() {
        crate::rt::test_runtime(async {
            let plugins_dir = tempfile::tempdir().unwrap();
            let controller = controller_with_edited_manifest(plugins_dir.path(), |_| {});

            let result = controller.start_plugin("test-plugin").await;

            assert!(result.is_ok(), "{result:?}");
            assert_eq!(
                description_of(&controller, "test-plugin").as_deref(),
                Some("edited on disk")
            );
            assert!(
                controller.runtime_plugins.borrow().is_empty(),
                "the repository already stops a plugin it registered with a service"
            );
        });
    }

    /// Goal: a manifest that cannot be read must fail the start and say why, rather than
    /// quietly start the plugin as it was registered.
    /// Method: break the manifest's syntax, start, and check what stays registered.
    #[test]
    fn a_start_refuses_a_broken_manifest_and_keeps_the_registered_one() {
        crate::rt::test_runtime(async {
            let plugins_dir = tempfile::tempdir().unwrap();
            let controller = controller_with_edited_manifest(plugins_dir.path(), |_| {});
            let manifest_file = plugins_dir
                .path()
                .join("test-plugin")
                .join(SERVICE_MANIFEST_FILE_NAME);
            std::fs::write(&manifest_file, "id = \"test-plugin\"\ntype = ").unwrap();

            let result = controller.start_plugin("test-plugin").await;

            let message = format!("{:#}", result.expect_err("the manifest is broken"));
            assert!(message.contains("check the syntax"), "{message}");
            assert_eq!(
                description_of(&controller, "test-plugin").as_deref(),
                Some("as registered")
            );
        });
    }

    /// Goal: a plugin with no service has no restart to re-read its manifest, so a reload
    /// has to apply it.
    /// Method: register the plugin without an executable, reload it, and check that the
    /// description on disk is the one registered.
    #[test]
    fn a_reload_applies_the_manifest_of_a_plugin_without_a_service() {
        crate::rt::test_runtime(async {
            let plugins_dir = tempfile::tempdir().unwrap();
            let controller = controller_with_edited_manifest(plugins_dir.path(), |registered| {
                registered.executable = None;
            });

            let result = controller.reload_plugin("test-plugin").await;

            assert!(result.is_ok(), "{result:?}");
            assert_eq!(
                description_of(&controller, "test-plugin").as_deref(),
                Some("edited on disk")
            );
        });
    }

    /// Goal: a managed plugin's manifest must only change together with its service, so a
    /// reload sends it to its restart and leaves what is registered alone.
    /// Method: reload a plugin registered with an executable, and check the error and that
    /// the registered description is unchanged.
    #[test]
    fn a_reload_sends_a_managed_plugin_to_its_restart() {
        crate::rt::test_runtime(async {
            let plugins_dir = tempfile::tempdir().unwrap();
            let controller = controller_with_edited_manifest(plugins_dir.path(), |_| {});

            let result = controller.reload_plugin("test-plugin").await;

            let message = result.expect_err("a restart applies it").to_string();
            assert!(message.contains("Restart this plugin"), "{message}");
            assert_eq!(
                description_of(&controller, "test-plugin").as_deref(),
                Some("as registered")
            );
        });
    }

    /// Goal: a device plugin's devices are registered at startup, so its edited manifest
    /// cannot take effect under a running daemon. A reload still has to say whether the
    /// manifest is usable, which is otherwise only found out by restarting the daemon.
    /// Method: a valid edit is accepted but not registered, a broken one is reported.
    #[test]
    fn a_reload_checks_a_device_plugin_without_applying_it() {
        crate::rt::test_runtime(async {
            let plugins_dir = tempfile::tempdir().unwrap();
            let controller = controller_with_edited_manifest(plugins_dir.path(), |registered| {
                registered.service_type = ServiceType::Device;
            });
            write_manifest(
                plugins_dir.path(),
                "test-plugin",
                "device",
                "edited on disk",
            );

            let valid = controller.reload_plugin("test-plugin").await;
            let manifest_file = plugins_dir
                .path()
                .join("test-plugin")
                .join(SERVICE_MANIFEST_FILE_NAME);
            std::fs::write(&manifest_file, "id = ").unwrap();
            let broken = controller.reload_plugin("test-plugin").await;

            assert!(valid.is_ok(), "{valid:?}");
            assert!(broken.is_err());
            assert_eq!(
                description_of(&controller, "test-plugin").as_deref(),
                Some("as registered")
            );
        });
    }

    /// Goal: the repository stops the services it registered at startup. A plugin it
    /// registered without one, which a manifest edit has since given an executable, would
    /// keep running after the daemon stops unless it is tracked for shutdown.
    /// Method: register the plugin without an executable, start it from a manifest with one.
    #[test]
    fn a_plugin_that_gains_a_service_is_tracked_for_shutdown() {
        crate::rt::test_runtime(async {
            let plugins_dir = tempfile::tempdir().unwrap();
            let controller = controller_with_edited_manifest(plugins_dir.path(), |registered| {
                registered.executable = None;
            });

            let result = controller.start_plugin("test-plugin").await;

            assert!(result.is_ok(), "{result:?}");
            let runtime_plugins = controller.runtime_plugins.borrow();
            assert_eq!(runtime_plugins.len(), 1);
            assert!(runtime_plugins[0].is_managed());
        });
    }

    /// Goal: a manifest edit that flips `privileged` changes the user the plugin runs as, and
    /// its folder still belongs to the old one. The folder is only handed over while the
    /// plugin is down, so a restart has to stop a running plugin first, or it comes back up
    /// unable to read its own files, restart after restart.
    /// Method: a running plugin restarted with and without that edit. The handover resets
    /// the mode of the config file, which needs no root and so shows whether it ran.
    #[test]
    fn a_restart_hands_the_folder_over_when_privileged_changed() {
        crate::rt::test_runtime(async {
            let cases = [
                (true, vec!["add", "stop", "restart"], 0o600),
                (false, vec!["add", "restart"], 0o644),
            ];
            for (privileged_before, expected_calls, expected_mode) in cases {
                let plugins_dir = tempfile::tempdir().unwrap();
                let config = Rc::new(Config::init_default_config().unwrap());
                let manager = FakeManager::new([ServiceStatus::Running]);
                let controller = controller_with(config, manager);
                controller.register(edited_manifest(plugins_dir.path(), |registered| {
                    registered.privileged = privileged_before;
                }));
                let config_path = plugins_dir
                    .path()
                    .join("test-plugin")
                    .join(PLUGIN_CONFIG_FILE_NAME);
                std::fs::write(&config_path, "{}").unwrap();
                std::fs::set_permissions(&config_path, Permissions::from_mode(0o644)).unwrap();

                let result = controller.restart_plugin("test-plugin").await;

                assert!(result.is_ok(), "{result:?}");
                assert_eq!(*controller.service_manager.calls.borrow(), expected_calls);
                let mode = std::fs::metadata(&config_path)
                    .unwrap()
                    .permissions()
                    .mode();
                assert_eq!(mode & 0o777, expected_mode, "{privileged_before}");
            }
        });
    }

    /// Goal: a manifest that lost its `executable` makes its plugin one without a service,
    /// which nothing can stop any more. The service it still has must be removed before
    /// that, or it runs on with no control over it and outlives the daemon.
    /// Method: a plugin found at runtime with a service, a manifest on disk without an
    /// executable, a restart, and what the fake init system was asked to do.
    #[test]
    fn a_manifest_that_loses_its_executable_has_its_service_removed() {
        crate::rt::test_runtime(async {
            let plugins_dir = tempfile::tempdir().unwrap();
            let config = Rc::new(Config::init_default_config().unwrap());
            let manager = FakeManager::new([ServiceStatus::Running]);
            let controller = controller_with(config, manager);
            let registered = edited_manifest(plugins_dir.path(), |_| {});
            controller
                .runtime_plugins
                .borrow_mut()
                .push(registered.clone());
            controller.register(registered);
            write_manifest(
                plugins_dir.path(),
                "test-plugin",
                "integration",
                "no service",
            );

            let result = controller.restart_plugin("test-plugin").await;

            let message = format!("{:#}", result.expect_err("there is nothing to restart"));
            assert!(message.contains("not managed"), "{message}");
            assert_eq!(*controller.service_manager.calls.borrow(), ["remove"]);
            assert!(controller
                .manifest("test-plugin")
                .unwrap()
                .is_managed()
                .not());
            assert!(
                controller.runtime_plugins.borrow().is_empty(),
                "a service that is gone is not one to remove on shutdown"
            );
        });
    }

    /// Goal: a service that could not be removed is still running, so its plugin has to stay
    /// registered with it: that is what keeps stop, restart and disable working on it.
    /// Method: the same edit, with a fake init system whose removal fails.
    #[test]
    fn a_service_that_cannot_be_removed_keeps_its_plugin_managed() {
        crate::rt::test_runtime(async {
            let plugins_dir = tempfile::tempdir().unwrap();
            let config = Rc::new(Config::init_default_config().unwrap());
            let mut manager = FakeManager::new([ServiceStatus::Running]);
            manager.fails_to_remove = true;
            let controller = controller_with(config, manager);
            let registered = edited_manifest(plugins_dir.path(), |_| {});
            controller
                .runtime_plugins
                .borrow_mut()
                .push(registered.clone());
            controller.register(registered);
            write_manifest(
                plugins_dir.path(),
                "test-plugin",
                "integration",
                "no service",
            );

            let result = controller.restart_plugin("test-plugin").await;

            let message = format!("{:#}", result.expect_err("the removal failed"));
            assert!(message.contains("the service did not stop"), "{message}");
            assert!(controller.manifest("test-plugin").unwrap().is_managed());
            assert_eq!(controller.runtime_plugins.borrow().len(), 1);
            assert!(controller.service_manager.calls.borrow().is_empty());
            assert!(controller.stop_plugin("test-plugin").await.is_ok());
        });
    }

    /// Goal: a re-read manifest may not turn a plugin into a different one. The id names
    /// its service, and a device plugin's devices are registered at startup.
    /// Method: compare a manifest with a renamed, a retyped and a merely edited copy.
    #[test]
    fn a_reloaded_manifest_must_describe_the_same_plugin() {
        let registered = managed_manifest(PathBuf::from("/nonexistent/test-plugin"));
        let mut renamed = registered.clone();
        renamed.id = "other-plugin".to_string();
        let mut retyped = registered.clone();
        retyped.service_type = ServiceType::Device;
        let mut edited = registered.clone();
        edited.args = vec!["--verbose".to_string()];

        assert!(ensure_same_plugin(&registered, &renamed).is_err());
        assert!(ensure_same_plugin(&registered, &retyped).is_err());
        assert!(ensure_same_plugin(&registered, &edited).is_ok());
    }

    fn description_of(controller: &PluginController, plugin_id: &str) -> Option<String> {
        controller.manifest(plugin_id).unwrap().description
    }

    /// Goal: a plugin folder added under a running daemon has to show up without a restart,
    /// while a plugin that is already registered keeps the manifest it was started with.
    /// Method: one known and one new plugin on disk, scanned twice.
    #[test]
    fn discovery_registers_new_plugins_and_leaves_known_ones_alone() {
        crate::rt::test_runtime(async {
            let plugins_dir = tempfile::tempdir().unwrap();
            write_manifest(plugins_dir.path(), "known", "integration", "edited on disk");
            write_manifest(plugins_dir.path(), "added", "integration", "new");
            let controller = PluginController::new_disabled();
            let mut known = managed_manifest(plugins_dir.path().join("known"));
            known.id = "known".to_string();
            known.description = Some("as registered".to_string());
            controller.register(known);

            controller.discover_plugins_in(plugins_dir.path()).await;
            controller.discover_plugins_in(plugins_dir.path()).await;

            assert_eq!(controller.manifests().len(), 2);
            assert_eq!(description_of(&controller, "added").as_deref(), Some("new"));
            assert_eq!(
                description_of(&controller, "known").as_deref(),
                Some("as registered")
            );
            let discovered = controller.runtime_plugins.borrow();
            assert_eq!(discovered.len(), 1, "a second scan must not add it again");
            assert_eq!(discovered[0].id, "added");
        });
    }

    /// Goal: the plugins are rescanned every time they are listed, so a folder that cannot be
    /// loaded must be reported once, not on every request. It must not hide the plugins
    /// that do load, and one that is fixed and breaks again is reported again.
    /// Method: one good and one broken manifest, scanned repeatedly. What is reported is
    /// what `unreported_problems` returns, so that and what it remembers are checked.
    #[test]
    fn discovery_reports_a_broken_manifest_once() {
        crate::rt::test_runtime(async {
            let plugins_dir = tempfile::tempdir().unwrap();
            let root = plugins_dir.path();
            write_manifest(root, "good", "integration", "loads");
            write_manifest(root, "broken", "integration", "loads");
            let broken_file = root.join("broken").join(SERVICE_MANIFEST_FILE_NAME);
            let valid = std::fs::read_to_string(&broken_file).unwrap();
            std::fs::write(&broken_file, "id = ").unwrap();
            let controller = PluginController::new_disabled();

            let (_, problems) = ServicePluginRepo::scan_service_manifests_in(root).await;
            assert_eq!(problems.len(), 1, "{problems:?}");
            assert_eq!(controller.unreported_problems(problems.clone()), problems);
            assert!(controller.unreported_problems(problems.clone()).is_empty());
            controller.discover_plugins_in(root).await;
            assert_eq!(
                *controller.reported_problems.borrow(),
                HashSet::from_iter(problems)
            );
            assert_eq!(controller.manifests().len(), 1);

            std::fs::write(&broken_file, valid).unwrap();
            controller.discover_plugins_in(root).await;
            assert!(controller.reported_problems.borrow().is_empty());
            assert_eq!(controller.manifests().len(), 2);

            std::fs::write(&broken_file, "id = ").unwrap();
            let (_, problems) = ServicePluginRepo::scan_service_manifests_in(root).await;
            assert_eq!(controller.unreported_problems(problems).len(), 1);
        });
    }

    /// Goal: what is remembered as reported must not grow with the number of broken plugin
    /// folders, and a problem past the limit must not be reported on every rescan instead.
    /// Method: more problems than the limit, twice.
    #[test]
    fn remembered_scan_problems_are_bounded() {
        let controller = PluginController::new_disabled();
        let problems: Vec<String> = (0..REPORTED_PROBLEMS_MAX + 5)
            .map(|index| format!("problem {index:03}"))
            .collect();

        let first = controller.unreported_problems(problems.clone());
        let second = controller.unreported_problems(problems);

        assert_eq!(first.len(), REPORTED_PROBLEMS_MAX);
        assert!(second.is_empty());
        assert_eq!(
            controller.reported_problems.borrow().len(),
            REPORTED_PROBLEMS_MAX
        );
    }

    /// Goal: a device plugin found under a running daemon cannot be loaded, since devices are
    /// registered at startup, and its status has to say so rather than look broken or idle.
    /// Method: discover one, and compare with a device plugin that was there from the start.
    #[test]
    fn a_discovered_device_plugin_reports_that_it_needs_a_restart() {
        crate::rt::test_runtime(async {
            let plugins_dir = tempfile::tempdir().unwrap();
            write_manifest(plugins_dir.path(), "added", "device", "new");
            let controller = PluginController::new_disabled();
            let mut known = managed_manifest(plugins_dir.path().join("known"));
            known.id = "known".to_string();
            known.service_type = ServiceType::Device;
            known.executable = None;
            controller.register(known);

            controller.discover_plugins_in(plugins_dir.path()).await;

            assert_eq!(
                controller.get_plugin_status("added").await.unwrap(),
                ServiceStatus::Stopped(Some(RESTART_TO_LOAD_DEVICES.to_string()))
            );
            assert_eq!(
                controller.get_plugin_status("known").await.unwrap(),
                ServiceStatus::Unmanaged
            );
        });
    }

    /// Goal: a new integration plugin has no service installed yet, and reporting that as
    /// unmanaged hides the start button that would install it. A device plugin is left as
    /// it was, since nothing starts one while the daemon runs.
    /// Method: map the statuses an init system can report, for one plugin of each type.
    #[test]
    fn an_uninstalled_integration_service_is_reported_stopped() {
        let integration = managed_manifest(PathBuf::from("/nonexistent/test-plugin"));
        let mut device = integration.clone();
        device.service_type = ServiceType::Device;

        assert_eq!(
            reported_status(&integration, ServiceStatus::Unmanaged),
            ServiceStatus::Stopped(None)
        );
        assert_eq!(
            reported_status(&integration, ServiceStatus::Running),
            ServiceStatus::Running
        );
        assert_eq!(
            reported_status(&device, ServiceStatus::Unmanaged),
            ServiceStatus::Unmanaged
        );
    }

    /// Goal: the repository only stops the plugins it registered at startup, so one started
    /// after being found later would outlive the daemon. An unmanaged plugin has no service,
    /// and neither has one that was found but never started, so nothing is stopped for them.
    /// Method: one of each, and what the fake init system is asked to do. Its status
    /// script answers for the two managed plugins in turn.
    #[test]
    fn shutdown_removes_only_the_installed_runtime_services() {
        crate::rt::test_runtime(async {
            let managed = managed_manifest(PathBuf::from("/nonexistent/test-plugin"));
            let mut unmanaged = managed.clone();
            unmanaged.executable = None;
            let never_started = managed.clone();
            let manager = FakeManager::new([ServiceStatus::Running, ServiceStatus::Unmanaged]);

            remove_runtime_services(&manager, &[managed, unmanaged, never_started]).await;

            assert_eq!(*manager.calls.borrow(), ["remove"]);
        });
    }

    /// Goal: starting a plugin whose service was never installed must work, which is what a
    /// daemon restart used to be needed for. Method: a fake init system that does not know
    /// the service yet, and the order of what it is asked to do.
    #[test]
    fn bring_up_installs_the_service_before_starting_it() {
        crate::rt::test_runtime(async {
            let manifest = managed_manifest(PathBuf::from("/nonexistent/test-plugin"));
            let cases = [
                (StartAction::Start, ["add", "stop", "start"]),
                (StartAction::Restart, ["add", "stop", "restart"]),
            ];
            for (action, expected_calls) in cases {
                let manager = FakeManager::new([ServiceStatus::Unmanaged, ServiceStatus::Running]);

                let result = bring_up_service(&manager, &manifest, None, action, false).await;

                assert!(result.is_ok(), "{result:?}");
                assert_eq!(*manager.calls.borrow(), expected_calls);
            }
        });
    }

    /// Goal: a plugin that exits right after it starts must not be reported as started, and
    /// the reason the init system gives has to reach the caller.
    /// Method: the fake reports it up once, then down with a reason.
    #[test]
    fn bring_up_reports_a_plugin_that_stops_right_after_starting() {
        crate::rt::test_runtime(async {
            let manifest = managed_manifest(PathBuf::from("/nonexistent/test-plugin"));
            let manager = FakeManager::new([
                ServiceStatus::Stopped(None),
                ServiceStatus::Running,
                ServiceStatus::Stopped(Some("exit-code".to_string())),
            ]);

            let result =
                bring_up_service(&manager, &manifest, None, StartAction::Start, false).await;

            let message = result.expect_err("the plugin is not running").to_string();
            assert!(message.contains("cc-plugin-test-plugin"), "{message}");
            assert!(message.contains("exit-code"), "{message}");
            assert_eq!(*manager.calls.borrow(), ["add", "stop", "start"]);
        });
    }

    /// Goal: a plugin that keeps exiting is reported as down while its init system waits to
    /// start it again, which would land in the middle of the handover of its folder. So a
    /// plugin that is down is stopped before the handover, and a running one is left alone.
    /// Method: the handover resets the manifest's mode, so the fake init system records
    /// that mode when it is asked to stop: still unset there means the stop came first.
    #[test]
    fn bring_up_stops_a_plugin_that_is_down_before_the_handover() {
        crate::sidecar::ensure_test_handle();
        crate::rt::test_runtime(async {
            let cases = [
                (ServiceStatus::Stopped(None), vec!["add", "stop", "restart"]),
                (ServiceStatus::Unmanaged, vec!["add", "stop", "restart"]),
                (ServiceStatus::Running, vec!["add", "restart"]),
            ];
            for (status_before, expected_calls) in cases {
                let dir = tempfile::tempdir().unwrap();
                let manifest_path = dir.path().join(SERVICE_MANIFEST_FILE_NAME);
                std::fs::write(&manifest_path, "").unwrap();
                std::fs::set_permissions(&manifest_path, Permissions::from_mode(0o666)).unwrap();
                let manifest = managed_manifest(dir.path().to_path_buf());
                let mut manager = FakeManager::new([status_before.clone(), ServiceStatus::Running]);
                manager.watched_file = Some(manifest_path);

                let result = bring_up_service(
                    &manager,
                    &manifest,
                    Some(ROOT_USER),
                    StartAction::Restart,
                    false,
                )
                .await;

                assert!(result.is_ok(), "{result:?}");
                assert_eq!(*manager.calls.borrow(), expected_calls, "{status_before:?}");
                let stopped_at_mode = *manager.mode_at_stop.borrow();
                let expected_mode = (status_before != ServiceStatus::Running).then_some(0o666);
                assert_eq!(stopped_at_mode, expected_mode, "{status_before:?}");
            }
        });
    }

    /// Goal: a plugin folder that was installed after the daemon started is still root's, and
    /// an unprivileged plugin cannot use it until it is handed over. That has to happen on a
    /// start, but never under a running plugin: see `secure_plugin_folder`.
    /// Method: the manifest's mode is reset whenever the folder is secured, and that needs
    /// no root, so it shows whether the handover ran.
    #[test]
    fn bring_up_secures_the_folder_only_while_the_plugin_is_down() {
        crate::sidecar::ensure_test_handle();
        crate::rt::test_runtime(async {
            let cases = [
                (ServiceStatus::Stopped(None), PLUGIN_MANIFEST_PERMISSIONS),
                (ServiceStatus::Unmanaged, PLUGIN_MANIFEST_PERMISSIONS),
                (ServiceStatus::Running, 0o666),
            ];
            for (status_before, expected_mode) in cases {
                let dir = tempfile::tempdir().unwrap();
                let manifest_path = dir.path().join(SERVICE_MANIFEST_FILE_NAME);
                std::fs::write(&manifest_path, "").unwrap();
                std::fs::set_permissions(&manifest_path, Permissions::from_mode(0o666)).unwrap();
                let manifest = managed_manifest(dir.path().to_path_buf());
                let manager = FakeManager::new([status_before.clone(), ServiceStatus::Running]);

                let result = bring_up_service(
                    &manager,
                    &manifest,
                    Some(ROOT_USER),
                    StartAction::Restart,
                    false,
                )
                .await;

                assert!(result.is_ok(), "{result:?}");
                let mode = std::fs::metadata(&manifest_path)
                    .unwrap()
                    .permissions()
                    .mode();
                assert_eq!(mode & 0o777, expected_mode, "{status_before:?}");
            }
        });
    }

    /// Goal: a plugin disabled when the daemon started is skipped during registration, so
    /// nothing writes its service definition. Enabling it later has to install one before
    /// it can start, and the definition has to be the same one registration would have
    /// written. Method: build it from a manifest and pin the fields that matter.
    #[test]
    fn service_definition_is_built_from_the_manifest() {
        let manifest = ServiceManifest {
            id: "test-plugin".to_string(),
            service_type: ServiceType::Integration,
            description: None,
            version: None,
            url: None,
            executable: Some(PathBuf::from("/usr/bin/test-plugin")),
            args: vec!["--verbose".to_string()],
            envs: vec![EnvVar::new("MY_VAR", "value").unwrap()],
            address: ConnectionType::None,
            tls: None,
            privileged: true,
            proxy: None,
            path: PathBuf::from("/etc/coolercontrol/plugins/test-plugin"),
        };

        let definition =
            ServicePluginRepo::service_definition(&"test-plugin".to_string(), &manifest)
                .expect("a manifest with an executable yields a definition");

        assert_eq!(definition.executable, PathBuf::from("/usr/bin/test-plugin"));
        assert_eq!(definition.args, vec!["--verbose".to_string()]);
        // Privileged plugins run as root, so no user is set for the supervisor.
        assert!(definition.username.is_none());
        let envs = definition.envs.expect("log level is always passed through");
        assert!(envs.contains(&EnvVar::new("MY_VAR", "value").unwrap()));
    }

    /// Goal: a manifest without an executable has nothing for an init system to manage, and
    /// must not produce a definition that would be installed as an empty service.
    /// Method: drop the executable and require nothing back.
    #[test]
    fn service_definition_is_absent_without_an_executable() {
        let manifest = ServiceManifest {
            id: "test-plugin".to_string(),
            service_type: ServiceType::Integration,
            description: None,
            version: None,
            url: None,
            executable: None,
            args: Vec::new(),
            envs: Vec::new(),
            address: ConnectionType::None,
            tls: None,
            privileged: false,
            proxy: None,
            path: PathBuf::from("/etc/coolercontrol/plugins/test-plugin"),
        };

        assert!(
            ServicePluginRepo::service_definition(&"test-plugin".to_string(), &manifest).is_none()
        );
    }

    /// Goal: an unprivileged plugin must be supervised as the dedicated plugin user rather
    /// than root. Method: flip `privileged` and check the user that comes back.
    #[test]
    fn unprivileged_plugins_run_as_the_plugin_user() {
        let manifest = ServiceManifest {
            id: "test-plugin".to_string(),
            service_type: ServiceType::Integration,
            description: None,
            version: None,
            url: None,
            executable: Some(PathBuf::from("/usr/bin/test-plugin")),
            args: Vec::new(),
            envs: Vec::new(),
            address: ConnectionType::None,
            tls: None,
            privileged: false,
            proxy: None,
            path: PathBuf::from("/etc/coolercontrol/plugins/test-plugin"),
        };

        let definition =
            ServicePluginRepo::service_definition(&"test-plugin".to_string(), &manifest)
                .expect("a manifest with an executable yields a definition");

        assert_eq!(definition.username.as_deref(), Some(CC_PLUGIN_USER));
    }

    /// Goal: a file name containing shell metacharacters, or one that looks like an option,
    /// must reach `chown` as one argument, so it can never be split into a command.
    /// Methodology: assert the argument vector directly. A shell would have split on `;` and the
    /// spaces; a direct exec cannot.
    #[test]
    fn chown_args_keep_a_hostile_path_as_one_argument() {
        let hostile = "/var/lib/coolercontrol/plugins/x/; touch /tmp/pwned".to_string();
        let option = "-R".to_string();

        let args = chown_args(&[hostile.clone(), option.clone()], ROOT_USER);

        assert_eq!(args, vec!["-Rh", "root:root", "--", &hostile, &option]);
    }

    /// Goal: the folder and root's files in it must never be the plugin user's, not even
    /// until they are taken back: another process of that user could use the moment to put
    /// its own manifest in place. So the handover may name neither.
    /// Method: a folder holding root's three files and the plugin's own, and what the
    /// handover would pass to `chown`.
    #[test]
    fn the_handover_names_neither_the_folder_nor_roots_files() {
        let dir = tempfile::tempdir().unwrap();
        for file_name in ROOT_ENTRY_NAMES {
            std::fs::write(dir.path().join(file_name), "").unwrap();
        }
        std::fs::write(dir.path().join(PLUGIN_CONFIG_FILE_NAME), "{}").unwrap();
        std::fs::create_dir(dir.path().join(PLUGIN_UI_DIR_NAME)).unwrap();

        let (mut paths, skipped_count) = handover_paths(dir.path()).unwrap();

        paths.sort_unstable();
        let expected = [PLUGIN_CONFIG_FILE_NAME, PLUGIN_UI_DIR_NAME]
            .map(|name| dir.path().join(name).to_str().unwrap().to_string());
        assert_eq!(paths, expected);
        assert_eq!(skipped_count, 0);
        let args = chown_args(&paths, CC_PLUGIN_USER);
        assert_eq!(args.len(), 5, "{args:?}");
    }

    /// Goal: a plugin decides how many files its folder holds, so the handover has to stop
    /// at a limit and say that it did, instead of growing a command line without bound.
    /// Method: a folder with a few entries too many, and what the handover would name.
    #[test]
    fn the_handover_stops_at_its_entry_limit() {
        let dir = tempfile::tempdir().unwrap();
        for index in 0..HANDOVER_ENTRIES_MAX + 3 {
            std::fs::write(dir.path().join(format!("file-{index}")), "").unwrap();
        }

        let (paths, skipped_count) = handover_paths(dir.path()).unwrap();

        assert_eq!(paths.len(), HANDOVER_ENTRIES_MAX);
        assert_eq!(skipped_count, 3);
    }

    /// Goal: the daemon's outbound credentials must not follow the rest of the plugin
    /// directory to the plugin user. A plugin that could read the token could
    /// authenticate as this daemon to a remote machine; one that could rewrite the pin
    /// could redirect trust to a certificate of its choosing.
    #[test]
    fn secure_plugin_folder_locks_down_daemon_credentials() {
        crate::sidecar::ensure_test_handle();
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let token_path = dir.path().join(trust::TOKEN_FILE_NAME);
            let pin_path = dir.path().join(trust::PIN_FILE_NAME);
            std::fs::write(&token_path, "cc_secret\n").unwrap();
            std::fs::write(&pin_path, "aa:bb\n").unwrap();
            std::fs::set_permissions(&token_path, Permissions::from_mode(0o644)).unwrap();
            std::fs::set_permissions(&pin_path, Permissions::from_mode(0o666)).unwrap();

            // The chown needs root; the permission bits are what this asserts, and they
            // are applied before it, exactly as the manifest test does.
            let _ = secure_daemon_credentials(dir.path());

            for path in [&token_path, &pin_path] {
                let mode = std::fs::metadata(path).unwrap().permissions().mode();
                assert_eq!(
                    mode & 0o777,
                    PLUGIN_CREDENTIAL_PERMISSIONS,
                    "{} should be 0600",
                    path.display()
                );
            }
        });
    }

    /// Goal: a failure part-way through securing must not skip the steps after it. The
    /// caller only warns and starts the plugin anyway, so a skipped step leaves the
    /// daemon's own credentials owned and readable by the plugin user.
    /// Methodology: as a non-root user every `chown` fails, which is exactly the partial
    /// failure this guards. Setting the permission bits needs no root, so all three files
    /// must still be hardened even though the call reports an error.
    #[test]
    fn secure_plugin_folder_hardens_every_file_despite_a_failure() {
        crate::sidecar::ensure_test_handle();
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let manifest_path = dir.path().join(SERVICE_MANIFEST_FILE_NAME);
            let token_path = dir.path().join(trust::TOKEN_FILE_NAME);
            let pin_path = dir.path().join(trust::PIN_FILE_NAME);
            std::fs::write(&manifest_path, "").unwrap();
            std::fs::write(&token_path, "a-token\n").unwrap();
            std::fs::write(&pin_path, "aa:bb\n").unwrap();
            for path in [&manifest_path, &token_path, &pin_path] {
                std::fs::set_permissions(path, Permissions::from_mode(0o666)).unwrap();
            }

            let _ = secure_plugin_folder(dir.path(), Some(&Owner::resolve(CC_PLUGIN_USER))).await;

            let manifest_mode = std::fs::metadata(&manifest_path)
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(
                manifest_mode & 0o777,
                PLUGIN_MANIFEST_PERMISSIONS,
                "the manifest should be hardened"
            );
            for path in [&token_path, &pin_path] {
                let mode = std::fs::metadata(path).unwrap().permissions().mode();
                assert_eq!(
                    mode & 0o777,
                    PLUGIN_CREDENTIAL_PERMISSIONS,
                    "{} should be 0600 even after an earlier step failed",
                    path.display()
                );
            }
        });
    }

    /// Goal: a plugin with no credentials is the normal case, so securing must be a
    /// no-op rather than an error that aborts plugin initialization.
    #[test]
    fn secure_daemon_credentials_ignores_absent_files() {
        crate::sidecar::ensure_test_handle();
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            assert!(secure_daemon_credentials(dir.path()).is_ok());
            assert!(dir.path().join(trust::TOKEN_FILE_NAME).exists().not());
        });
    }

    /// A file outside the plugin folder that a planted link leads to, and its mode.
    const SENTINEL_MODE: u32 = 0o644;

    fn sentinel(dir: &Path) -> PathBuf {
        let path = dir.join("sentinel");
        std::fs::write(&path, "untouched").unwrap();
        std::fs::set_permissions(&path, Permissions::from_mode(SENTINEL_MODE)).unwrap();
        path
    }

    fn assert_untouched(sentinel: &Path) {
        let mode = std::fs::metadata(sentinel).unwrap().permissions().mode();
        assert_eq!(mode & 0o7777, SENTINEL_MODE, "the link was followed");
        assert_eq!(std::fs::read_to_string(sentinel).unwrap(), "untouched");
    }

    /// Goal: a plugin owns its `config.json`, so it can swap it for a symlink to a file it
    /// wants: its own manifest, to set `privileged`, or any file on the system. Securing
    /// the config must change neither the mode nor the owner of what the link points to.
    /// Method: plant such a link, to a file and to nothing, and secure the plugin's files.
    /// The mode needs no root, so a followed link shows in the sentinel's mode.
    #[test]
    fn a_planted_config_link_is_removed_and_never_followed() {
        crate::sidecar::ensure_test_handle();
        crate::rt::test_runtime(async {
            let outside = tempfile::tempdir().unwrap();
            let sentinel = sentinel(outside.path());
            for target in [sentinel.clone(), outside.path().join("absent")] {
                let dir = tempfile::tempdir().unwrap();
                let config_path = dir.path().join(PLUGIN_CONFIG_FILE_NAME);
                std::os::unix::fs::symlink(&target, &config_path).unwrap();
                let manifest = managed_manifest(dir.path().to_path_buf());

                secure_plugin_files(&manifest, None).await;

                assert_untouched(&sentinel);
                assert!(config_path.symlink_metadata().is_err(), "the link stays");
                assert!(outside.path().join("absent").exists().not());
            }
        });
    }

    /// Goal: where hard links to files of others are allowed, a second name for a file does
    /// what a symlink does. Such a config must not be secured, as that secures the other file.
    /// Method: link the config to a sentinel and require an error and an unchanged mode.
    #[test]
    fn a_hard_linked_config_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let sentinel = sentinel(dir.path());
        let config_path = dir.path().join(PLUGIN_CONFIG_FILE_NAME);
        std::fs::hard_link(&sentinel, &config_path).unwrap();

        let result = secure_config_file(&config_path, None);

        assert!(result.is_err());
        assert_untouched(&sentinel);
    }

    /// Goal: the daemon reads its token and writes its TLS pin by name, so a link planted
    /// under either name would have root secure, read or overwrite another file.
    /// Method: plant a link to a file as the token and a dangling one as the pin, then
    /// secure the credentials. Both links have to go and the sentinel stays as it was.
    #[test]
    fn planted_credential_links_are_removed_and_never_followed() {
        let outside = tempfile::tempdir().unwrap();
        let sentinel = sentinel(outside.path());
        let dir = tempfile::tempdir().unwrap();
        let token_path = dir.path().join(trust::TOKEN_FILE_NAME);
        let pin_path = dir.path().join(trust::PIN_FILE_NAME);
        std::os::unix::fs::symlink(&sentinel, &token_path).unwrap();
        std::os::unix::fs::symlink(outside.path().join("absent"), &pin_path).unwrap();

        let result = secure_daemon_credentials(dir.path());

        assert!(result.is_ok(), "{result:?}");
        assert_untouched(&sentinel);
        assert!(
            token_path.symlink_metadata().is_err(),
            "the token link stays"
        );
        assert!(pin_path.symlink_metadata().is_err(), "the pin link stays");
    }

    /// Goal: a credential with a second name is not the daemon's alone, since whoever holds
    /// the other name reads and rewrites it. Refusing it would leave the daemon using it, so
    /// it goes like a planted symlink does, and the file behind it stays as it was.
    /// Method: give a sentinel the token's and the pin's name as well, then secure the
    /// credentials. Both names have to go, with the sentinel's mode and content unchanged.
    #[test]
    fn hard_linked_credentials_are_removed_and_never_secured() {
        let dir = tempfile::tempdir().unwrap();
        let sentinel = sentinel(dir.path());
        let token_path = dir.path().join(trust::TOKEN_FILE_NAME);
        let pin_path = dir.path().join(trust::PIN_FILE_NAME);
        std::fs::hard_link(&sentinel, &token_path).unwrap();
        std::fs::hard_link(&sentinel, &pin_path).unwrap();

        let result = secure_daemon_credentials(dir.path());

        assert!(result.is_ok(), "{result:?}");
        assert_untouched(&sentinel);
        assert!(token_path.symlink_metadata().is_err(), "the token stays");
        assert!(pin_path.symlink_metadata().is_err(), "the pin stays");
    }

    /// Goal: a manifest that is a link is never secured through it, and never removed
    /// either: a plugin without its manifest is not a plugin.
    /// Method: make the manifest a link to a sentinel and require an error, with both left.
    #[test]
    fn a_linked_manifest_is_refused_and_kept() {
        let outside = tempfile::tempdir().unwrap();
        let sentinel = sentinel(outside.path());
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join(SERVICE_MANIFEST_FILE_NAME);
        std::os::unix::fs::symlink(&sentinel, &manifest_path).unwrap();

        let result = secure_manifest(dir.path());

        assert!(result.is_err());
        assert_untouched(&sentinel);
        assert!(manifest_path.symlink_metadata().is_ok());
    }

    /// Goal: the owner is looked up once, ahead of the handover, and a system without the
    /// plugin user must still have every mode set, since a mode needs no owner. A lookup
    /// that fails early must not turn into files that were never secured.
    /// Method: hand a plugin's files to a user that does not exist and read the modes back.
    #[test]
    fn a_handover_to_an_unknown_user_still_sets_every_mode() {
        crate::sidecar::ensure_test_handle();
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let manifest_path = dir.path().join(SERVICE_MANIFEST_FILE_NAME);
            let config_path = dir.path().join(PLUGIN_CONFIG_FILE_NAME);
            std::fs::write(&manifest_path, "").unwrap();
            std::fs::write(&config_path, "{}").unwrap();
            for path in [&manifest_path, &config_path] {
                std::fs::set_permissions(path, Permissions::from_mode(0o666)).unwrap();
            }
            let manifest = managed_manifest(dir.path().to_path_buf());

            secure_plugin_files(&manifest, Some("cc-no-such-user")).await;

            let mode_of = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode();
            assert_eq!(mode_of(dir.path()) & 0o7777, PLUGIN_FOLDER_PERMISSIONS);
            assert_eq!(mode_of(&manifest_path) & 0o777, PLUGIN_MANIFEST_PERMISSIONS);
            assert_eq!(
                mode_of(&config_path) & 0o777,
                PLUGIN_CONFIG_FILE_PERMISSIONS
            );
        });
    }

    /// Goal: the manifest must not stay group- or world-writable, since it declares `privileged`
    /// and therefore decides whether the plugin runs as root.
    /// Methodology: leave a 0666 manifest behind, secure the folder, and re-read the mode. This
    /// half of `secure_plugin_folder` does not need root, unlike the ownership reset.
    #[test]
    fn secure_plugin_folder_resets_manifest_permissions() {
        crate::sidecar::ensure_test_handle();
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let manifest_path = dir.path().join(SERVICE_MANIFEST_FILE_NAME);
            std::fs::write(&manifest_path, "id = \"test\"\n").unwrap();
            std::fs::set_permissions(&manifest_path, Permissions::from_mode(0o666)).unwrap();

            let _ = secure_plugin_folder(dir.path(), Some(&Owner::ROOT)).await;

            let mode = std::fs::metadata(&manifest_path)
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(
                mode & 0o777,
                PLUGIN_MANIFEST_PERMISSIONS,
                "Manifest must be reset to 644 so the plugin cannot rewrite it"
            );
        });
    }

    /// Goal: the plugin must not be able to rename or delete what root owns in its folder,
    /// or the root-owned manifest protects nothing. The sticky bit is what enforces that, and
    /// the group write bit is what keeps the folder usable by the plugin.
    /// Methodology: setting the mode needs no root, so secure a folder and read it back.
    #[test]
    fn secure_plugin_folder_makes_the_folder_sticky() {
        crate::sidecar::ensure_test_handle();
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();

            let _ = secure_plugin_folder(dir.path(), Some(&Owner::ROOT)).await;

            let mode = std::fs::metadata(dir.path()).unwrap().permissions().mode();
            assert_eq!(mode & 0o7777, PLUGIN_FOLDER_PERMISSIONS);
        });
    }

    /// Goal: a folder with no manifest is still secured without erroring.
    /// Methodology: secure an empty directory as root and assert success, so plugins that have
    /// not yet been given a manifest do not fail initialization.
    #[test]
    fn secure_plugin_folder_without_manifest_succeeds() {
        crate::sidecar::ensure_test_handle();
        crate::rt::test_runtime(async {
            if is_root().not() {
                // Skip: the recursive chown fails for a non-root user.
                return;
            }
            let dir = tempfile::tempdir().unwrap();

            let result = secure_plugin_folder(dir.path(), Some(&Owner::ROOT)).await;

            assert!(result.is_ok(), "Missing manifest must not be an error");
        });
    }

    /// Goal: the manifest stays root-owned even though the rest of the folder is handed to the
    /// unprivileged plugin user, which is what stops a plugin from setting `privileged = true`.
    /// Methodology: root-only. Secure a folder as `cc-plugin-user` and compare the manifest's uid
    /// against the directory's.
    #[test]
    fn secure_plugin_folder_keeps_manifest_owned_by_root() {
        crate::sidecar::ensure_test_handle();
        crate::rt::test_runtime(async {
            if is_root().not() {
                // Skip: chown to another user requires root.
                return;
            }
            let dir = tempfile::tempdir().unwrap();
            let manifest_path = dir.path().join(SERVICE_MANIFEST_FILE_NAME);
            std::fs::write(&manifest_path, "id = \"test\"\n").unwrap();
            let nested = dir.path().join("data.txt");
            std::fs::write(&nested, "x").unwrap();

            let owner = Owner::resolve(CC_PLUGIN_USER);
            if owner.ids().is_err() {
                // Skip: the plugin user does not exist on this machine.
                return;
            }

            let result = secure_plugin_folder(dir.path(), Some(&owner)).await;

            assert!(result.is_ok(), "{result:?}");
            let manifest_uid = std::fs::metadata(&manifest_path).unwrap().uid();
            let nested_uid = std::fs::metadata(&nested).unwrap().uid();
            let folder = std::fs::metadata(dir.path()).unwrap();
            assert_eq!(manifest_uid, 0, "Manifest must remain owned by root");
            assert_eq!(folder.uid(), 0, "The folder itself must be root's");
            assert_eq!(
                folder.gid(),
                std::fs::metadata(&nested).unwrap().gid(),
                "The plugin's group must keep the folder"
            );
            assert_ne!(
                nested_uid, manifest_uid,
                "The rest of the folder must be handed to the plugin user"
            );
        });
    }

    #[test]
    fn test_secure_config_file_sets_600_permissions() {
        crate::sidecar::ensure_test_handle();
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let config_path = dir.path().join("config.json");
            std::fs::write(&config_path, "{}").unwrap();
            std::fs::set_permissions(&config_path, Permissions::from_mode(0o644)).unwrap();

            // secure_config_file will set permissions and attempt chown.
            // chown may fail if not root, but permissions should still be set.
            let _ = secure_config_file(&config_path, Some(&Owner::ROOT));

            let perms = std::fs::metadata(&config_path).unwrap().permissions();
            assert_eq!(
                perms.mode() & 0o777,
                PLUGIN_CONFIG_FILE_PERMISSIONS,
                "Config file should have 600 permissions"
            );
        });
    }

    #[test]
    fn test_secure_config_file_nonexistent_file_returns_error() {
        crate::sidecar::ensure_test_handle();
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let config_path = dir.path().join("nonexistent.json");

            let result = secure_config_file(&config_path, Some(&Owner::ROOT));
            assert!(result.is_err(), "Should fail for nonexistent file");
        });
    }

    #[test]
    fn test_secure_config_file_chown_fails_for_non_root() {
        crate::sidecar::ensure_test_handle();
        crate::rt::test_runtime(async {
            if is_root() {
                // Skip: chown won't fail when running as root
                return;
            }
            let dir = tempfile::tempdir().unwrap();
            let config_path = dir.path().join("config.json");
            std::fs::write(&config_path, "{}").unwrap();

            let result = secure_config_file(&config_path, Some(&Owner::ROOT));
            assert!(
                result.is_err(),
                "chown to root should fail when not running as root"
            );
        });
    }

    #[test]
    fn test_secure_config_file_chown_succeeds_as_root() {
        crate::sidecar::ensure_test_handle();
        crate::rt::test_runtime(async {
            if !is_root() {
                // Skip: requires root privileges
                return;
            }
            let dir = tempfile::tempdir().unwrap();
            let config_path = dir.path().join("config.json");
            std::fs::write(&config_path, "{}").unwrap();

            let result = secure_config_file(&config_path, Some(&Owner::ROOT));
            assert!(result.is_ok(), "chown to root should succeed as root");

            let perms = std::fs::metadata(&config_path).unwrap().permissions();
            assert_eq!(
                perms.mode() & 0o777,
                PLUGIN_CONFIG_FILE_PERMISSIONS,
                "Config file should have 600 permissions"
            );
        });
    }

    #[test]
    fn test_secure_config_file_permissions_maintained_after_rewrite() {
        crate::sidecar::ensure_test_handle();
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let config_path = dir.path().join("config.json");
            std::fs::write(&config_path, "{}").unwrap();

            let _ = secure_config_file(&config_path, Some(&Owner::ROOT));

            // Simulate a rewrite that resets permissions
            std::fs::write(&config_path, "{\"updated\": true}").unwrap();
            std::fs::set_permissions(&config_path, Permissions::from_mode(0o644)).unwrap();

            let _ = secure_config_file(&config_path, Some(&Owner::ROOT));

            let perms = std::fs::metadata(&config_path).unwrap().permissions();
            assert_eq!(
                perms.mode() & 0o777,
                PLUGIN_CONFIG_FILE_PERMISSIONS,
                "Permissions should be restored to 600 after re-securing"
            );
        });
    }

    #[test]
    fn test_secure_config_file_no_owner_skips_chown() {
        crate::sidecar::ensure_test_handle();
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let config_path = dir.path().join("config.json");
            std::fs::write(&config_path, "{}").unwrap();
            std::fs::set_permissions(&config_path, Permissions::from_mode(0o644)).unwrap();

            let result = secure_config_file(&config_path, None);
            assert!(result.is_ok(), "Should succeed without chown");

            let perms = std::fs::metadata(&config_path).unwrap().permissions();
            assert_eq!(
                perms.mode() & 0o777,
                PLUGIN_CONFIG_FILE_PERMISSIONS,
                "Config file should have 600 permissions even without chown"
            );
        });
    }
}
