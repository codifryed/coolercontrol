// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cc_fs;
use crate::events::notifier::{notify_all_sessions, NotificationHandle, NotificationIcon};
use crate::repositories::gpu::boot_image::{self, BootFacts, BootSetup, RebuildTool, Refusal};
use crate::repositories::utils::{DirectCommand, ShellCommand, ShellCommandResult};
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
const RPM_OSTREE_PATH: &str = "/usr/bin/rpm-ostree";
/// Rebuilding every kernel's image, with fallbacks or UKIs, can take minutes. A kill mid-build
/// leaves a half-written image, so the bound is generous.
const BOOT_IMAGE_TIMEOUT: Duration = Duration::from_secs(600);
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const ERROR_EXCERPT_LINES: usize = 5;
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
    (get_pp_feature_mask().await.unwrap_or_default() & PP_OVERDRIVE_MASK) > 0
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
    let ppfeaturemask =
        rt::spawn_blocking(|| cc_fs::read_bytes_blocking(Path::new(PP_FEATURE_MASK_PATH)))
            .await??;
    parse_pp_feature_mask(&String::from_utf8(ppfeaturemask)?)
}

fn parse_pp_feature_mask(ppfeaturemask: &str) -> Result<u64> {
    let ppfeaturemask = ppfeaturemask
        .trim()
        .strip_prefix("0x")
        .context("Invalid ppfeaturemask")?;
    u64::from_str_radix(ppfeaturemask, 16).context("Invalid ppfeaturemask")
}

/// Reads the boot image facts of this system off the runtime thread.
async fn detect_boot_facts() -> Result<BootFacts> {
    let mut facts =
        rt::spawn_blocking(|| boot_image::gather_boot_facts_blocking(Path::new("/"))).await?;
    if facts.sdbootutil {
        let result = DirectCommand::new("/usr/bin/sdbootutil", PROBE_TIMEOUT)
            .arg("is-installed")
            .run()
            .await;
        facts.sdbootutil_installed = matches!(result, ShellCommandResult::Success { .. });
    }
    Ok(facts)
}

