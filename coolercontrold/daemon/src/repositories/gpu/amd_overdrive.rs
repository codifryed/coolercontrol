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

use std::path::PathBuf;
use std::time::Duration;

use crate::cc_fs;
use crate::notifier::{notify_all_sessions, NotificationHandle, NotificationIcon};
use crate::repositories::utils::{ShellCommand, ShellCommandResult};
use crate::rt;
use anyhow::{anyhow, Context, Result};
use log::{debug, info, warn};

pub const PP_OVERDRIVE_MASK: u64 = 0x4000;
pub const PP_FEATURE_MASK_PATH: &str = "/sys/module/amdgpu/parameters/ppfeaturemask";
const MODPROBE_CONF_PATH: &str = "/etc/modprobe.d";
const AMDGPU_OVERDRIVE_CONF_FILE: &str = "99-amdgpu-overdrive.conf";
const OS_RELEASE_PATH: &str = "/etc/os-release";
const OSTREE_BOOTED_PATH: &str = "/run/ostree-booted";
const INITRAMFS_TIMEOUT: Duration = Duration::from_secs(120);

/// Computes the new ppfeaturemask with the overdrive bit enabled.
pub fn compute_overdrive_mask(current_mask: u64) -> u64 {
    current_mask | PP_OVERDRIVE_MASK
}

/// Checks whether the overdrive bit is set in the current ppfeaturemask.
pub async fn is_overdrive_enabled() -> bool {
    (get_pp_feature_mask().await.unwrap_or_default() & PP_OVERDRIVE_MASK) > 0
}

/// Returns the kernel boot parameter string to enable overdrive.
pub async fn get_fan_control_boot_option() -> String {
    if let Ok(current_mask) = get_pp_feature_mask().await {
        let new_mask = compute_overdrive_mask(current_mask);
        format!("amdgpu.ppfeaturemask=0x{new_mask:X}")
    } else {
        "amdgpu.ppfeaturemask=0xffffffff".to_owned()
    }
}

/// Reads and parses the current ppfeaturemask from sysfs.
pub async fn get_pp_feature_mask() -> Result<u64> {
    let ppfeaturemask = cc_fs::read_txt(PP_FEATURE_MASK_PATH).await?;
    let ppfeaturemask = ppfeaturemask
        .trim()
        .strip_prefix("0x")
        .context("Invalid ppfeaturemask")?;
    u64::from_str_radix(ppfeaturemask, 16).context("Invalid ppfeaturemask")
}

/// The method used to apply the amdgpu ppfeaturemask configuration.
#[derive(Debug, PartialEq, Eq)]
enum AmdgpuConfigurator {
    /// Write modprobe.d config and regenerate initramfs
    Modprobe(Option<InitramfsType>),
    /// Set kernel argument via rpm-ostree (atomic Fedora distros)
    RpmOstreeKarg,
}

#[derive(Debug, PartialEq, Eq)]
enum InitramfsType {
    Debian,
    Mkinitcpio,
    Dracut,
}

/// Parses os-release content and determines the appropriate configurator.
fn parse_amdgpu_configurator(
    os_release_content: &str,
    ostree_booted: bool,
) -> Result<AmdgpuConfigurator> {
    let mut id = String::new();
    let mut id_like = String::new();
    for line in os_release_content.lines() {
        if let Some(value) = line.strip_prefix("ID=") {
            id = value.trim_matches('"').to_lowercase();
        } else if let Some(value) = line.strip_prefix("ID_LIKE=") {
            id_like = value.trim_matches('"').to_lowercase();
        }
    }
    let ids: Vec<&str> = std::iter::once(id.as_str())
        // often used for base distros:
        .chain(id_like.split_ascii_whitespace())
        .collect();
    for distro_id in &ids {
        match *distro_id {
            // Debian family: uses update-initramfs
            "debian" | "ubuntu" | "pop" | "linuxmint" | "elementary" | "zorin" | "kali"
            | "raspbian" | "neon" => {
                return Ok(AmdgpuConfigurator::Modprobe(Some(InitramfsType::Debian)));
            }
            // Arch family: uses mkinitcpio
            "arch" | "cachyos" | "manjaro" | "endeavouros" | "garuda" | "steamos" | "artix" => {
                return Ok(AmdgpuConfigurator::Modprobe(Some(
                    InitramfsType::Mkinitcpio,
                )));
            }
            // Fedora with ostree: uses rpm-ostree kernel arguments
            "fedora" if ostree_booted => {
                return Ok(AmdgpuConfigurator::RpmOstreeKarg);
            }
            // Dracut family: Fedora, RHEL, SUSE, Gentoo, Void
            "fedora"
            | "nobara"
            | "ultramarine"
            | "rhel"
            | "centos"
            | "rocky"
            | "almalinux"
            | "opensuse-tumbleweed"
            | "opensuse-leap"
            | "opensuse"
            | "suse"
            | "sles"
            | "gentoo"
            | "void" => {
                return Ok(AmdgpuConfigurator::Modprobe(Some(InitramfsType::Dracut)));
            }
            // Declarative distros: must configure through system config
            "nixos" => {
                return Err(anyhow!(
                    "Overdrive should be configured through \
                    NixOS system configuration"
                ));
            }
            "guix" => {
                return Err(anyhow!(
                    "Overdrive should be configured through \
                    Guix system configuration"
                ));
            }
            _ => (),
        }
    }
    // Unknown distro: write modprobe.d but skip initramfs regen.
    warn!(
        "Unrecognized distro (ID={id}, ID_LIKE={id_like}). \
        Skipping initramfs regeneration. Please report your \
        distribution so we can add support."
    );
    Ok(AmdgpuConfigurator::Modprobe(None))
}

