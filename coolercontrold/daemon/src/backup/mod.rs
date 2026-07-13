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

//! Configuration backup, rotation, and (later) restore.
//!
//! A backup is a timestamped directory under `<config_dir>/backups/` holding a
//! byte-for-byte copy of the daemon's on-disk config files plus a
//! `manifest.toml`. Byte copies (not re-serialization) keep the exact bytes and
//! capture every file regardless of what the running command has loaded. Each
//! backup is built in a staging directory and renamed into place, so an
//! interrupted backup never leaves a partial directory in the rotation set.

mod archive;
mod manifest;

use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use log::warn;

use crate::cc_fs;
use crate::paths;
use manifest::{Manifest, MANIFEST_FILE_NAME};

/// Number of timestamped backups retained by default when `--keep` is not given.
pub const DEFAULT_KEEP: u32 = 10;

/// `strftime` pattern for backup directory names. Colon-free so it is a valid,
/// tidy path segment, and fixed-width so a reverse lexical sort is newest-first.
const STAMP_FORMAT: &str = "%Y-%m-%dT%H-%M-%S";

/// Prefix for the in-progress staging directory, hidden and skipped by rotation.
const STAGING_PREFIX: &str = ".staging-";

/// Upper bound on backups sharing the same one-second timestamp.
const MAX_SAME_SECOND_BACKUPS: u32 = 99;

pub struct BackupOptions {
    /// Number of timestamped backups to retain; older ones are pruned.
    pub keep: u32,
    /// Include the `.passwd` / `.tokens` credential files.
    pub include_secrets: bool,
    /// Also emit a portable `.tar.gz` of the backup.
    pub archive: bool,
    /// Destination for the archive; the current directory is used when `None`.
    pub output: Option<PathBuf>,
}

impl Default for BackupOptions {
    fn default() -> Self {
        Self {
            keep: DEFAULT_KEEP,
            include_secrets: false,
            archive: false,
            output: None,
        }
    }
}

/// The artifacts produced by a backup.
pub struct BackupOutput {
    /// The rotated backup directory under `<config_dir>/backups/`.
    pub dir: PathBuf,
    /// The portable archive, when one was requested.
    pub archive: Option<PathBuf>,
}

/// A source file to capture and the name it takes inside the backup.
struct SourceFile {
    path: PathBuf,
    name: String,
}

/// Backs up the live configuration to a new rotated directory under
/// `<config_dir>/backups/`, optionally also emitting a portable archive.
pub async fn run_backup(opts: &BackupOptions) -> Result<BackupOutput> {
    let sources = default_sources(opts.include_secrets);
    create_backup(paths::backups_dir(), &sources, opts).await
}

/// Core backup logic, parameterized on the backups directory and source list so
/// it is testable without touching the global config paths.
async fn create_backup(
    backups_dir: &Path,
    sources: &[SourceFile],
    opts: &BackupOptions,
) -> Result<BackupOutput> {
    assert!(opts.keep >= 1, "a backup rotation must retain at least one");
    prepare_backups_dir(backups_dir).await?;

    let now = chrono::Local::now();
    let stamp = now.format(STAMP_FORMAT).to_string();
    let staging = backups_dir.join(format!("{STAGING_PREFIX}{stamp}"));
    reset_staging_dir(&staging).await?;

    let included = copy_sources(sources, &staging).await?;
    let manifest = Manifest::new(now.to_rfc3339(), included, opts.include_secrets);
    cc_fs::write_string(staging.join(MANIFEST_FILE_NAME), manifest.to_toml()?)
        .await
        .context("Writing backup manifest")?;

    let final_dir = unique_dir(backups_dir, &stamp);
    cc_fs::rename(&staging, &final_dir)
        .await
        .with_context(|| format!("Publishing backup to {}", final_dir.display()))?;
    prune_backups(backups_dir, opts.keep).await;

    let archive = maybe_archive(&final_dir, opts)?;
    Ok(BackupOutput {
        dir: final_dir,
        archive,
    })
}

/// Writes the portable archive when requested, using the backup's unique
/// directory name so distinct backups never collide on the same destination.
fn maybe_archive(final_dir: &Path, opts: &BackupOptions) -> Result<Option<PathBuf>> {
    if !opts.archive {
        return Ok(None);
    }
    let name = final_dir
        .file_name()
        .expect("backup dir has a name")
        .to_string_lossy();
    let path = archive::write(final_dir, name.as_ref(), opts.output.as_deref())?;
    Ok(Some(path))
}

/// The config files a backup captures, in a stable order. Credential files are
/// appended only when opted in.
fn default_sources(include_secrets: bool) -> Vec<SourceFile> {
    let mut sources: Vec<SourceFile> = [
        paths::config_file(),
        paths::ui_config_file(),
        paths::alert_config_file(),
        paths::mode_config_file(),
        paths::calibration_config_file(),
        paths::overrides_file(),
        paths::detect_override_file(),
    ]
    .into_iter()
    .map(source)
    .collect();
    if include_secrets {
        sources.push(source(paths::passwd_file()));
        sources.push(source(paths::tokens_file()));
    }
    sources
}

