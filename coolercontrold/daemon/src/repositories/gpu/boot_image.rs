// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Picks how a kernel module option reaches the next boot, from the boot image tooling actually
//! in use on disk. The distro ID alone is not enough: Arch derivatives ship mkinitcpio, dracut,
//! limine hooks or kernel-install, and Debian ships either initramfs-tools or dracut.

use std::ops::Not;
use std::path::Path;

use crate::cc_fs;

/// Bounds the per-directory scans so a strange tree cannot stall detection.
const MAX_DIR_ENTRIES: usize = 256;
const _: () = assert!(MAX_DIR_ENTRIES > 0);

/// Lines that mean failure even when the tool exits 0.
const LIMINE_FAILURE_MARKERS: [&str; 1] = ["failed for kernel"];
const UPDATE_INITRAMFS_FAILURE_MARKERS: [&str; 2] = ["Not updating initramfs", "is ro mounted"];

/// What is on disk, gathered once so classification stays pure.
// Independent observations of the disk, not a state machine.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BootFacts {
    pub os_id: String,
    pub ostree_booted: bool,
    pub rpm_ostree: bool,
    pub steamos: bool,
    pub root_read_only: bool,
    pub transactional_update: bool,
    pub pacman: bool,
    pub limine_mkinitcpio: bool,
    pub limine_dracut: bool,
    pub reinstall_kernels: bool,
    pub kernel_install: bool,
    pub kernel_install_layout: Option<String>,
    pub kernel_install_pacman_script: bool,
    pub dracut_rebuild: bool,
    pub mkinitcpio_presets: bool,
    pub mkinitcpio_hook_active: bool,
    pub arch_dracut_hook_active: bool,
    pub update_initramfs: bool,
    pub debian_dracut_postinst: bool,
    pub sdbootutil: bool,
    pub sdbootutil_installed: bool,
    pub dracut: bool,
    pub running_kernel_in_boot: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootSetup {
    Refused(Refusal),
    /// The image initrd is built upstream, so the mask goes on the kernel command line.
    RpmOstree,
    Rebuild(RebuildTool),
    /// The conf is written, but nothing known rebuilds the boot image.
    Unrecognized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    NixOs,
    Guix,
    /// `/etc` changes are discarded on every OS update.
    SteamOs,
    /// Plain ostree has no command that replaces a kernel argument, so it is left to the user.
    Ostree,
    /// Both write the image the bootloader loads, and nothing on disk says which ran last.
    CompetingTools(RebuildTool, RebuildTool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebuildTool {
    TransactionalUpdate,
    LimineMkinitcpio,
    LimineDracut,
    ReinstallKernels,
    KernelInstall,
    DracutRebuild,
    Mkinitcpio,
    ArchDracutHook,
    UpdateInitramfs,
    DebianDracut,
    Sdbootutil,
    Dracut,
}

impl RebuildTool {
    /// Stable name for clients.
    pub fn id(self) -> &'static str {
        match self {
            Self::TransactionalUpdate => "transactional-update",
            Self::LimineMkinitcpio => "limine-mkinitcpio",
            Self::LimineDracut => "limine-dracut",
            Self::ReinstallKernels => "reinstall-kernels",
            Self::KernelInstall => "kernel-install",
            Self::DracutRebuild => "dracut-rebuild",
            Self::Mkinitcpio => "mkinitcpio",
            Self::ArchDracutHook => "arch-dracut-hook",
            Self::UpdateInitramfs => "update-initramfs",
            Self::DebianDracut => "debian-dracut",
            Self::Sdbootutil => "sdbootutil",
            Self::Dracut => "dracut",
        }
    }

    /// The shell command that rebuilds the image of every installed kernel. Tools are called by
    /// absolute path: `limine-mkinitcpio-hook` puts an interactive `mkinitcpio` wrapper first
    /// on the systemd PATH.
    pub fn command(self) -> &'static str {
        match self {
            Self::TransactionalUpdate => {
                // --continue keeps a pending update; without it the update is discarded.
                "/usr/sbin/transactional-update --no-selfupdate --continue initrd"
            }
            Self::LimineMkinitcpio => "/usr/bin/limine-mkinitcpio",
            Self::LimineDracut => "/usr/bin/limine-dracut",
            Self::ReinstallKernels => "/usr/bin/reinstall-kernels",
            Self::KernelInstall => "/usr/bin/kernel-install add-all",
            Self::DracutRebuild => "/usr/bin/dracut-rebuild",
            Self::Mkinitcpio => "/usr/bin/mkinitcpio -P",
            // The pacman hook script has no rebuild-all mode; it builds the kernels it is fed.
            Self::ArchDracutHook => {
                "for f in /usr/lib/modules/*/pkgbase; do echo \"${f#/}\"; done \
                | /usr/share/libalpm/scripts/dracut-install"
            }
            Self::UpdateInitramfs => "/usr/sbin/update-initramfs -u -k all",
            // Debian 13's dracut has no update-initramfs and writes a file GRUB never loads
            // unless the output name is given, as its kernel hook does.
            Self::DebianDracut => {
                "rc=0; for k in /boot/vmlinuz-*; do v=\"${k#/boot/vmlinuz-}\"; \
                /usr/bin/dracut --force \"/boot/initrd.img-$v\" \"$v\" || rc=1; done; exit $rc"
            }
            Self::Sdbootutil => "/usr/bin/sdbootutil mkinitrd",
            Self::Dracut => "/usr/bin/dracut --regenerate-all --force",
        }
    }

    /// Output that means the rebuild failed although the tool exited 0.
    pub fn failure_markers(self) -> &'static [&'static str] {
        match self {
            Self::LimineMkinitcpio | Self::LimineDracut => &LIMINE_FAILURE_MARKERS,
            Self::UpdateInitramfs => &UPDATE_INITRAMFS_FAILURE_MARKERS,
            _ => &[],
        }
    }

    /// Finds the first line of `output` carrying a failure marker.
    pub fn failure_line(self, output: &str) -> Option<&str> {
        let markers = self.failure_markers();
        output
            .lines()
            .find(|line| markers.iter().any(|marker| line.contains(marker)))
            .map(str::trim)
    }
}