/// Detects the appropriate configurator for the current distro.
async fn detect_amdgpu_configurator() -> Result<AmdgpuConfigurator> {
    let os_release = cc_fs::read_txt(OS_RELEASE_PATH).await.unwrap_or_default();
    let ostree_booted = PathBuf::from(OSTREE_BOOTED_PATH).exists();
    parse_amdgpu_configurator(&os_release, ostree_booted)
}

/// Enables AMD GPU overdrive by configuring the ppfeaturemask kernel parameter.
/// Detects the distro and uses the appropriate method (modprobe.d or rpm-ostree kargs).
pub async fn amd_gpu_overdrive_enable(notification_handle: NotificationHandle) -> Result<String> {
    let current_mask = get_pp_feature_mask().await?;
    let new_mask = compute_overdrive_mask(current_mask);
    if new_mask == current_mask {
        return Ok("Overdrive is already enabled".to_owned());
    }

    let configurator = detect_amdgpu_configurator().await?;
    info!("Detected AMD GPU configurator: {configurator:?}");

    match configurator {
        AmdgpuConfigurator::Modprobe(initramfs_type) => {
            let conf = format!("options amdgpu ppfeaturemask=0x{new_mask:X}\n");
            let conf_path = PathBuf::from(MODPROBE_CONF_PATH).join(AMDGPU_OVERDRIVE_CONF_FILE);
            cc_fs::create_dir_all(MODPROBE_CONF_PATH).await?;
            cc_fs::write_string(&conf_path, conf).await?;
            info!("Wrote AMD GPU overdrive config to {}", conf_path.display());

            if let Some(initramfs) = initramfs_type {
                // Spawn in background so the API responds immediately.
                // The config file is already written, and a reboot is
                // required regardless.
                rt::spawn(async move {
                    let result = regenerate_initramfs(initramfs).await;
                    if result.is_ok() {
                        warn!(
                            "Initramfs regeneration complete. \
                            Please reboot to apply AMD GPU overdrive."
                        );
                        notify_all_sessions(
                            "Initramfs Regeneration Complete",
                            "Please reboot to apply AMD GPU overdrive.",
                            NotificationIcon::Info,
                            true,
                            None,
                            Some(&notification_handle),
                        );
                    } else if let Err(err) = result {
                        warn!(
                            "Initramfs regeneration failed: {err}. \
                            You may need to regenerate initramfs manually."
                        );
                        notify_all_sessions(
                            "Initramfs Regeneration Failed",
                            &format!(
                                "{err}. You may need to regenerate \
                                initramfs manually."
                            ),
                            NotificationIcon::Error,
                            true,
                            Some(2),
                            Some(&notification_handle),
                        );
                    }
                });
                Ok("AMD GPU overdrive configuration written. \
                    Initramfs is regenerating in the background \
                    - you will be notified when you can reboot."
                    .to_owned())
            } else {
                warn!(
                    "Unknown distro, skipping initramfs regeneration. \
                    You may need to regenerate initramfs manually \
                    if amdgpu is loaded from initramfs."
                );
                Ok("AMD GPU overdrive configuration written. \
                    Please reboot to apply."
                    .to_owned())
            }
        }
        AmdgpuConfigurator::RpmOstreeKarg => {
            let karg = format!("amdgpu.ppfeaturemask=0x{new_mask:X}");
            let command = format!("rpm-ostree kargs --append-if-missing={karg}");
            let result = ShellCommand::new(&command, Duration::from_secs(30))
                .run()
                .await;
            match result {
                ShellCommandResult::Error(stderr) => {
                    Err(anyhow!("Error running rpm-ostree kargs: {stderr}"))
                }
                ShellCommandResult::Success { stdout, stderr } => {
                    debug!("rpm-ostree kargs output: {stdout} - {stderr}");
                    Ok("AMD GPU overdrive kernel argument added via rpm-ostree. \
                        Please reboot to apply."
                        .to_owned())
                }
            }
        }
    }
}

