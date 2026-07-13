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

//! Restores a backup over the live configuration.
//!
//! The daemon holds config in memory and never re-reads it, so it must be
//! stopped first or it will clobber the restored files. Restore therefore prints
//! a loud warning and requires a typed confirmation (bypassable for automation),
//! validates the backup before touching anything, and copies each file into place
//! atomically. There is no auto-snapshot: the user is told to back up first if
//! they want a safety net.

use std::ffi::OsStr;
use std::fs::Permissions;
use std::io::{IsTerminal, Write};
use std::ops::Not;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use log::{error, info, warn};

use super::archive;
use super::manifest::{Manifest, MANIFEST_FILE_NAME};
use crate::cc_fs;
use crate::paths;

pub struct RestoreOptions {
    /// Backup to restore; the most recent backup is used when `None`.
    pub source: Option<PathBuf>,
    /// Skip the interactive confirmation (for scripts).
    pub yes: bool,
    /// Restore even if the backup fails validation.
    pub force: bool,
}

enum Source {
    Dir(PathBuf),
    Archive(PathBuf),
}

/// Restores a backup over the live config after warning, validating, and
/// confirming. The daemon must be stopped; restore does not check for it.
pub async fn run_restore(opts: &RestoreOptions) -> Result<()> {
    let source = resolve_source(opts.source.as_deref())?;
    // Keep the temp dir alive until the files have been copied out of it.
    let mut _extracted: Option<tempfile::TempDir> = None;
    let backup_dir = match source {
        Source::Dir(dir) => dir,
        Source::Archive(archive_path) => {
            let tmp = tempfile::tempdir().context("Creating temp dir for archive")?;
            let root = archive::extract(&archive_path, tmp.path())?;
            _extracted = Some(tmp);
            root
        }
    };

    warn_on_version_mismatch(&backup_dir).await;
    verify_backup(&backup_dir, opts.force).await?;

    if confirm(&backup_dir, opts.yes)?.not() {
        info!("Restore cancelled");
        return Ok(());
    }

    let restored = apply_backup(&backup_dir, paths::config_dir()).await?;
    info!(
        "Restored {restored} configuration file(s) from {}",
        backup_dir.display()
    );
    info!("Start or restart the CoolerControl daemon to apply the restored configuration");
    Ok(())
}

/// Resolves the restore source: the most recent backup when unspecified, else a
/// directory or `.tar.gz` file, auto-detected.
fn resolve_source(source: Option<&Path>) -> Result<Source> {
    match source {
        None => {
            let latest = super::valid_backup_dirs(paths::backups_dir())
                .into_iter()
                .next()
                .ok_or_else(|| anyhow!("No backups found in {}", paths::backups_dir().display()))?;
            info!("Restoring the most recent backup: {}", latest.display());
            Ok(Source::Dir(latest))
        }
        Some(path) if path.is_dir() => Ok(Source::Dir(path.to_path_buf())),
        Some(path) if path.is_file() => Ok(Source::Archive(path.to_path_buf())),
        Some(path) => bail!("Backup source not found: {}", path.display()),
    }
}

/// Logs a warning when the backup was made by a different daemon version, or when
/// its manifest is missing or unreadable. Non-fatal: restore proceeds.
async fn warn_on_version_mismatch(backup_dir: &Path) {
    let Ok(contents) = cc_fs::read_txt(backup_dir.join(MANIFEST_FILE_NAME)).await else {
        warn!("Backup has no manifest; it may not be a CoolerControl backup");
        return;
    };
    match Manifest::from_toml(&contents) {
        Ok(manifest) if manifest.daemon_version != crate::VERSION => warn!(
            "Backup was made by daemon {} but this is {}; restoring anyway",
            manifest.daemon_version,
            crate::VERSION
        ),
        Ok(_) => {}
        Err(err) => warn!("Backup manifest is unreadable: {err}"),
    }
}