fn source(path: &Path) -> SourceFile {
    // Every config path is a constant ending in a file name.
    let name = path
        .file_name()
        .expect("config path has a file name")
        .to_string_lossy()
        .into_owned();
    SourceFile {
        path: path.to_path_buf(),
        name,
    }
}

async fn prepare_backups_dir(backups_dir: &Path) -> Result<()> {
    cc_fs::create_dir_all(backups_dir)
        .await
        .with_context(|| format!("Creating backups directory {}", backups_dir.display()))?;
    // Root-only: backups hold config and, when opted in, credential files.
    cc_fs::set_permissions(backups_dir, Permissions::from_mode(0o700))
        .await
        .with_context(|| format!("Securing backups directory {}", backups_dir.display()))
}

async fn reset_staging_dir(staging: &Path) -> Result<()> {
    // Clear a staging directory left behind by a crashed run before reusing it.
    if cc_fs::exists(staging) {
        cc_fs::remove_dir_all(staging)
            .await
            .with_context(|| format!("Clearing stale staging dir {}", staging.display()))?;
    }
    cc_fs::create_dir_all(staging)
        .await
        .with_context(|| format!("Creating staging dir {}", staging.display()))
}

/// Copies each existing source into `staging`, returning the names captured.
/// Missing files are skipped so a fresh install with no alerts/modes still backs up.
async fn copy_sources(sources: &[SourceFile], staging: &Path) -> Result<Vec<String>> {
    let mut included = Vec::with_capacity(sources.len());
    for src in sources {
        if !cc_fs::exists(&src.path) {
            continue;
        }
        let bytes = cc_fs::read_image(&src.path)
            .await
            .with_context(|| format!("Reading {} for backup", src.path.display()))?;
        let dst = staging.join(&src.name);
        cc_fs::write(&dst, bytes)
            .await
            .with_context(|| format!("Writing backup file {}", dst.display()))?;
        included.push(src.name.clone());
    }
    Ok(included)
}