async fn regenerate_initramfs(initramfs_type: InitramfsType) -> Result<()> {
    info!("Regenerating initramfs ({initramfs_type:?})");
    let command = match initramfs_type {
        InitramfsType::Debian => "update-initramfs -u -q",
        InitramfsType::Mkinitcpio => "mkinitcpio -P",
        InitramfsType::Dracut => "dracut --regenerate-all --force --quiet",
    };
    let result = ShellCommand::new(command, INITRAMFS_TIMEOUT).run().await;
    match result {
        ShellCommandResult::Error(stderr) => Err(anyhow!("Error regenerating initramfs: {stderr}")),
        ShellCommandResult::Success { stdout, stderr } => {
            debug!("Initramfs regen output: {stdout} - {stderr}");
            if stderr.is_empty() {
                info!("Initramfs regenerated successfully");
                Ok(())
            } else {
                warn!("Initramfs regeneration produced output on stderr: {stderr}");
                // Some tools write warnings to stderr but succeed.
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        compute_overdrive_mask, parse_amdgpu_configurator, AmdgpuConfigurator, InitramfsType,
    };

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

    // Verify Debian is detected from ID field and uses update-initramfs.
    #[test]
    fn detect_debian() {
        let os_release = "ID=debian\nVERSION_ID=\"12\"\n";
        let result = parse_amdgpu_configurator(os_release, false).unwrap();
        assert_eq!(
            result,
            AmdgpuConfigurator::Modprobe(Some(InitramfsType::Debian))
        );
    }

    // Verify Ubuntu is detected via ID_LIKE=debian fallback.
    #[test]
    fn detect_ubuntu_via_id_like() {
        let os_release = "ID=ubuntu\nID_LIKE=debian\nVERSION_ID=\"24.04\"\n";
        let result = parse_amdgpu_configurator(os_release, false).unwrap();
        assert_eq!(
            result,
            AmdgpuConfigurator::Modprobe(Some(InitramfsType::Debian))
        );
    }

    // Verify Arch Linux is detected and uses mkinitcpio.
    #[test]
    fn detect_arch() {
        let os_release = "ID=arch\n";
        let result = parse_amdgpu_configurator(os_release, false).unwrap();
        assert_eq!(
            result,
            AmdgpuConfigurator::Modprobe(Some(InitramfsType::Mkinitcpio))
        );
    }

    // Verify CachyOS is detected via ID match (before ID_LIKE=arch).
    #[test]
    fn detect_cachyos() {
        let os_release = "ID=cachyos\nID_LIKE=\"arch\"\n";
        let result = parse_amdgpu_configurator(os_release, false).unwrap();
        assert_eq!(
            result,
            AmdgpuConfigurator::Modprobe(Some(InitramfsType::Mkinitcpio))
        );
    }

    // Verify non-atomic Fedora is detected and uses dracut.
    #[test]
    fn detect_fedora() {
        let os_release = "ID=fedora\nVERSION_ID=41\n";
        let result = parse_amdgpu_configurator(os_release, false).unwrap();
        assert_eq!(
            result,
            AmdgpuConfigurator::Modprobe(Some(InitramfsType::Dracut))
        );
    }

    // Verify atomic Fedora (ostree-booted) uses rpm-ostree kargs.
    #[test]
    fn detect_fedora_atomic() {
        let os_release = "ID=fedora\nVARIANT_ID=silverblue\n";
        let result = parse_amdgpu_configurator(os_release, true).unwrap();
        assert_eq!(result, AmdgpuConfigurator::RpmOstreeKarg);
    }

    // Verify Bazzite (ID_LIKE=fedora) with ostree uses rpm-ostree kargs.
    #[test]
    fn detect_bazzite_atomic() {
        let os_release = "ID=bazzite\nID_LIKE=\"fedora\"\n";
        let result = parse_amdgpu_configurator(os_release, true).unwrap();
        assert_eq!(result, AmdgpuConfigurator::RpmOstreeKarg);
    }

    // Verify Pop!_OS is detected via ID=pop and uses update-initramfs.
    #[test]
    fn detect_popos() {
        let os_release = "ID=pop\nID_LIKE=\"ubuntu debian\"\n";
        let result = parse_amdgpu_configurator(os_release, false).unwrap();
        assert_eq!(
            result,
            AmdgpuConfigurator::Modprobe(Some(InitramfsType::Debian))
        );
    }

    // Verify Manjaro is detected via ID=manjaro and uses mkinitcpio.
    #[test]
    fn detect_manjaro() {
        let os_release = "ID=manjaro\n";
        let result = parse_amdgpu_configurator(os_release, false).unwrap();
        assert_eq!(
            result,
            AmdgpuConfigurator::Modprobe(Some(InitramfsType::Mkinitcpio))
        );
    }

    // Verify SteamOS is detected via ID=steamos and uses mkinitcpio.
    #[test]
    fn detect_steamos() {
        let os_release = "ID=steamos\nID_LIKE=arch\n";
        let result = parse_amdgpu_configurator(os_release, false).unwrap();
        assert_eq!(
            result,
            AmdgpuConfigurator::Modprobe(Some(InitramfsType::Mkinitcpio))
        );
    }

    // Verify Nobara is detected via ID=nobara and uses dracut.
    #[test]
    fn detect_nobara() {
        let os_release = "ID=nobara\nID_LIKE=fedora\n";
        let result = parse_amdgpu_configurator(os_release, false).unwrap();
        assert_eq!(
            result,
            AmdgpuConfigurator::Modprobe(Some(InitramfsType::Dracut))
        );
    }

    // Verify openSUSE Tumbleweed is detected and uses dracut.
    #[test]
    fn detect_opensuse_tumbleweed() {
        let os_release = "ID=opensuse-tumbleweed\nID_LIKE=\"opensuse suse\"\n";
        let result = parse_amdgpu_configurator(os_release, false).unwrap();
        assert_eq!(
            result,
            AmdgpuConfigurator::Modprobe(Some(InitramfsType::Dracut))
        );
    }

    // Verify Gentoo is detected via ID=gentoo and uses dracut.
    #[test]
    fn detect_gentoo() {
        let os_release = "ID=gentoo\n";
        let result = parse_amdgpu_configurator(os_release, false).unwrap();
        assert_eq!(
            result,
            AmdgpuConfigurator::Modprobe(Some(InitramfsType::Dracut))
        );
    }

    // Verify Void Linux is detected via ID=void and uses dracut.
    #[test]
    fn detect_void() {
        let os_release = "ID=void\n";
        let result = parse_amdgpu_configurator(os_release, false).unwrap();
        assert_eq!(
            result,
            AmdgpuConfigurator::Modprobe(Some(InitramfsType::Dracut))
        );
    }

    // Verify NixOS returns an error directing to system configuration.
    #[test]
    fn detect_nixos_returns_error() {
        let os_release = "ID=nixos\n";
        let result = parse_amdgpu_configurator(os_release, false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("NixOS system configuration"));
    }

    // Verify Guix returns an error directing to system configuration.
    #[test]
    fn detect_guix_returns_error() {
        let os_release = "ID=guix\n";
        let result = parse_amdgpu_configurator(os_release, false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Guix system configuration"));
    }

    // Verify unknown distros get modprobe.d config without initramfs regen.
    #[test]
    fn detect_unknown_distro() {
        let os_release = "ID=slackware\n";
        let result = parse_amdgpu_configurator(os_release, false).unwrap();
        assert_eq!(result, AmdgpuConfigurator::Modprobe(None));
    }

    // Verify empty os-release falls back to modprobe.d without initramfs regen.
    #[test]
    fn detect_empty_os_release() {
        let result = parse_amdgpu_configurator("", false).unwrap();
        assert_eq!(result, AmdgpuConfigurator::Modprobe(None));
    }
}