/// Picks the setup. First match wins, most specific first.
pub fn classify(facts: &BootFacts) -> BootSetup {
    let setup = classify_unchecked(facts);
    match setup {
        BootSetup::Rebuild(_) => assert!(facts.ostree_booted.not(), "ostree images are upstream"),
        BootSetup::RpmOstree => {
            assert!(facts.ostree_booted);
            assert!(facts.rpm_ostree);
        }
        BootSetup::Refused(Refusal::CompetingTools(first, second)) => assert_ne!(first, second),
        BootSetup::Refused(_) | BootSetup::Unrecognized => {}
    }
    setup
}

fn classify_unchecked(facts: &BootFacts) -> BootSetup {
    if let Some(refusal) = refusal(facts) {
        return BootSetup::Refused(refusal);
    }
    if facts.ostree_booted {
        // The image initrd is built upstream, so a local /etc file never reaches it.
        return if facts.rpm_ostree {
            BootSetup::RpmOstree
        } else {
            BootSetup::Refused(Refusal::Ostree)
        };
    }
    if facts.root_read_only {
        if facts.transactional_update {
            return BootSetup::Rebuild(RebuildTool::TransactionalUpdate);
        }
    }
    if let Some((first, second)) = competing_tools(facts) {
        return BootSetup::Refused(Refusal::CompetingTools(first, second));
    }
    let tool = if facts.pacman {
        arch_family_tool(facts)
    } else {
        None
    };
    match tool.or_else(|| other_tool(facts)) {
        Some(tool) => BootSetup::Rebuild(tool),
        None => BootSetup::Unrecognized,
    }
}

fn refusal(facts: &BootFacts) -> Option<Refusal> {
    match facts.os_id.as_str() {
        "nixos" => Some(Refusal::NixOs),
        "guix" => Some(Refusal::Guix),
        _ if facts.steamos => Some(Refusal::SteamOs),
        _ => None,
    }
}

