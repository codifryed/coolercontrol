// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! AMD GPU overdrive setup: sets the overdrive bit of `amdgpu.ppfeaturemask` for the next boot.
//!
//! The flow: detect how this system builds its boot image (`boot_image`), write the module
//! option, then rebuild the image, or set a kernel argument on rpm-ostree. The rebuild runs
//! detached from the daemon, so a daemon restart cannot cut it short. The run's state is kept in
//! memory only; after a restart, the state comes from the disk and the loaded module.

use std::cell::RefCell;
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use crate::api::CCError;
use crate::cc_fs;
use crate::events::amd_gpu_overdrive::{
    AmdGpuOverdriveHandle, AmdGpuOverdrivePlan, AmdGpuOverdriveStatus, OverdriveAction,
    OverdriveRoute, OverdriveState,
};
use crate::events::notifier::{notify_all_sessions, NotificationHandle, NotificationIcon};
use crate::repositories::gpu::boot_image::{self, BootFacts, BootSetup, RebuildTool, Refusal};
use crate::repositories::gpu::rebuild_preflight;
use crate::repositories::utils::{DirectCommand, ShellCommandResult};
use crate::rt;
use anyhow::{anyhow, Context, Result};
use log::{debug, info, warn};

pub const PP_OVERDRIVE_MASK: u64 = 0x4000;
pub const PP_FEATURE_MASK_PATH: &str = "/sys/module/amdgpu/parameters/ppfeaturemask";
const PP_FEATURE_MASK_KARG_KEY: &str = "amdgpu.ppfeaturemask";
const MODPROBE_CONF_PATH: &str = "/etc/modprobe.d";
const AMDGPU_OVERDRIVE_CONF_FILE: &str = "99-amdgpu-overdrive.conf";
const DRACUT_CONF_PATH: &str = "/etc/dracut.conf.d";
const DRACUT_OVERDRIVE_CONF_FILE: &str = "99-coolercontrol-amdgpu.conf";
/// Holds the rebuild's output. `/run` is cleared on boot.
const RUN_DIR_PATH: &str = "/run/coolercontrold-amd-overdrive";
const OUTPUT_FILE: &str = "rebuild.log";
const RPM_OSTREE_PATH: &str = "/usr/bin/rpm-ostree";
/// systemd creates this when it is the init system.
const SYSTEMD_BOOTED_PATH: &str = "/run/systemd/system";
const SYSTEMD_RUN_PATH: &str = "/usr/bin/systemd-run";
const SETSID_PATH: &str = "/usr/bin/setsid";
const SYSTEMD_INHIBIT_PATH: &str = "/usr/bin/systemd-inhibit";
/// logind creates this. Without logind, systemd-inhibit fails and would not run the rebuild.
const LOGIND_SEATS_PATH: &str = "/run/systemd/seats";
/// A fixed name, so systemd refuses a second rebuild while one still runs.
const REBUILD_UNIT: &str = "coolercontrold-amd-overdrive-rebuild";
/// Rebuilding every kernel's image can take minutes. Past this the daemon stops waiting, but
/// the rebuild goes on: a kill mid-build leaves a half-written image.
const REBUILD_WAIT_LIMIT: Duration = Duration::from_hours(1);
/// rpm-ostree writes a new deployment, which can take minutes.
const RPM_OSTREE_TIMEOUT: Duration = Duration::from_secs(600);
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// Enough to keep failure lines that follow pages of build warnings.
const OUTPUT_LIMIT_BYTES: usize = 1024 * 1024;
const ERROR_EXCERPT_LINES: usize = 5;
const _: () = assert!(ERROR_EXCERPT_LINES > 0);
/// rpm-ostree's exit code for "No changes", with `--unchanged-exit-77`.
const RPM_OSTREE_UNCHANGED_EXIT_CODE: i32 = 77;

/// Computes the new ppfeaturemask with the overdrive bit enabled.
pub fn compute_overdrive_mask(current_mask: u64) -> u64 {
    let new_mask = current_mask | PP_OVERDRIVE_MASK;
    assert_eq!(new_mask & current_mask, current_mask);
    new_mask
}

/// Checks whether the overdrive bit is set in the current ppfeaturemask.
pub async fn is_overdrive_enabled() -> bool {
    OverdrivePaths::under(Path::new("/"))
        .overdrive_enabled()
        .await
}

/// Returns the kernel boot parameter string to enable overdrive.
pub async fn get_fan_control_boot_option() -> String {
    if let Ok(current_mask) = get_pp_feature_mask().await {
        overdrive_karg(compute_overdrive_mask(current_mask))
    } else {
        format!("{PP_FEATURE_MASK_KARG_KEY}=0xffffffff")
    }
}

fn overdrive_karg(mask: u64) -> String {
    format!("{PP_FEATURE_MASK_KARG_KEY}=0x{mask:X}")
}

/// Reads and parses the current ppfeaturemask from sysfs, on the blocking pool so no sysfs read
/// runs on the runtime thread.
pub async fn get_pp_feature_mask() -> Result<u64> {
    read_pp_feature_mask(PathBuf::from(PP_FEATURE_MASK_PATH)).await
}