/// Enables AMD GPU overdrive by configuring the ppfeaturemask kernel parameter, by whichever
/// route the system's boot image tooling supports.
pub async fn amd_gpu_overdrive_enable(notification_handle: NotificationHandle) -> Result<String> {
    let current_mask = get_pp_feature_mask().await?;
    let new_mask = compute_overdrive_mask(current_mask);
    if new_mask == current_mask {
        return Ok("Overdrive is already enabled".to_owned());
    }
    assert!(new_mask & PP_OVERDRIVE_MASK > 0);

    let facts = detect_boot_facts().await?;
    let setup = boot_image::classify(&facts);
    info!("Detected AMD GPU overdrive boot setup: {setup:?}");
    match setup {
        BootSetup::Refused(refusal) => Err(anyhow!(refusal_message(refusal, new_mask))),
        BootSetup::RpmOstree => {
            set_rpm_ostree_karg(&overdrive_karg(new_mask)).await?;
            Ok("AMD GPU overdrive kernel argument set. Please reboot to apply.".to_owned())
        }
        BootSetup::Rebuild(tool) => {
            write_overdrive_conf(new_mask, facts.dracut).await?;
            spawn_boot_image_rebuild(tool, notification_handle);
            Ok(format!(
                "AMD GPU overdrive configuration written. The boot image is rebuilding in the \
                background with {}. You will be notified when you can reboot.",
                tool.command()
            ))
        }
        BootSetup::Unrecognized => {
            write_overdrive_conf(new_mask, facts.dracut).await?;
            warn!("Unrecognized boot image tooling, skipping the boot image rebuild.");
            Ok(
                "AMD GPU overdrive configuration written. CoolerControl did not recognize how \
                this system builds its boot image. If amdgpu loads from the initramfs, rebuild \
                it before rebooting. Otherwise, just reboot."
                    .to_owned(),
            )
        }
    }
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
async fn write_overdrive_conf(new_mask: u64, dracut: bool) -> Result<()> {
    let conf_path = PathBuf::from(MODPROBE_CONF_PATH).join(AMDGPU_OVERDRIVE_CONF_FILE);
    cc_fs::create_dir_all(MODPROBE_CONF_PATH).await?;
    cc_fs::write_string(&conf_path, overdrive_conf(new_mask)).await?;
    info!("Wrote AMD GPU overdrive config to {}", conf_path.display());
    if dracut {
        let drop_in = format!("install_items+=\" {} \"\n", conf_path.display());
        let drop_in_path = PathBuf::from(DRACUT_CONF_PATH).join(DRACUT_OVERDRIVE_CONF_FILE);
        cc_fs::create_dir_all(DRACUT_CONF_PATH).await?;
        cc_fs::write_string(&drop_in_path, drop_in).await?;
        info!("Wrote dracut drop-in to {}", drop_in_path.display());
    }
    Ok(())
}

/// Runs the rebuild in the background so the API responds immediately. The config file is
/// already written, and a reboot is required regardless.
fn spawn_boot_image_rebuild(tool: RebuildTool, notification_handle: NotificationHandle) {
    rt::spawn(async move {
        match rebuild_boot_image(tool).await {
            Ok(()) => {
                warn!("Boot image rebuild complete. Please reboot to apply AMD GPU overdrive.");
                notify_all_sessions(
                    "Boot Image Rebuild Complete",
                    "Please reboot to apply AMD GPU overdrive.",
                    NotificationIcon::Info,
                    true,
                    None,
                    Some(&notification_handle),
                );
            }
            Err(err) => {
                let body = format!(
                    "{err}. The configuration is written. Run {} manually, then reboot.",
                    tool.command()
                );
                warn!("Boot image rebuild failed: {body}");
                notify_all_sessions(
                    "Boot Image Rebuild Failed",
                    &body,
                    NotificationIcon::Error,
                    true,
                    Some(2),
                    Some(&notification_handle),
                );
            }
        }
    });
}

async fn rebuild_boot_image(tool: RebuildTool) -> Result<()> {
    info!("Rebuilding the boot image with: {}", tool.command());
    let result = ShellCommand::new(tool.command(), BOOT_IMAGE_TIMEOUT)
        .run()
        .await;
    match result {
        ShellCommandResult::Error(stderr) => Err(anyhow!(
            "Boot image rebuild failed: {}",
            tail_lines(&stderr, ERROR_EXCERPT_LINES)
        )),
        ShellCommandResult::Success { stdout, stderr } => {
            debug!("Boot image rebuild output: {stdout} - {stderr}");
            let failure_line = tool
                .failure_line(&stderr)
                .or_else(|| tool.failure_line(&stdout));
            if let Some(line) = failure_line {
                return Err(anyhow!("Boot image rebuild failed: {line}"));
            }
            info!("Boot image rebuilt successfully");
            Ok(())
        }
    }
}

/// `--append-if-missing` compares the whole `key=value`, so an existing mask, as Bazzite sets,
/// would be duplicated. Replace the key when present instead.
async fn set_rpm_ostree_karg(karg: &str) -> Result<()> {
    let current_kargs = match DirectCommand::new(RPM_OSTREE_PATH, PROBE_TIMEOUT)
        .arg("kargs")
        .run()
        .await
    {
        ShellCommandResult::Success { stdout, .. } => stdout,
        ShellCommandResult::Error(stderr) => {
            return Err(anyhow!("Error reading rpm-ostree kargs: {stderr}"));
        }
    };
    let edit = rpm_ostree_karg_edit(&current_kargs, karg);
    let (exit_code, stdout, stderr) = DirectCommand::new(RPM_OSTREE_PATH, BOOT_IMAGE_TIMEOUT)
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
}