/// Two generators whose pacman hooks both rebuild the image on kernel updates. Picking one
/// would be a guess. The wrapper packages are not counted against mkinitcpio: they depend on it
/// and are known to be the one in effect.
fn competing_tools(facts: &BootFacts) -> Option<(RebuildTool, RebuildTool)> {
    if facts.pacman.not() {
        return None;
    }
    if facts.limine_mkinitcpio {
        if facts.limine_dracut {
            return Some((RebuildTool::LimineMkinitcpio, RebuildTool::LimineDracut));
        }
    }
    let wrapper = facts.limine_mkinitcpio
        || facts.limine_dracut
        || facts.reinstall_kernels
        || facts.kernel_install_pacman_script
        || facts.dracut_rebuild;
    if wrapper {
        return None;
    }
    if facts.mkinitcpio_presets {
        if facts.mkinitcpio_hook_active {
            if facts.arch_dracut_hook_active {
                return Some((RebuildTool::Mkinitcpio, RebuildTool::ArchDracutHook));
            }
        }
    }
    None
}

/// Arch derivatives: the pacman hook in effect decides, not which binaries are installed.
/// mkinitcpio is a hard dependency of the limine hook, and presets can be left over from before
/// it was installed, so the hook packages come first.
fn arch_family_tool(facts: &BootFacts) -> Option<RebuildTool> {
    if facts.limine_mkinitcpio {
        return Some(RebuildTool::LimineMkinitcpio);
    }
    if facts.limine_dracut {
        return Some(RebuildTool::LimineDracut);
    }
    if facts.reinstall_kernels {
        return Some(RebuildTool::ReinstallKernels);
    }
    if facts.kernel_install_pacman_script {
        if layout_is(facts, &["bls", "uki"]) {
            return Some(RebuildTool::KernelInstall);
        }
    }
    if facts.dracut_rebuild {
        return Some(RebuildTool::DracutRebuild);
    }
    if facts.mkinitcpio_presets {
        if facts.mkinitcpio_hook_active {
            return Some(RebuildTool::Mkinitcpio);
        }
    }
    if facts.arch_dracut_hook_active {
        return Some(RebuildTool::ArchDracutHook);
    }
    None
}

fn other_tool(facts: &BootFacts) -> Option<RebuildTool> {
    if facts.update_initramfs {
        return Some(RebuildTool::UpdateInitramfs);
    }
    if facts.debian_dracut_postinst {
        return Some(RebuildTool::DebianDracut);
    }
    if facts.sdbootutil_installed {
        return Some(RebuildTool::Sdbootutil);
    }
    // Only for UKIs: on BLS, kernel-install also rewrites boot entries, and dracut alone is
    // enough.
    if facts.kernel_install {
        if layout_is(facts, &["uki"]) {
            return Some(RebuildTool::KernelInstall);
        }
    }
    // dracut's default output name matches what the bootloader loads only where kernels are
    // named by version in /boot (Fedora, RHEL, openSUSE). Arch names them by package.
    if facts.dracut {
        if facts.running_kernel_in_boot {
            return Some(RebuildTool::Dracut);
        }
    }
    None
}

fn layout_is(facts: &BootFacts, layouts: &[&str]) -> bool {
    assert!(layouts.is_empty().not());
    facts
        .kernel_install_layout
        .as_deref()
        .is_some_and(|layout| layouts.contains(&layout))
}