async fn read_pp_feature_mask(path: PathBuf) -> Result<u64> {
    let ppfeaturemask = rt::spawn_blocking(move || cc_fs::read_bytes_blocking(&path)).await??;
    parse_pp_feature_mask(&String::from_utf8(ppfeaturemask)?)
}

fn parse_pp_feature_mask(ppfeaturemask: &str) -> Result<u64> {
    let ppfeaturemask = ppfeaturemask
        .trim()
        .strip_prefix("0x")
        .context("Invalid ppfeaturemask")?;
    u64::from_str_radix(ppfeaturemask, 16).context("Invalid ppfeaturemask")
}

/// Where setup reads and writes. Rooted, so tests can run it against a scratch tree.
#[derive(Debug, Clone)]
struct OverdrivePaths {
    root: PathBuf,
}

impl OverdrivePaths {
    fn under(root: &Path) -> Self {
        assert!(root.is_absolute());
        Self {
            root: root.to_path_buf(),
        }
    }

    fn at(&self, absolute_path: &str) -> PathBuf {
        assert!(
            absolute_path.starts_with('/'),
            "paths are given from the root"
        );
        self.root.join(&absolute_path[1..])
    }

    fn conf(&self) -> PathBuf {
        self.at(MODPROBE_CONF_PATH).join(AMDGPU_OVERDRIVE_CONF_FILE)
    }

    fn drop_in(&self) -> PathBuf {
        self.at(DRACUT_CONF_PATH).join(DRACUT_OVERDRIVE_CONF_FILE)
    }

    fn output(&self) -> PathBuf {
        self.at(RUN_DIR_PATH).join(OUTPUT_FILE)
    }

    async fn overdrive_enabled(&self) -> bool {
        let mask = read_pp_feature_mask(self.at(PP_FEATURE_MASK_PATH)).await;
        (mask.unwrap_or_default() & PP_OVERDRIVE_MASK) > 0
    }