/// Validates the backup's config files. Refuses (unless `force`) when any is
/// invalid, so a corrupt backup cannot be restored into a startup failure.
async fn verify_backup(backup_dir: &Path, force: bool) -> Result<()> {
    let reports = crate::config_validate::check_dir(backup_dir).await;
    let mut any_invalid = false;
    for report in &reports {
        if let Some(msg) = &report.error {
            any_invalid = true;
            if force {
                warn!("Backup file {} is invalid: {msg}", report.name);
            } else {
                error!("Backup file {} is invalid: {msg}", report.name);
            }
        }
    }
    if any_invalid.not() {
        return Ok(());
    }
    if force {
        warn!("Restoring despite invalid config (--force)");
        Ok(())
    } else {
        bail!("Backup contains invalid config; fix it or pass --force to restore anyway")
    }
}

/// Prompts for a typed `yes` unless `yes` was passed. Refuses rather than hangs
/// when stdin is not a terminal and no bypass was given.
fn confirm(backup_dir: &Path, yes: bool) -> Result<bool> {
    if yes {
        return Ok(true);
    }
    if std::io::stdin().is_terminal().not() {
        bail!("Refusing to restore without confirmation (stdin is not a terminal); pass --yes");
    }
    println!("This will OVERWRITE your current CoolerControl configuration with the backup at");
    println!("  {}", backup_dir.display());
    println!("Stop the daemon first, or it will overwrite the restored files on its next save.");
    println!("This cannot be undone. Make a backup first if you want a safety net.");
    print!("Type 'yes' to continue: ");
    std::io::stdout()
        .flush()
        .context("Flushing confirmation prompt")?;
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .context("Reading confirmation")?;
    Ok(line.trim() == "yes")
}

/// Copies the backup's config files over the live config, each written to a temp
/// file and renamed into place. The set of files comes from the manifest (see
/// `restorable_files`), never from a raw directory listing, so an untrusted backup
/// cannot smuggle extra files. Credential files are made owner-only. Returns the
/// number of files restored.
async fn apply_backup(backup_dir: &Path, config_dir: &Path) -> Result<usize> {
    let names = restorable_files(backup_dir).await;
    cc_fs::create_dir_all(config_dir)
        .await
        .with_context(|| format!("Creating config directory {}", config_dir.display()))?;
    let mut restored = 0;
    for name in &names {
        let src = backup_dir.join(name);
        if src.is_file().not() {
            continue;
        }
        let bytes = cc_fs::read_image(&src)
            .await
            .with_context(|| format!("Reading backup file {name}"))?;
        let dst = config_dir.join(name);
        let tmp = config_dir.join(format!(".{name}.restore-tmp"));
        cc_fs::write(&tmp, bytes)
            .await
            .with_context(|| format!("Writing {}", dst.display()))?;
        if is_secret(name) {
            // Credential files must not be world-readable once in the config dir.
            cc_fs::set_permissions(&tmp, Permissions::from_mode(0o600))
                .await
                .with_context(|| format!("Securing {}", dst.display()))?;
        }
        cc_fs::rename(&tmp, &dst)
            .await
            .with_context(|| format!("Publishing {}", dst.display()))?;
        restored += 1;
    }
    Ok(restored)
}

/// The config file names a restore will write, taken from the backup manifest so
/// only files the backup declares are applied: an untrusted archive cannot smuggle
/// arbitrary files, and a manifest that declares no secrets cannot carry
/// credentials. Falls back to the recognized config names for a manifest-less
/// directory. Every name is constrained to a single path component, so a crafted
/// manifest entry can never escape the config directory.
async fn restorable_files(backup_dir: &Path) -> Vec<String> {
    let Some(manifest) = read_manifest(backup_dir).await else {
        warn!("Backup has no readable manifest; restoring only recognized config files");
        return known_config_names();
    };
    let includes_secrets = manifest.includes_secrets;
    manifest
        .files
        .into_iter()
        .filter(|name| is_plain_file_name(name))
        .filter(|name| name != MANIFEST_FILE_NAME)
        .filter(|name| includes_secrets || is_secret(name).not())
        .collect()
}