/// Reads the facts under `root` (`/` outside tests) with blocking calls. Run it off the runtime
/// thread. `sdbootutil_installed` needs a command and is left for the caller.
pub fn gather_boot_facts_blocking(root: &Path) -> BootFacts {
    assert!(root.is_absolute());
    let at = |path: &str| root.join(path);
    let exists = |path: &str| cc_fs::exists(at(path));
    let read = |path: &str| read_text(&at(path));
    let kernel_install_conf = read("etc/kernel/install.conf")
        .or_else(|| read("usr/lib/kernel/install.conf"))
        .unwrap_or_default();
    let running_kernel = read("proc/sys/kernel/osrelease").unwrap_or_default();
    let running_kernel = running_kernel.trim();
    BootFacts {
        os_id: parse_os_release_id(&read("etc/os-release").unwrap_or_default()),
        ostree_booted: exists("run/ostree-booted"),
        rpm_ostree: exists("usr/bin/rpm-ostree"),
        steamos: exists("usr/bin/steamos-readonly"),
        root_read_only: parse_fstab_root_read_only(&read("etc/fstab").unwrap_or_default()),
        transactional_update: exists("usr/sbin/transactional-update"),
        pacman: exists("usr/bin/pacman"),
        limine_mkinitcpio: exists("usr/bin/limine-mkinitcpio"),
        limine_dracut: exists("usr/bin/limine-dracut"),
        reinstall_kernels: exists("usr/bin/reinstall-kernels"),
        kernel_install: exists("usr/bin/kernel-install"),
        kernel_install_layout: parse_kernel_install_layout(&kernel_install_conf),
        kernel_install_pacman_script: exists("usr/share/libalpm/scripts/kernel-install"),
        dracut_rebuild: exists("usr/bin/dracut-rebuild"),
        mkinitcpio_presets: dir_has_extension(&at("etc/mkinitcpio.d"), "preset"),
        mkinitcpio_hook_active: hook_active(root, "90-mkinitcpio-install.hook"),
        arch_dracut_hook_active: hook_active(root, "90-dracut-install.hook")
            && exists("usr/share/libalpm/scripts/dracut-install"),
        update_initramfs: exists("usr/sbin/update-initramfs"),
        debian_dracut_postinst: exists("etc/kernel/postinst.d/dracut"),
        sdbootutil: exists("usr/bin/sdbootutil"),
        sdbootutil_installed: false,
        dracut: exists("usr/bin/dracut"),
        running_kernel_in_boot: running_kernel.is_empty().not()
            && exists(&format!("boot/vmlinuz-{running_kernel}")),
    }
}

/// A pacman hook is in effect when the package ships it and `/etc/pacman.d/hooks` has no file
/// of the same name. Such a file replaces it, and a `/dev/null` link masks it.
fn hook_active(root: &Path, hook_name: &str) -> bool {
    assert!(hook_name.is_empty().not());
    let shipped = root.join("usr/share/libalpm/hooks").join(hook_name);
    let overridden = root.join("etc/pacman.d/hooks").join(hook_name);
    cc_fs::exists(shipped) && cc_fs::exists(overridden).not()
}

fn read_text(path: &Path) -> Option<String> {
    let bytes = cc_fs::read_bytes_blocking(path).ok()?;
    String::from_utf8(bytes).ok()
}

fn dir_has_extension(dir: &Path, extension: &str) -> bool {
    assert!(extension.starts_with('.').not(), "no leading dot");
    let Ok(entries) = cc_fs::read_dir(dir) else {
        return false;
    };
    entries
        .take(MAX_DIR_ENTRIES)
        .filter_map(Result::ok)
        .any(|entry| entry.path().extension().is_some_and(|ext| ext == extension))
}

pub fn parse_os_release_id(os_release: &str) -> String {
    os_release
        .lines()
        .find_map(|line| line.strip_prefix("ID="))
        .map(|id| id.trim().trim_matches('"').to_lowercase())
        .unwrap_or_default()
}

/// Whether `/` is mounted read-only, as transactional systems declare in fstab.
pub fn parse_fstab_root_read_only(fstab: &str) -> bool {
    fstab
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('#').not())
        .filter_map(|line| {
            let mut fields = line.split_ascii_whitespace();
            let mount_point = fields.nth(1)?;
            let options = fields.nth(1)?;
            Some((mount_point, options))
        })
        .filter(|(mount_point, _)| *mount_point == "/")
        // MicroOS and Aeon write `ro=vfs`, read-only for the mount but not the filesystem.
        .any(|(_, options)| {
            options
                .split(',')
                .any(|option| option == "ro" || option.starts_with("ro="))
        })
}