    async fn conf_present(&self) -> bool {
        let conf = self.conf();
        rt::spawn_blocking(move || cc_fs::exists(conf))
            .await
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum JobPhase {
    #[default]
    Idle,
    Applying,
    Failed,
}

/// The last setup run since the daemon started. Memory only: after a restart, the state comes
/// from the disk and the loaded module.
#[derive(Debug, Clone, Default)]
struct Job {
    phase: JobPhase,
    action: Option<OverdriveAction>,
    tool: Option<&'static str>,
    manual_command: Option<String>,
    error: Option<String>,
    /// The run finished, so its change waits for a reboot.
    done: bool,
    rebuild_skipped: bool,
}

#[derive(Debug, Clone, Copy)]
enum Work {
    Rebuild(RebuildTool),
    RpmOstree(u64),
}

/// Runs AMD GPU overdrive setup in the background and reports its state. One per daemon,
/// owned by the device actor, so runs never overlap.
pub struct OverdriveSetup {
    job: Rc<RefCell<Job>>,
    handle: AmdGpuOverdriveHandle,
    notification_handle: NotificationHandle,
    paths: OverdrivePaths,
}

impl OverdriveSetup {
    pub fn new(handle: AmdGpuOverdriveHandle, notification_handle: NotificationHandle) -> Self {
        Self::with_root(handle, notification_handle, Path::new("/"))
    }

    pub fn with_root(
        handle: AmdGpuOverdriveHandle,
        notification_handle: NotificationHandle,
        root: &Path,
    ) -> Self {
        Self {
            job: Rc::new(RefCell::new(Job::default())),
            handle,
            notification_handle,
            paths: OverdrivePaths::under(root),
        }
    }

    pub async fn status(&self) -> AmdGpuOverdriveStatus {
        let job = self.job.borrow().clone();
        status_of(&job, &self.paths).await
    }

    /// What `enable` would do, for the user to confirm first.
    pub async fn plan(&self) -> Result<AmdGpuOverdrivePlan> {
        enable_plan(&self.paths).await
    }

    /// Starts setup by whichever route the system's boot image tooling supports, and returns
    /// at once. Completion arrives as an `amd-gpu-overdrive` event. Without `rebuild`, the
    /// rebuild route only writes the module option and leaves the rebuild to the user.
    pub async fn enable(&self, rebuild: bool) -> Result<AmdGpuOverdriveStatus> {
        let current_mask = read_pp_feature_mask(self.paths.at(PP_FEATURE_MASK_PATH)).await?;
        let new_mask = compute_overdrive_mask(current_mask);
        if new_mask == current_mask {
            return Ok(self.status().await);
        }
        let status = self.status().await;
        if run_needed(status.state).not() {
            return Ok(status);
        }
        let facts = detect_boot_facts(&self.paths).await?;
        let setup = boot_image::classify(&facts);
        info!("Detected AMD GPU overdrive boot setup: {setup:?}");
        match setup {
            BootSetup::Refused(refusal) => {
                return Err(CCError::UserError {
                    msg: refusal_message(refusal, new_mask),
                }
                .into());
            }
            BootSetup::RpmOstree => {
                self.start(
                    OverdriveAction::Enable,
                    "rpm-ostree",
                    Work::RpmOstree(new_mask),
                );
            }
            BootSetup::Rebuild(tool) if rebuild => {
                refuse_on_preflight(&self.paths).await?;
                write_overdrive_conf(&self.paths, new_mask, facts.dracut).await?;
                self.start(OverdriveAction::Enable, tool.id(), Work::Rebuild(tool));
            }
            BootSetup::Rebuild(tool) => {
                write_overdrive_conf(&self.paths, new_mask, facts.dracut).await?;
                info!("Wrote the AMD GPU overdrive config. The user rebuilds the boot image.");
                self.job.replace(config_only_job(
                    Some(tool.id()),
                    Some(tool.command().to_owned()),
                ));
            }
            BootSetup::Unrecognized => {
                write_overdrive_conf(&self.paths, new_mask, facts.dracut).await?;
                warn!("Unrecognized boot image tooling, skipping the boot image rebuild.");
                self.job.replace(config_only_job(None, None));
            }
        }
        let status = self.status().await;
        self.handle.broadcast(status.clone());
        Ok(status)
    }

    /// Marks the job as running and runs `work` in the background.
    fn start(&self, action: OverdriveAction, tool: &'static str, work: Work) {
        let previous = self.job.replace(Job {
            phase: JobPhase::Applying,
            action: Some(action),
            tool: Some(tool),
            ..Job::default()
        });
        assert_ne!(previous.phase, JobPhase::Applying, "runs never overlap");
        // The device actor runs as a task on main_scope, where a scope spawn would panic, so
        // this uses rt::spawn, whose 'static future needs the Rc cloned.
        let job = Rc::clone(&self.job);
        let handle = self.handle.clone();
        let notification_handle = self.notification_handle.clone();
        let paths = self.paths.clone();
        rt::spawn(async move {
            let result = match work {
                Work::Rebuild(tool) => rebuild_boot_image(tool, &paths).await,
                Work::RpmOstree(new_mask) => set_rpm_ostree_karg(&overdrive_karg(new_mask)).await,
            };
            let manual_command = match work {
                Work::Rebuild(tool) => Some(tool.command().to_owned()),
                Work::RpmOstree(_) => None,
            };
            finish_run(&mut job.borrow_mut(), &result, manual_command);
            notify_completion(&result, &notification_handle);
            let job = job.borrow().clone();
            handle.broadcast(status_of(&job, &paths).await);
        });
    }
}

/// An enable that wrote the conf and left the rebuild to the user, with `command` when the
/// tool is known.
fn config_only_job(tool: Option<&'static str>, command: Option<String>) -> Job {
    Job {
        action: Some(OverdriveAction::Enable),
        tool,
        manual_command: command,
        done: true,
        rebuild_skipped: true,
        ..Job::default()
    }
}

/// What enabling would do: the route, its exact command, the files written, and any refusal or
/// failed check that would stop it.
async fn enable_plan(paths: &OverdrivePaths) -> Result<AmdGpuOverdrivePlan> {
    let mask = read_pp_feature_mask(paths.at(PP_FEATURE_MASK_PATH)).await?;
    let new_mask = compute_overdrive_mask(mask);
    let facts = detect_boot_facts(paths).await?;
    let plan = match boot_image::classify(&facts) {
        BootSetup::Refused(refusal) => AmdGpuOverdrivePlan {
            route: None,
            tool: None,
            command: None,
            files: Vec::new(),
            problem: Some(refusal_message(refusal, new_mask)),
        },
        BootSetup::RpmOstree => {
            let edit = rpm_ostree_karg_edit_now(&overdrive_karg(new_mask)).await?;
            AmdGpuOverdrivePlan {
                route: Some(OverdriveRoute::KernelArgument),
                tool: Some("rpm-ostree".to_owned()),
                command: Some(format!("{RPM_OSTREE_PATH} kargs {edit}")),
                files: Vec::new(),
                problem: None,
            }
        }
        BootSetup::Rebuild(tool) => AmdGpuOverdrivePlan {
            route: Some(OverdriveRoute::Rebuild),
            tool: Some(tool.id().to_owned()),
            command: Some(tool.command().to_owned()),
            files: overdrive_files(paths, facts.dracut),
            problem: preflight_problem(paths).await?,
        },
        BootSetup::Unrecognized => AmdGpuOverdrivePlan {
            route: Some(OverdriveRoute::ConfigOnly),
            tool: None,
            command: None,
            files: overdrive_files(paths, facts.dracut),
            problem: None,
        },
    };
    Ok(plan)
}

/// The files the rebuild and config-only routes write.
fn overdrive_files(paths: &OverdrivePaths, dracut: bool) -> Vec<String> {
    let mut files = Vec::with_capacity(2);
    files.push(paths.conf().display().to_string());
    if dracut {
        files.push(paths.drop_in().display().to_string());
    }
    files
}

/// A finished or running setup starts no new run. A failure can be retried.
fn run_needed(state: OverdriveState) -> bool {
    matches!(
        state,
        OverdriveState::NotConfigured | OverdriveState::Failed
    )
}

/// Records how a run ended. A failure keeps the command that finishes it by hand.
fn finish_run(job: &mut Job, result: &Result<()>, manual_command: Option<String>) {
    assert_eq!(job.phase, JobPhase::Applying);
    match result {
        Ok(()) => {
            job.phase = JobPhase::Idle;
            job.done = true;
        }
        Err(err) => {
            job.phase = JobPhase::Failed;
            job.error = Some(err.to_string());
            job.manual_command = manual_command;
        }
    }
}

fn notify_completion(result: &Result<()>, notification_handle: &NotificationHandle) {
    match result {
        Ok(()) => {
            info!("AMD GPU overdrive configured. Please reboot to apply it.");
            notify_all_sessions(
                "AMD GPU Overdrive Configured",
                "Please reboot to apply AMD GPU overdrive.",
                NotificationIcon::Info,
                true,
                None,
                Some(notification_handle),
            );
        }
        Err(err) => {
            warn!("AMD GPU overdrive setup failed: {err}");
            notify_all_sessions(
                "AMD GPU Overdrive Setup Failed",
                &err.to_string(),
                NotificationIcon::Error,
                true,
                Some(2),
                Some(notification_handle),
            );
        }
    }
}

async fn status_of(job: &Job, paths: &OverdrivePaths) -> AmdGpuOverdriveStatus {
    let active = paths.overdrive_enabled().await;
    let conf_present = paths.conf_present().await;
    AmdGpuOverdriveStatus {
        state: derive_state(job, active, conf_present),
        tool: job.tool.map(str::to_owned),
        manual_command: job.manual_command.clone(),
        error: job.error.clone(),
        rebuild_skipped: job.rebuild_skipped,
        action: job.action,
    }
}

/// A run in progress or a failure outranks the disk, which may hold a conf whose image never
/// finished. Without a run since the daemon started, a conf the module does not reflect yet
/// means a reboot is pending.
fn derive_state(job: &Job, active: bool, conf_present: bool) -> OverdriveState {
    match job.phase {
        JobPhase::Applying => OverdriveState::Applying,
        JobPhase::Failed => OverdriveState::Failed,
        JobPhase::Idle if active => OverdriveState::Active,
        JobPhase::Idle if job.done || conf_present => OverdriveState::RebootRequired,
        JobPhase::Idle => OverdriveState::NotConfigured,
    }
}

/// Why a rebuild should not start now, from checks made before anything is written.
async fn preflight_problem(paths: &OverdrivePaths) -> Result<Option<String>> {
    let root = paths.root.clone();
    let problem = rt::spawn_blocking(move || rebuild_preflight::check_blocking(&root)).await?;
    Ok(problem.map(|problem| problem.message()))
}

/// Runs before anything is written, so a refusal leaves the system as it was.
async fn refuse_on_preflight(paths: &OverdrivePaths) -> Result<()> {
    let Some(msg) = preflight_problem(paths).await? else {
        return Ok(());
    };
    info!("Not rebuilding the boot image: {msg}");
    Err(CCError::UserError { msg }.into())
}

/// Reads the boot image facts of this system off the runtime thread.
async fn detect_boot_facts(paths: &OverdrivePaths) -> Result<BootFacts> {
    let root = paths.root.clone();
    let mut facts =
        rt::spawn_blocking(move || boot_image::gather_boot_facts_blocking(&root)).await?;
    if facts.sdbootutil {
        let sdbootutil = paths.at("/usr/bin/sdbootutil").display().to_string();
        let result = DirectCommand::new(&sdbootutil, PROBE_TIMEOUT)
            .arg("is-installed")
            .run()
            .await;
        facts.sdbootutil_installed = matches!(result, ShellCommandResult::Success { .. });
    }
    Ok(facts)
}

fn refusal_message(refusal: Refusal, new_mask: u64) -> String {
    let karg = overdrive_karg(new_mask);
    match refusal {
        Refusal::NixOs => "NixOS manages kernel parameters declaratively. Set \
            hardware.amdgpu.overdrive.enable = true; in configuration.nix, run \
            nixos-rebuild boot, then reboot."
            .to_owned(),
        Refusal::Guix => format!(
            "Guix System manages kernel parameters declaratively. Add {karg} to the \
            kernel-arguments of your operating-system declaration, run guix system \
            reconfigure, then reboot."
        ),
        Refusal::SteamOs => format!(
            "SteamOS discards changes to /etc on OS updates, so CoolerControl cannot keep this \
            setting. Add {karg} to the kernel command line instead."
        ),
        Refusal::Ostree => format!(
            "Set the kernel argument yourself, then reboot: ostree admin kargs edit-in-place \
            --append-if-missing={karg}. If the command line already sets \
            {PP_FEATURE_MASK_KARG_KEY}, change that value instead."
        ),
        Refusal::CompetingTools(first, second) => format!(
            "Both {} and {} rebuild the boot image on this system, so CoolerControl cannot tell \
            which one your bootloader loads. Add {} to {MODPROBE_CONF_PATH}/\
            {AMDGPU_OVERDRIVE_CONF_FILE}, then rebuild the boot image with the tool you use.",
            first.id(),
            second.id(),
            overdrive_conf(new_mask).trim(),
        ),
    }
}

fn overdrive_conf(mask: u64) -> String {
    format!("options amdgpu ppfeaturemask=0x{mask:X}\n")
}

/// Writes the modprobe option, and for dracut a drop-in that copies it into the image: dracut
/// only copies `/etc/modprobe.d` in host-only mode, which Debian 13 and generic images lack.
async fn write_overdrive_conf(paths: &OverdrivePaths, new_mask: u64, dracut: bool) -> Result<()> {
    assert!(new_mask & PP_OVERDRIVE_MASK > 0);
    let conf_path = paths.conf();
    cc_fs::create_dir_all(paths.at(MODPROBE_CONF_PATH)).await?;
    cc_fs::write_string(&conf_path, overdrive_conf(new_mask)).await?;
    info!("Wrote AMD GPU overdrive config to {}", conf_path.display());
    if dracut {
        // The image reads the conf at its real path, not the test root's.
        let drop_in =
            format!("install_items+=\" {MODPROBE_CONF_PATH}/{AMDGPU_OVERDRIVE_CONF_FILE} \"\n");
        let drop_in_path = paths.drop_in();
        cc_fs::create_dir_all(paths.at(DRACUT_CONF_PATH)).await?;
        cc_fs::write_string(&drop_in_path, drop_in).await?;
        info!("Wrote dracut drop-in to {}", drop_in_path.display());
    }
    Ok(())
}

/// Runs the rebuild detached from the daemon, so a daemon stop or restart cannot cut it short
/// and leave a half-written image: in its own unit under systemd, in its own session otherwise.
/// Its output goes to a file for the same reason, and the daemon only waits for its exit status.
async fn rebuild_boot_image(tool: RebuildTool, paths: &OverdrivePaths) -> Result<()> {
    info!("Rebuilding the boot image with: {}", tool.command());
    let output = paths.output();
    let launch_paths = paths.clone();
    let (systemd, inhibit, stale_output) = rt::spawn_blocking(move || {
        let exists = |path: &str| cc_fs::exists(launch_paths.at(path));
        (
            exists(SYSTEMD_BOOTED_PATH),
            exists(SYSTEMD_INHIBIT_PATH) && exists(LOGIND_SEATS_PATH),
            cc_fs::exists(launch_paths.output()),
        )
    })
    .await?;
    cc_fs::create_dir_all(paths.at(RUN_DIR_PATH)).await?;
    // A stale output would read as this run's when the launcher fails before the tool starts.
    if stale_output {
        cc_fs::remove_file(&output).await?;
    }
    let argv = rebuild_argv(tool.command(), &output, systemd, inhibit);
    let result = run_argv(&argv, REBUILD_WAIT_LIMIT).await;
    let output_text = read_output(&output).await;
    debug!("Boot image rebuild output: {output_text}");
    match result {
        Ok((exit_code, _, stderr)) => rebuild_outcome(tool, exit_code, &output_text, &stderr),
        Err(err) => {
            warn!("Stopped waiting for the boot image rebuild: {err}");
            Err(anyhow!(
                "The boot image rebuild is still running after an hour. Reboot only once it has \
                finished. With systemd, systemctl status {REBUILD_UNIT} shows it."
            ))
        }
    }
}

/// The launcher, then systemd-inhibit to hold off shutdown while the image is written, then the
/// shell that sends the tool's output to `output`.
fn rebuild_argv(command: &str, output: &Path, systemd: bool, inhibit: bool) -> Vec<String> {
    let output = output.display().to_string();
    assert!(output.contains('\'').not(), "the path is single-quoted");
    let mut argv: Vec<String> = Vec::with_capacity(14);
    if systemd {
        argv.extend(
            [
                SYSTEMD_RUN_PATH,
                &format!("--unit={REBUILD_UNIT}"),
                "--wait",
                "--collect",
                "--quiet",
                "--description=CoolerControl AMD GPU overdrive boot image rebuild",
                "--",
            ]
            .map(str::to_owned),
        );
    } else {
        argv.extend([SETSID_PATH, "--fork", "--wait"].map(str::to_owned));
    }
    if inhibit {
        argv.extend(
            [
                SYSTEMD_INHIBIT_PATH,
                "--what=shutdown",
                "--mode=block",
                "--who=CoolerControl",
                "--why=Rebuilding boot images",
            ]
            .map(str::to_owned),
        );
    }
    argv.extend(
        [
            "/bin/sh",
            "-c",
            &format!("exec >'{output}' 2>&1\n{command}"),
        ]
        .map(str::to_owned),
    );
    argv
}

async fn run_argv(argv: &[String], timeout: Duration) -> Result<(i32, String, String)> {
    assert!(argv.is_empty().not());
    let mut command = DirectCommand::new(&argv[0], timeout);
    for arg in &argv[1..] {
        command = command.arg(arg);
    }
    command.run_with_code().await
}

/// The output's tail, where tools put the failure. A missing file reads as empty.
async fn read_output(output: &Path) -> String {
    let Ok(bytes) = cc_fs::read_bytes(output).await else {
        return String::new();
    };
    let start = bytes.len().saturating_sub(OUTPUT_LIMIT_BYTES);
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

/// A failure line wins over the exit code, since some tools exit 0 on failure. A launcher that
/// fails before the tool starts leaves no output, so its own stderr explains it.
fn rebuild_outcome(tool: RebuildTool, exit_code: i32, output: &str, stderr: &str) -> Result<()> {
    if let Some(line) = tool.failure_line(output) {
        return Err(anyhow!("Boot image rebuild failed: {line}"));
    }
    if exit_code == 0 {
        info!("Boot image rebuilt successfully");
        return Ok(());
    }
    let excerpt = if output.trim().is_empty() {
        tail_lines(stderr, ERROR_EXCERPT_LINES)
    } else {
        tail_lines(output, ERROR_EXCERPT_LINES)
    };
    Err(anyhow!(
        "Boot image rebuild failed with exit code {exit_code}: {excerpt}"
    ))
}

/// The edit that sets `karg`, from the kernel arguments as they are now.
async fn rpm_ostree_karg_edit_now(karg: &str) -> Result<String> {
    match DirectCommand::new(RPM_OSTREE_PATH, PROBE_TIMEOUT)
        .arg("kargs")
        .run()
        .await
    {
        ShellCommandResult::Success { stdout, .. } => Ok(rpm_ostree_karg_edit(&stdout, karg)),
        ShellCommandResult::Error(stderr) => {
            Err(anyhow!("Error reading rpm-ostree kargs: {stderr}"))
        }
    }
}

async fn set_rpm_ostree_karg(karg: &str) -> Result<()> {
    let edit = rpm_ostree_karg_edit_now(karg).await?;
    let (exit_code, stdout, stderr) = DirectCommand::new(RPM_OSTREE_PATH, RPM_OSTREE_TIMEOUT)
        .arg("kargs")
        .arg("--unchanged-exit-77")
        .arg(edit)
        .run_with_code()
        .await?;
    debug!("rpm-ostree kargs output: {stdout} - {stderr}");
    if exit_code == 0 || exit_code == RPM_OSTREE_UNCHANGED_EXIT_CODE {
        Ok(())
    } else {
        Err(anyhow!("Error running rpm-ostree kargs: {stderr}"))
    }
}

fn karg_key(karg: &str) -> &str {
    karg.split_once('=').map_or(karg, |(key, _)| key)
}

/// `--append-if-missing` compares the whole `key=value`, so an existing mask, as Bazzite sets,
/// would be duplicated. Replace the key when present instead.
fn rpm_ostree_karg_edit(current_kargs: &str, karg: &str) -> String {
    let key = karg_key(karg);
    let key_present = current_kargs
        .split_ascii_whitespace()
        .any(|arg| karg_key(arg) == key);
    if key_present {
        format!("--replace={karg}")
    } else {
        format!("--append={karg}")
    }
}

fn tail_lines(text: &str, line_count: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(line_count);
    lines[start..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "gated-tests")]
    use tokio_util::sync::CancellationToken;

    // Verify the module parameter parses as the kernel prints it, and only with its hex prefix.
    #[test]
    fn the_pp_feature_mask_parses_as_the_kernel_prints_it() {
        assert_eq!(parse_pp_feature_mask("0xfff7bfff\n").unwrap(), 0xfff7_bfff);
        assert!(parse_pp_feature_mask("fff7bfff\n").is_err());
        assert!(parse_pp_feature_mask("0xzz\n").is_err());
    }

    // Verify OR'ing a zero mask produces exactly the overdrive bit.
    #[test]
    fn compute_overdrive_mask_sets_bit() {
        assert_eq!(compute_overdrive_mask(0x0), 0x4000);
    }

    // Verify existing bits are preserved when overdrive bit is added.
    #[test]
    fn compute_overdrive_mask_preserves_existing_bits() {
        assert_eq!(compute_overdrive_mask(0xffff_bfff), 0xffff_ffff);
    }

    // Verify the mask is idempotent when the overdrive bit is already set.
    #[test]
    fn compute_overdrive_mask_idempotent_when_already_set() {
        assert_eq!(compute_overdrive_mask(0xffff_ffff), 0xffff_ffff);
        assert_eq!(compute_overdrive_mask(0x4000), 0x4000);
    }

    // Verify an existing mask key is replaced, as on Bazzite, and a missing one appended. A
    // similar but different key must not count as present.
    #[test]
    fn rpm_ostree_replaces_an_existing_mask() {
        let karg = "amdgpu.ppfeaturemask=0xFFFFFFFF";
        let bazzite = "rhgb quiet amdgpu.ppfeaturemask=0xfffd7fff";
        assert_eq!(
            rpm_ostree_karg_edit(bazzite, karg),
            "--replace=amdgpu.ppfeaturemask=0xFFFFFFFF"
        );
        let silverblue = "rhgb quiet amdgpu.ppfeaturemask_extra=1";
        assert_eq!(
            rpm_ostree_karg_edit(silverblue, karg),
            "--append=amdgpu.ppfeaturemask=0xFFFFFFFF"
        );
    }

    // Verify each refusal names the route the user should take instead.
    #[test]
    fn refusals_name_the_route_to_take() {
        assert!(refusal_message(Refusal::NixOs, 0x4000).contains("configuration.nix"));
        assert!(refusal_message(Refusal::Guix, 0x4000).contains("amdgpu.ppfeaturemask=0x4000"));
        assert!(refusal_message(Refusal::SteamOs, 0x4000).contains("kernel command line"));
        assert!(refusal_message(Refusal::Ostree, 0x4000)
            .contains("edit-in-place --append-if-missing=amdgpu.ppfeaturemask=0x4000"));
        let competing = refusal_message(
            Refusal::CompetingTools(RebuildTool::Mkinitcpio, RebuildTool::ArchDracutHook),
            0x4000,
        );
        assert!(competing.contains("mkinitcpio and arch-dracut-hook"));
        assert!(competing.contains("options amdgpu ppfeaturemask=0x4000 to /etc/modprobe.d/"));
    }

    // Verify the error excerpt keeps the last lines, where build tools put the failure.
    #[test]
    fn error_excerpt_keeps_the_last_lines() {
        assert_eq!(tail_lines("a\nb\nc\nd", 2), "c\nd");
        assert_eq!(tail_lines("a", 5), "a");
        assert_eq!(tail_lines("", 5), "");
    }

    // Verify the state table: a run or failure outranks the disk, the loaded module decides
    // active, and a conf it does not reflect yet means a reboot is pending, also after a restart.
    #[test]
    fn the_state_comes_from_the_job_then_the_disk() {
        let idle = Job::default();
        let running = Job {
            phase: JobPhase::Applying,
            ..Job::default()
        };
        let failed = Job {
            phase: JobPhase::Failed,
            ..Job::default()
        };
        let done = Job {
            done: true,
            ..Job::default()
        };
        assert_eq!(derive_state(&running, true, true), OverdriveState::Applying);
        assert_eq!(derive_state(&failed, false, true), OverdriveState::Failed);
        assert_eq!(derive_state(&idle, true, false), OverdriveState::Active);
        assert_eq!(
            derive_state(&idle, false, true),
            OverdriveState::RebootRequired
        );
        assert_eq!(
            derive_state(&done, false, false),
            OverdriveState::RebootRequired
        );
        assert_eq!(
            derive_state(&idle, false, false),
            OverdriveState::NotConfigured
        );
    }

    // Verify only a state with nothing applied, or a failure, starts a run.
    #[test]
    fn only_unconfigured_or_failed_setups_run() {
        assert!(run_needed(OverdriveState::NotConfigured));
        assert!(run_needed(OverdriveState::Failed));
        assert!(run_needed(OverdriveState::Applying).not());
        assert!(run_needed(OverdriveState::RebootRequired).not());
        assert!(run_needed(OverdriveState::Active).not());
    }

    // Verify a failure keeps its error and the command to finish by hand, and a success
    // leaves neither.
    #[test]
    fn a_finished_run_records_its_end() {
        let running = Job {
            phase: JobPhase::Applying,
            action: Some(OverdriveAction::Enable),
            ..Job::default()
        };
        let mut ok = running.clone();
        finish_run(&mut ok, &Ok(()), Some("/usr/bin/mkinitcpio -P".to_owned()));
        assert_eq!(ok.phase, JobPhase::Idle);
        assert!(ok.done);
        assert_eq!(ok.manual_command, None);
        let mut failed = running;
        finish_run(
            &mut failed,
            &Err(anyhow!("no space")),
            Some("/usr/bin/mkinitcpio -P".to_owned()),
        );
        assert_eq!(failed.phase, JobPhase::Failed);
        assert_eq!(failed.error.as_deref(), Some("no space"));
        assert_eq!(
            failed.manual_command.as_deref(),
            Some("/usr/bin/mkinitcpio -P")
        );
    }

    // Verify the rebuild is launched detached, in its own unit or session, with shutdown held
    // off only where logind can do it, and with the tool's output sent to the file.
    #[test]
    fn the_rebuild_runs_detached() {
        let output = Path::new("/run/coolercontrold-amd-overdrive/rebuild.log");
        let unit = rebuild_argv("/usr/bin/mkinitcpio -P", output, true, true);
        assert_eq!(unit[0], SYSTEMD_RUN_PATH);
        assert!(unit.contains(&"--wait".to_owned()));
        assert!(unit.contains(&format!("--unit={REBUILD_UNIT}")));
        assert!(unit.contains(&SYSTEMD_INHIBIT_PATH.to_owned()));
        assert_eq!(
            unit.last().unwrap(),
            "exec >'/run/coolercontrold-amd-overdrive/rebuild.log' 2>&1\n/usr/bin/mkinitcpio -P"
        );
        let session = rebuild_argv("/usr/bin/mkinitcpio -P", output, false, false);
        assert_eq!(session[..3], [SETSID_PATH, "--fork", "--wait"]);
        assert!(session.contains(&SYSTEMD_INHIBIT_PATH.to_owned()).not());
        assert_eq!(session[3..5], ["/bin/sh", "-c"]);
    }

    // Verify a failure line beats exit 0, a non-zero exit reports the output's tail, and a
    // launcher that failed before the tool ran reports its own stderr.
    #[test]
    fn a_rebuild_outcome_reads_the_output() {
        let tool = RebuildTool::LimineMkinitcpio;
        assert!(rebuild_outcome(tool, 0, "building\ndone\n", "").is_ok());
        let hidden = rebuild_outcome(tool, 0, "==> Build failed for kernel linux\n", "");
        assert!(hidden
            .unwrap_err()
            .to_string()
            .contains("failed for kernel"));
        let exit = rebuild_outcome(RebuildTool::Mkinitcpio, 1, "a\nb\nno space left", "");
        let exit = exit.unwrap_err().to_string();
        assert!(exit.contains("exit code 1"));
        assert!(exit.contains("no space left"));
        let launcher = rebuild_outcome(tool, 1, "", "Unit already loaded");
        assert!(launcher
            .unwrap_err()
            .to_string()
            .contains("Unit already loaded"));
    }

    // Verify the conf names the mask, and the dracut drop-in copies the conf at its real path.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn the_conf_and_drop_in_are_written_under_the_root() {
        crate::rt::test_runtime(async {
            let root = tempfile::tempdir().unwrap();
            let paths = OverdrivePaths::under(root.path());
            write_overdrive_conf(&paths, 0xfff7_ffff, true)
                .await
                .unwrap();
            let conf = std::fs::read_to_string(paths.conf()).unwrap();
            assert_eq!(conf, "options amdgpu ppfeaturemask=0xFFF7FFFF\n");
            let drop_in = std::fs::read_to_string(paths.drop_in()).unwrap();
            assert_eq!(
                drop_in,
                "install_items+=\" /etc/modprobe.d/99-amdgpu-overdrive.conf \"\n"
            );
            assert!(paths.conf_present().await);
        });
    }

    // Verify a real detached run reports the tool's exit status and output, through setsid
    // as on a system without systemd. The tool's output reaches the file, not the daemon.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_detached_run_reports_its_exit_status() {
        crate::rt::test_runtime(async {
            crate::sidecar::ensure_test_handle();
            let dir = tempfile::tempdir().unwrap();
            let output = dir.path().join(OUTPUT_FILE);
            let argv = rebuild_argv("echo building; exit 3", &output, false, false);
            let (exit_code, stdout, _) = run_argv(&argv, PROBE_TIMEOUT).await.unwrap();
            assert_eq!(exit_code, 3);
            assert_eq!(stdout, "");
            assert_eq!(read_output(&output).await, "building\n");
        });
    }