async fn read_manifest(backup_dir: &Path) -> Option<Manifest> {
    let contents = cc_fs::read_txt(backup_dir.join(MANIFEST_FILE_NAME))
        .await
        .ok()?;
    Manifest::from_toml(&contents).ok()
}

/// True when `name` is a single path component (no directory parts, no `..`), so
/// `config_dir.join(name)` cannot escape the config directory.
fn is_plain_file_name(name: &str) -> bool {
    Path::new(name).file_name() == Some(OsStr::new(name))
}

/// The files a restore recognizes when a backup has no manifest to enumerate them:
/// the known daemon config files plus the two credential files.
fn known_config_names() -> Vec<String> {
    let mut names: Vec<String> = crate::config_validate::CONFIG_FILE_NAMES
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    names.push(".passwd".to_string());
    names.push(".tokens".to_string());
    names
}

fn is_secret(name: &str) -> bool {
    name == ".passwd" || name == ".tokens"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, contents: &str) {
        std::fs::write(dir.join(name), contents).unwrap();
    }

    /// Goal: apply overwrites present config, restores secrets owner-only, and
    /// never copies the manifest. Method: stage a backup and a stale config dir,
    /// apply, and assert contents, the skipped manifest, and secret permissions.
    #[test]
    fn apply_backup_overwrites_and_secures_secrets() {
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let backup = tmp.path().join("backup");
            let config = tmp.path().join("config");
            std::fs::create_dir_all(&backup).unwrap();
            std::fs::create_dir_all(&config).unwrap();
            write(&backup, "config.toml", "new = true\n");
            write(&backup, ".passwd", "hash");
            let manifest = Manifest::new(
                "2026-07-12T10:00:00+00:00".to_string(),
                vec!["config.toml".to_string(), ".passwd".to_string()],
                true,
            );
            write(&backup, MANIFEST_FILE_NAME, &manifest.to_toml().unwrap());
            write(&config, "config.toml", "old = true\n");

            let restored = apply_backup(&backup, &config).await.unwrap();

            assert_eq!(restored, 2); // config.toml + .passwd, not the manifest
            assert_eq!(
                std::fs::read_to_string(config.join("config.toml")).unwrap(),
                "new = true\n"
            );
            assert!(!config.join(MANIFEST_FILE_NAME).exists());
            let mode = std::fs::metadata(config.join(".passwd"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        });
    }

    /// Goal: an invalid backup is refused without `--force` and allowed with it.
    /// Method: stage a backup with malformed alerts.json and check both paths.
    #[test]
    fn verify_backup_gates_on_force() {
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let backup = tmp.path().join("backup");
            std::fs::create_dir_all(&backup).unwrap();
            write(&backup, "alerts.json", "{ not valid");

            assert!(verify_backup(&backup, false).await.is_err());
            assert!(verify_backup(&backup, true).await.is_ok());
        });
    }

    /// Goal: source auto-detection maps a directory to Dir, a file to Archive, and
    /// a missing path to an error. Method: exercise each with a real temp path.
    #[test]
    fn resolve_source_detects_dir_file_and_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("d");
        std::fs::create_dir_all(&dir).unwrap();
        let file = tmp.path().join("a.tar.gz");
        std::fs::write(&file, b"x").unwrap();

        assert!(matches!(
            resolve_source(Some(&dir)).unwrap(),
            Source::Dir(_)
        ));
        assert!(matches!(
            resolve_source(Some(&file)).unwrap(),
            Source::Archive(_)
        ));
        assert!(resolve_source(Some(&tmp.path().join("missing"))).is_err());
    }

    /// Goal: `--yes` bypasses the prompt; a non-terminal stdin without it refuses
    /// rather than hanging. Method: assert the bypass, and the non-TTY refusal only
    /// when the test's stdin is actually not a terminal.
    #[test]
    fn confirm_yes_bypasses_and_non_tty_refuses() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(confirm(tmp.path(), true).unwrap());
        if std::io::stdin().is_terminal().not() {
            assert!(confirm(tmp.path(), false).is_err());
        }
    }

    /// Goal: a backup archived then extracted restores identically (spans archive
    /// write, extract, and apply). Method: build a backup dir, archive it, extract
    /// it, apply, and assert the file lands.
    #[test]
    fn restore_from_extracted_archive_round_trips() {
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let backup = tmp.path().join("2026-07-12T10-00-00");
            std::fs::create_dir_all(&backup).unwrap();
            write(&backup, "config.toml", "from = \"archive\"\n");
            write(&backup, MANIFEST_FILE_NAME, "format_version = 1\n");

            let out = tmp.path().join("out");
            std::fs::create_dir_all(&out).unwrap();
            let archive_path =
                archive::write(&backup, "2026-07-12T10-00-00", Some(&out), false).unwrap();

            let extract_dir = tmp.path().join("x");
            let root = archive::extract(&archive_path, &extract_dir).unwrap();
            let config = tmp.path().join("config");
            let restored = apply_backup(&root, &config).await.unwrap();

            assert_eq!(restored, 1);
            assert_eq!(
                std::fs::read_to_string(config.join("config.toml")).unwrap(),
                "from = \"archive\"\n"
            );
        });
    }

    /// Goal: restore applies only the files the manifest declares, so an untrusted
    /// backup cannot smuggle an unlisted file, and a "no secrets" manifest cannot
    /// carry credentials even when a `.passwd` sits in the directory. Method: stage
    /// a backup whose manifest lists only config.toml but whose directory also holds
    /// an extra file and a secret, then assert only config.toml is restored.
    #[test]
    fn apply_backup_restores_only_manifested_files() {
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let backup = tmp.path().join("backup");
            let config = tmp.path().join("config");
            std::fs::create_dir_all(&backup).unwrap();
            std::fs::create_dir_all(&config).unwrap();
            let manifest = Manifest::new(
                "2026-07-12T10:00:00+00:00".to_string(),
                vec!["config.toml".to_string()],
                false,
            );
            write(&backup, MANIFEST_FILE_NAME, &manifest.to_toml().unwrap());
            write(&backup, "config.toml", "ok = true\n");
            write(&backup, "evil.sh", "rm -rf /\n");
            write(&backup, ".passwd", "attacker-known");

            let restored = apply_backup(&backup, &config).await.unwrap();

            assert_eq!(restored, 1);
            assert!(config.join("config.toml").exists());
            assert!(
                config.join("evil.sh").exists().not(),
                "an unlisted file must not be restored"
            );
            assert!(
                config.join(".passwd").exists().not(),
                "a secret must not be restored when the manifest excludes it"
            );
        });
    }

    /// Goal: manifest file names that are not a single path component (traversal or
    /// nested paths) are dropped, so `config_dir.join(name)` cannot escape. Method:
    /// build a manifest with a `..` entry and a nested entry and assert only the
    /// plain name survives.
    #[test]
    fn restorable_files_rejects_path_traversal() {
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let backup = tmp.path().join("backup");
            std::fs::create_dir_all(&backup).unwrap();
            let manifest = Manifest::new(
                "2026-07-12T10:00:00+00:00".to_string(),
                vec![
                    "config.toml".to_string(),
                    "../escape.toml".to_string(),
                    "sub/nested.toml".to_string(),
                ],
                false,
            );
            write(&backup, MANIFEST_FILE_NAME, &manifest.to_toml().unwrap());

            let names = restorable_files(&backup).await;

            assert_eq!(names, vec!["config.toml"]);
        });
    }
}