pub fn parse_kernel_install_layout(install_conf: &str) -> Option<String> {
    install_conf
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('#').not())
        .filter_map(|line| line.strip_prefix("layout="))
        .next_back()
        .map(|layout| layout.trim().trim_matches('"').to_owned())
        .filter(|layout| layout.is_empty().not())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arch() -> BootFacts {
        BootFacts {
            os_id: "arch".to_owned(),
            pacman: true,
            mkinitcpio_presets: true,
            mkinitcpio_hook_active: true,
            ..BootFacts::default()
        }
    }

    fn debian() -> BootFacts {
        BootFacts {
            os_id: "debian".to_owned(),
            update_initramfs: true,
            ..BootFacts::default()
        }
    }

    fn fedora() -> BootFacts {
        BootFacts {
            os_id: "fedora".to_owned(),
            dracut: true,
            kernel_install: true,
            kernel_install_layout: Some("bls".to_owned()),
            running_kernel_in_boot: true,
            ..BootFacts::default()
        }
    }

    // Verify declarative and atomic-update systems are refused before any tool is considered,
    // even when a rebuild tool is also present.
    #[test]
    fn declarative_and_steamos_systems_are_refused() {
        let nixos = BootFacts {
            os_id: "nixos".to_owned(),
            ..debian()
        };
        assert_eq!(classify(&nixos), BootSetup::Refused(Refusal::NixOs));
        let guix = BootFacts {
            os_id: "guix".to_owned(),
            ..BootFacts::default()
        };
        assert_eq!(classify(&guix), BootSetup::Refused(Refusal::Guix));
        let steamos = BootFacts {
            steamos: true,
            ..arch()
        };
        assert_eq!(classify(&steamos), BootSetup::Refused(Refusal::SteamOs));
    }

    // Verify ostree systems take a kernel argument through rpm-ostree, and ignore the dracut
    // that atomic images also ship. Plain ostree is left to the user.
    #[test]
    fn ostree_takes_a_kernel_argument() {
        let silverblue = BootFacts {
            ostree_booted: true,
            rpm_ostree: true,
            ..fedora()
        };
        assert_eq!(classify(&silverblue), BootSetup::RpmOstree);
        let bootc = BootFacts {
            ostree_booted: true,
            ..fedora()
        };
        assert_eq!(classify(&bootc), BootSetup::Refused(Refusal::Ostree));
    }

    // Verify a read-only root with transactional-update uses it, while a read-write openSUSE
    // with sdbootutil uses sdbootutil, and one with GRUB falls through to dracut.
    #[test]
    fn opensuse_variants_pick_their_tool() {
        let aeon = BootFacts {
            os_id: "opensuse-aeon".to_owned(),
            root_read_only: true,
            transactional_update: true,
            sdbootutil: true,
            sdbootutil_installed: true,
            ..fedora()
        };
        assert_eq!(
            classify(&aeon),
            BootSetup::Rebuild(RebuildTool::TransactionalUpdate)
        );
        let tumbleweed_sdboot = BootFacts {
            os_id: "opensuse-tumbleweed".to_owned(),
            sdbootutil: true,
            sdbootutil_installed: true,
            ..fedora()
        };
        assert_eq!(
            classify(&tumbleweed_sdboot),
            BootSetup::Rebuild(RebuildTool::Sdbootutil)
        );
        let tumbleweed_grub = BootFacts {
            transactional_update: true,
            ..fedora()
        };
        assert_eq!(
            classify(&tumbleweed_grub),
            BootSetup::Rebuild(RebuildTool::Dracut)
        );
    }

    // Verify the limine hook wins over stale mkinitcpio presets, which is the reported limine
    // failure: `mkinitcpio -P` found no presets.
    #[test]
    fn limine_hooks_win_over_presets() {
        let omarchy = BootFacts {
            os_id: "omarchy".to_owned(),
            limine_mkinitcpio: true,
            ..arch()
        };
        assert_eq!(
            classify(&omarchy),
            BootSetup::Rebuild(RebuildTool::LimineMkinitcpio)
        );
        let limine_dracut = BootFacts {
            limine_dracut: true,
            arch_dracut_hook_active: true,
            dracut: true,
            ..BootFacts {
                pacman: true,
                ..BootFacts::default()
            }
        };
        assert_eq!(
            classify(&limine_dracut),
            BootSetup::Rebuild(RebuildTool::LimineDracut)
        );
    }

    // Verify two generators that both rebuild on kernel updates are refused rather than
    // guessed between, while a wrapper beside mkinitcpio, as CachyOS has, is not a conflict.
    #[test]
    fn competing_generators_are_refused() {
        let both_hooks = BootFacts {
            arch_dracut_hook_active: true,
            dracut: true,
            ..arch()
        };
        assert_eq!(
            classify(&both_hooks),
            BootSetup::Refused(Refusal::CompetingTools(
                RebuildTool::Mkinitcpio,
                RebuildTool::ArchDracutHook
            ))
        );
        let both_limine = BootFacts {
            limine_mkinitcpio: true,
            limine_dracut: true,
            ..arch()
        };
        assert_eq!(
            classify(&both_limine),
            BootSetup::Refused(Refusal::CompetingTools(
                RebuildTool::LimineMkinitcpio,
                RebuildTool::LimineDracut
            ))
        );
        let cachyos = BootFacts {
            limine_mkinitcpio: true,
            arch_dracut_hook_active: true,
            ..arch()
        };
        assert_eq!(
            classify(&cachyos),
            BootSetup::Rebuild(RebuildTool::LimineMkinitcpio)
        );
    }

    // Verify the Arch dracut flavours: EndeavourOS systemd-boot, the EOS/Garuda/CachyOS
    // dracut-rebuild helpers, and Arch's own dracut hook.
    #[test]
    fn arch_dracut_flavours_pick_their_tool() {
        let base = BootFacts {
            pacman: true,
            dracut: true,
            ..BootFacts::default()
        };
        let eos_sdboot = BootFacts {
            reinstall_kernels: true,
            dracut_rebuild: true,
            ..base.clone()
        };
        assert_eq!(
            classify(&eos_sdboot),
            BootSetup::Rebuild(RebuildTool::ReinstallKernels)
        );
        let garuda = BootFacts {
            dracut_rebuild: true,
            ..base.clone()
        };
        assert_eq!(
            classify(&garuda),
            BootSetup::Rebuild(RebuildTool::DracutRebuild)
        );
        let arch_dracut = BootFacts {
            arch_dracut_hook_active: true,
            ..base
        };
        assert_eq!(
            classify(&arch_dracut),
            BootSetup::Rebuild(RebuildTool::ArchDracutHook)
        );
    }

    // Verify plain mkinitcpio needs both presets and an unmasked hook, and that an Arch box
    // with neither never falls back to dracut's default output, which no bootloader loads.
    #[test]
    fn mkinitcpio_needs_presets_and_its_hook() {
        assert_eq!(
            classify(&arch()),
            BootSetup::Rebuild(RebuildTool::Mkinitcpio)
        );
        let masked = BootFacts {
            mkinitcpio_hook_active: false,
            ..arch()
        };
        assert_eq!(classify(&masked), BootSetup::Unrecognized);
        let no_presets = BootFacts {
            mkinitcpio_presets: false,
            dracut: true,
            ..arch()
        };
        assert_eq!(classify(&no_presets), BootSetup::Unrecognized);
    }

    // Verify CachyOS's own update-initramfs dispatcher does not outrank the pacman hooks, and
    // only serves when none of them matched.
    #[test]
    fn cachyos_update_initramfs_is_a_last_resort() {
        let cachyos = BootFacts {
            os_id: "cachyos".to_owned(),
            update_initramfs: true,
            ..arch()
        };
        assert_eq!(
            classify(&cachyos),
            BootSetup::Rebuild(RebuildTool::Mkinitcpio)
        );
        let no_presets = BootFacts {
            mkinitcpio_presets: false,
            ..cachyos
        };
        assert_eq!(
            classify(&no_presets),
            BootSetup::Rebuild(RebuildTool::UpdateInitramfs)
        );
    }

    // Verify Arch kernel-install needs its pacman script and an explicit layout, since
    // auto-detection reports bls on limine machines too.
    #[test]
    fn arch_kernel_install_needs_an_explicit_layout() {
        let configured = BootFacts {
            kernel_install: true,
            kernel_install_pacman_script: true,
            kernel_install_layout: Some("uki".to_owned()),
            ..arch()
        };
        assert_eq!(
            classify(&configured),
            BootSetup::Rebuild(RebuildTool::KernelInstall)
        );
        let unset = BootFacts {
            kernel_install_layout: None,
            ..configured
        };
        assert_eq!(
            classify(&unset),
            BootSetup::Rebuild(RebuildTool::Mkinitcpio)
        );
    }

    // Verify Debian uses update-initramfs whichever backend owns it, and Debian 13 dracut,
    // which lacks the wrapper, rebuilds per kernel.
    #[test]
    fn debian_variants_pick_their_tool() {
        assert_eq!(
            classify(&debian()),
            BootSetup::Rebuild(RebuildTool::UpdateInitramfs)
        );
        let trixie_dracut = BootFacts {
            update_initramfs: false,
            debian_dracut_postinst: true,
            dracut: true,
            ..debian()
        };
        assert_eq!(
            classify(&trixie_dracut),
            BootSetup::Rebuild(RebuildTool::DebianDracut)
        );
    }

    // Verify Fedora uses dracut on BLS and kernel-install only for a UKI layout.
    #[test]
    fn fedora_uses_kernel_install_only_for_ukis() {
        assert_eq!(classify(&fedora()), BootSetup::Rebuild(RebuildTool::Dracut));
        let uki = BootFacts {
            kernel_install_layout: Some("uki".to_owned()),
            ..fedora()
        };
        assert_eq!(
            classify(&uki),
            BootSetup::Rebuild(RebuildTool::KernelInstall)
        );
    }

    // Verify Arch helpers (dracut-rebuild, limine-mkinitcpio) are ignored without pacman, and
    // that a system with no known tool is reported as unrecognized rather than guessed.
    #[test]
    fn unknown_setups_are_not_guessed() {
        let stray_helper = BootFacts {
            dracut_rebuild: true,
            limine_mkinitcpio: true,
            ..BootFacts::default()
        };
        assert_eq!(classify(&stray_helper), BootSetup::Unrecognized);
        let alpine = BootFacts {
            os_id: "alpine".to_owned(),
            ..BootFacts::default()
        };
        assert_eq!(classify(&alpine), BootSetup::Unrecognized);
    }

    // Verify every command calls its tool by absolute path, so no PATH wrapper intercepts it.
    // ArchDracutHook and DebianDracut start with a shell loop, so their tool is checked inside.
    #[test]
    fn commands_use_absolute_paths() {
        let tools = [
            RebuildTool::TransactionalUpdate,
            RebuildTool::LimineMkinitcpio,
            RebuildTool::LimineDracut,
            RebuildTool::ReinstallKernels,
            RebuildTool::KernelInstall,
            RebuildTool::DracutRebuild,
            RebuildTool::Mkinitcpio,
            RebuildTool::UpdateInitramfs,
            RebuildTool::Sdbootutil,
            RebuildTool::Dracut,
        ];
        for tool in tools {
            assert!(tool.command().starts_with('/'), "{tool:?}");
        }
        assert!(RebuildTool::ArchDracutHook
            .command()
            .contains("| /usr/share/libalpm/scripts/dracut-install"));
        assert!(RebuildTool::DebianDracut
            .command()
            .contains("/usr/bin/dracut --force"));
        assert!(RebuildTool::UpdateInitramfs.command().contains("-q").not());
    }

    // Verify the limine and update-initramfs lines that hide a failure behind exit 0 are
    // found, and that ordinary output is not mistaken for one.
    #[test]
    fn failure_lines_behind_exit_zero_are_found() {
        let limine = "==> Building\nERROR: mkinitcpio failed for kernel 6.17.1, skipping.\n";
        assert_eq!(
            RebuildTool::LimineMkinitcpio.failure_line(limine),
            Some("ERROR: mkinitcpio failed for kernel 6.17.1, skipping.")
        );
        assert_eq!(
            RebuildTool::UpdateInitramfs.failure_line("W: /boot is ro mounted.\n"),
            Some("W: /boot is ro mounted.")
        );
        assert_eq!(
            RebuildTool::LimineMkinitcpio.failure_line("==> done\n"),
            None
        );
        assert_eq!(RebuildTool::Mkinitcpio.failure_line(limine), None);
    }

    // Verify the ID is read unquoted and lowercased, and ID_LIKE is not mistaken for it.
    #[test]
    fn os_release_id_is_parsed() {
        let os_release = "NAME=\"Artix\"\nID_LIKE=arch\nID=\"Artix\"\n";
        assert_eq!(parse_os_release_id(os_release), "artix");
        assert_eq!(parse_os_release_id(""), "");
    }

    // Verify only an `ro` option on the `/` line counts, not on other mounts or comments. The
    // MicroOS line is copied from a real install, which spells it `ro=vfs`.
    #[test]
    fn fstab_read_only_root_is_parsed() {
        let microos = "UUID=1 / btrfs ro=vfs 0 0\nUUID=1 /var btrfs defaults,subvol=@/var 0 0\n";
        assert!(parse_fstab_root_read_only(microos));
        assert!(parse_fstab_root_read_only(
            "UUID=1 / btrfs ro,subvol=@ 0 0\n"
        ));
        assert!(parse_fstab_root_read_only("UUID=1 / btrfs rootro=vfs 0 0\n").not());
        let normal = "# UUID=1 / btrfs ro 0 0\nUUID=1 / ext4 defaults,errors=remount-ro 0 1\n";
        assert!(parse_fstab_root_read_only(normal).not());
    }

    // Verify the last uncommented layout wins and quotes are stripped.
    #[test]
    fn kernel_install_layout_is_parsed() {
        assert_eq!(
            parse_kernel_install_layout("#layout=bls\nlayout=\"uki\"\n"),
            Some("uki".to_owned())
        );
        assert_eq!(
            parse_kernel_install_layout("initrd_generator=dracut\n"),
            None
        );
    }

    // Verify facts are read from a real tree: an Omarchy-like limine layout with an override
    // hook and no presets, laid out under a temporary root.
    #[test]
    fn facts_are_gathered_from_a_limine_tree() {
        let root = tempfile::tempdir().unwrap();
        let write = |path: &str, content: &str| {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        };
        write("etc/os-release", "ID=omarchy\nID_LIKE=arch\n");
        write("usr/bin/pacman", "");
        write("usr/bin/limine-mkinitcpio", "");
        write("usr/share/libalpm/hooks/90-mkinitcpio-install.hook", "");
        write("etc/pacman.d/hooks/90-mkinitcpio-install.hook", "");
        std::fs::create_dir_all(root.path().join("etc/mkinitcpio.d")).unwrap();

        let facts = gather_boot_facts_blocking(root.path());
        assert_eq!(facts.os_id, "omarchy");
        assert!(facts.limine_mkinitcpio);
        assert!(facts.mkinitcpio_presets.not());
        assert!(facts.mkinitcpio_hook_active.not());
        assert_eq!(
            classify(&facts),
            BootSetup::Rebuild(RebuildTool::LimineMkinitcpio)
        );
    }

    // Verify a directory counts only with a file of that extension, and a missing one does not.
    #[test]
    fn a_dir_has_an_extension_only_with_such_a_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(dir_has_extension(&dir.path().join("missing"), "preset").not());
        std::fs::write(dir.path().join("linux.conf"), "").unwrap();
        assert!(dir_has_extension(dir.path(), "preset").not());
        std::fs::write(dir.path().join("linux.preset"), "").unwrap();
        assert!(dir_has_extension(dir.path(), "preset"));
    }

    // Verify a Debian tree with the running kernel in /boot resolves to update-initramfs.
    #[test]
    fn facts_are_gathered_from_a_debian_tree() {
        let root = tempfile::tempdir().unwrap();
        let write = |path: &str, content: &str| {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        };
        write("etc/os-release", "ID=debian\n");
        write("usr/sbin/update-initramfs", "");
        write("proc/sys/kernel/osrelease", "6.12.48-amd64\n");
        write("boot/vmlinuz-6.12.48-amd64", "");

        let facts = gather_boot_facts_blocking(root.path());
        assert!(facts.update_initramfs);
        assert!(facts.running_kernel_in_boot);
        assert!(facts.pacman.not());
        assert_eq!(
            classify(&facts),
            BootSetup::Rebuild(RebuildTool::UpdateInitramfs)
        );
    }
}