    /// A scratch Arch tree with mkinitcpio in effect and overdrive off, and a setup over it.
    #[cfg(feature = "gated-tests")]
    fn arch_tree() -> (tempfile::TempDir, OverdriveSetup) {
        let root = tempfile::tempdir().unwrap();
        let file = |path: &str, content: &str| {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        };
        file("sys/module/amdgpu/parameters/ppfeaturemask", "0xfff7bfff\n");
        file("usr/bin/pacman", "");
        file("etc/mkinitcpio.d/linux.preset", "");
        file("usr/share/libalpm/hooks/90-mkinitcpio-install.hook", "");
        let setup = OverdriveSetup::with_root(
            AmdGpuOverdriveHandle::new(CancellationToken::new()),
            NotificationHandle::new(CancellationToken::new()),
            root.path(),
        );
        (root, setup)
    }

    // Verify the plan names the tool's exact command and the conf it writes, and runs nothing.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_plan_names_the_command_and_files() {
        crate::rt::test_runtime(async {
            let (root, setup) = arch_tree();
            let plan = setup.plan().await.unwrap();
            assert_eq!(plan.route, Some(OverdriveRoute::Rebuild));
            assert_eq!(plan.command.as_deref(), Some("/usr/bin/mkinitcpio -P"));
            let conf = root.path().join("etc/modprobe.d/99-amdgpu-overdrive.conf");
            assert_eq!(plan.files, [conf.display().to_string()]);
            assert_eq!(plan.problem, None);
            assert!(conf.exists().not());
        });
    }

    // Verify a config-only enable writes the conf, starts no rebuild, and hands the user the
    // command, so the state waits for the rebuild and a reboot.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_config_only_enable_leaves_the_rebuild_to_the_user() {
        crate::rt::test_runtime(async {
            let (root, setup) = arch_tree();
            let status = setup.enable(false).await.unwrap();
            assert_eq!(status.state, OverdriveState::RebootRequired);
            assert!(status.rebuild_skipped);
            assert_eq!(status.tool.as_deref(), Some("mkinitcpio"));
            assert_eq!(
                status.manual_command.as_deref(),
                Some("/usr/bin/mkinitcpio -P")
            );
            let conf = root.path().join("etc/modprobe.d/99-amdgpu-overdrive.conf");
            assert!(conf.exists());
            // A second enable finds the setting staged and starts nothing.
            let again = setup.enable(true).await.unwrap();
            assert_eq!(again.state, OverdriveState::RebootRequired);
            assert!(again.rebuild_skipped);
        });
    }

    // Verify enabling with the module already in overdrive changes nothing on disk.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn an_active_overdrive_starts_no_run() {
        crate::rt::test_runtime(async {
            let root = tempfile::tempdir().unwrap();
            let mask_dir = root.path().join("sys/module/amdgpu/parameters");
            std::fs::create_dir_all(&mask_dir).unwrap();
            std::fs::write(mask_dir.join("ppfeaturemask"), "0xffffffff\n").unwrap();
            let setup = OverdriveSetup::with_root(
                AmdGpuOverdriveHandle::new(CancellationToken::new()),
                NotificationHandle::new(CancellationToken::new()),
                root.path(),
            );
            let status = setup.enable(true).await.unwrap();
            assert_eq!(status.state, OverdriveState::Active);
            assert_eq!(status.action, None);
            assert!(OverdrivePaths::under(root.path())
                .conf_present()
                .await
                .not());
        });
    }
}