/// Picks a non-existing backup directory name, disambiguating collisions within
/// the same second with a bounded numeric suffix.
fn unique_dir(backups_dir: &Path, stamp: &str) -> PathBuf {
    let first = backups_dir.join(stamp);
    if !first.exists() {
        return first;
    }
    for n in 1..=MAX_SAME_SECOND_BACKUPS {
        let candidate = backups_dir.join(format!("{stamp}-{n}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    backups_dir.join(format!("{stamp}-{MAX_SAME_SECOND_BACKUPS}"))
}

/// Removes the oldest backups beyond `keep`. Pruning failures are logged, not
/// fatal: a fresh backup already succeeded by this point.
async fn prune_backups(backups_dir: &Path, keep: u32) {
    let mut dirs = valid_backup_dirs(backups_dir);
    let keep = keep as usize;
    if dirs.len() <= keep {
        return;
    }
    for path in dirs.split_off(keep) {
        if let Err(err) = cc_fs::remove_dir_all(&path).await {
            warn!("Could not prune old backup {}: {err}", path.display());
        }
    }
}

/// Backup directories (those containing a manifest) sorted newest-first.
fn valid_backup_dirs(backups_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = cc_fs::read_dir(backups_dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join(MANIFEST_FILE_NAME).exists())
        .collect();
    // Directory names are fixed-width timestamps, so reverse name order is newest-first.
    dirs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(dir: &Path, name: &str, contents: &str) {
        std::fs::write(dir.join(name), contents).unwrap();
    }

    fn source_file(dir: &Path, name: &str) -> SourceFile {
        SourceFile {
            path: dir.join(name),
            name: name.to_string(),
        }
    }

    fn make_backup_dir(backups_dir: &Path, name: &str) {
        let dir = backups_dir.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(MANIFEST_FILE_NAME), "format_version = 1\n").unwrap();
    }

    /// Goal: a backup must copy every present source verbatim, skip missing ones,
    /// write a manifest listing exactly what was captured, and leave no staging
    /// directory behind. Method: stage a source dir with two of three files, run
    /// `create_backup`, then assert the published directory's contents and manifest.
    #[test]
    fn create_backup_captures_present_files_and_skips_missing() {
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let src = tmp.path().join("src");
            std::fs::create_dir_all(&src).unwrap();
            write_file(&src, "config.toml", "answer = 42\n");
            write_file(&src, "alerts.json", "{\"a\":1}");
            // modes.json intentionally absent to exercise the skip path.
            let sources = [
                source_file(&src, "config.toml"),
                source_file(&src, "alerts.json"),
                source_file(&src, "modes.json"),
            ];
            let backups = tmp.path().join("backups");

            let out = create_backup(
                &backups,
                &sources,
                &BackupOptions {
                    keep: 10,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            let dir = out.dir;
            assert!(out.archive.is_none());

            assert!(dir.exists());
            assert_eq!(
                std::fs::read_to_string(dir.join("config.toml")).unwrap(),
                "answer = 42\n"
            );
            assert!(dir.join("alerts.json").exists());
            assert!(!dir.join("modes.json").exists());

            let manifest = Manifest::from_toml(
                &std::fs::read_to_string(dir.join(MANIFEST_FILE_NAME)).unwrap(),
            )
            .unwrap();
            assert_eq!(manifest.files, vec!["config.toml", "alerts.json"]);
            assert!(!manifest.includes_secrets);
            assert_eq!(manifest.daemon_version, crate::VERSION);

            // No staging directory should remain after a clean publish.
            let leftover_staging = std::fs::read_dir(&backups)
                .unwrap()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with(STAGING_PREFIX));
            assert!(!leftover_staging);
        });
    }

    /// Goal: rotation must retain exactly the newest `keep` backups and delete the
    /// oldest. Method: create twelve timestamp-named backup dirs, prune to ten,
    /// and assert the two oldest are gone and the newest survive.
    #[test]
    fn rotation_keeps_newest_n() {
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let backups = tmp.path().join("backups");
            std::fs::create_dir_all(&backups).unwrap();
            for i in 0..12 {
                make_backup_dir(&backups, &format!("2026-07-12T10-00-{i:02}"));
            }

            prune_backups(&backups, 10).await;

            let remaining: std::collections::BTreeSet<String> = std::fs::read_dir(&backups)
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            assert_eq!(remaining.len(), 10);
            assert!(!remaining.contains("2026-07-12T10-00-00"));
            assert!(!remaining.contains("2026-07-12T10-00-01"));
            assert!(remaining.contains("2026-07-12T10-00-02"));
            assert!(remaining.contains("2026-07-12T10-00-11"));
        });
    }

    /// Goal: only real backups (directories with a manifest) are counted for
    /// rotation, never stray files or half-written directories. Method: populate a
    /// backups dir with one valid backup, a manifest-less directory, and a loose
    /// file, then assert only the valid one is returned.
    #[test]
    fn valid_backup_dirs_ignores_non_backups() {
        let tmp = tempfile::tempdir().unwrap();
        let backups = tmp.path().join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        make_backup_dir(&backups, "2026-07-12T10-00-00");
        std::fs::create_dir_all(backups.join("not-a-backup")).unwrap();
        std::fs::write(backups.join("loose.txt"), "x").unwrap();

        let dirs = valid_backup_dirs(&backups);

        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].file_name().unwrap(), "2026-07-12T10-00-00");
    }

    /// Goal: credential files are captured only when secrets are opted in. Method:
    /// build the default source list both ways and assert `.passwd`/`.tokens`
    /// appear only with the flag.
    #[test]
    fn default_sources_adds_secrets_only_when_opted_in() {
        let without: Vec<String> = default_sources(false)
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert_eq!(without.len(), 7);
        assert!(!without.iter().any(|n| n == ".passwd"));
        assert!(!without.iter().any(|n| n == ".tokens"));

        let with: Vec<String> = default_sources(true)
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert_eq!(with.len(), 9);
        assert!(with.iter().any(|n| n == ".passwd"));
        assert!(with.iter().any(|n| n == ".tokens"));
    }

    /// Goal: an opted-in backup captures secrets, records the flag, and its
    /// archive extracts to the same files (secrets included) nested under the
    /// backup name. Method: run `create_backup` with secrets + archive, inspect
    /// the directory and manifest, then unpack the archive.
    #[test]
    fn create_backup_with_secrets_and_archive() {
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let src = tmp.path().join("src");
            std::fs::create_dir_all(&src).unwrap();
            write_file(&src, "config.toml", "answer = 42\n");
            write_file(&src, ".passwd", "hash");
            let sources = [
                source_file(&src, "config.toml"),
                source_file(&src, ".passwd"),
            ];
            let backups = tmp.path().join("backups");
            let out_dir = tmp.path().join("out");
            std::fs::create_dir_all(&out_dir).unwrap();

            let out = create_backup(
                &backups,
                &sources,
                &BackupOptions {
                    keep: 10,
                    include_secrets: true,
                    archive: true,
                    output: Some(out_dir.clone()),
                },
            )
            .await
            .unwrap();

            assert!(out.dir.join(".passwd").exists());
            let manifest = Manifest::from_toml(
                &std::fs::read_to_string(out.dir.join(MANIFEST_FILE_NAME)).unwrap(),
            )
            .unwrap();
            assert!(manifest.includes_secrets);

            let archive = out.archive.expect("archive was requested");
            assert_eq!(archive.parent().unwrap(), out_dir);
            let name = out.dir.file_name().unwrap().to_string_lossy().into_owned();
            let extract = tmp.path().join("extract");
            let decoder = flate2::read::GzDecoder::new(std::fs::File::open(&archive).unwrap());
            tar::Archive::new(decoder).unpack(&extract).unwrap();
            assert!(extract.join(&name).join(".passwd").exists());
            assert_eq!(
                std::fs::read_to_string(extract.join(&name).join("config.toml")).unwrap(),
                "answer = 42\n"
            );
        });
    }
}
