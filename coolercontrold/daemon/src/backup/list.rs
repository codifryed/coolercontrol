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

//! Lists the available backups, newest first, from each backup's manifest.

use std::path::Path;

use anyhow::Result;

use super::manifest::{Manifest, MANIFEST_FILE_NAME};
use crate::cc_fs;
use crate::paths;

/// Prints a table of the available backups, newest first.
pub async fn run_list() -> Result<()> {
    let dirs = super::valid_backup_dirs(paths::backups_dir());
    if dirs.is_empty() {
        println!("No backups found in {}", paths::backups_dir().display());
        return Ok(());
    }
    println!(
        "{:<21}  {:<9}  {:>5}  {:>9}  secrets",
        "TIMESTAMP", "VERSION", "FILES", "SIZE"
    );
    for dir in dirs {
        let summary = summarize(&dir).await;
        let name = dir
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        println!(
            "{name:<21}  {:<9}  {:>5}  {:>9}  {}",
            summary.version,
            summary.files,
            human_size(summary.size_bytes),
            if summary.includes_secrets {
                "yes"
            } else {
                "no"
            }
        );
    }
    Ok(())
}

struct Summary {
    version: String,
    files: usize,
    includes_secrets: bool,
    size_bytes: u64,
}

/// Reads a backup's manifest for display; falls back to placeholders when the
/// manifest is missing or unreadable.
async fn summarize(dir: &Path) -> Summary {
    let (version, files, includes_secrets) =
        match cc_fs::read_txt(dir.join(MANIFEST_FILE_NAME)).await {
            Ok(contents) => match Manifest::from_toml(&contents) {
                Ok(manifest) => (
                    manifest.daemon_version,
                    manifest.files.len(),
                    manifest.includes_secrets,
                ),
                Err(_) => ("?".to_string(), 0, false),
            },
            Err(_) => ("?".to_string(), 0, false),
        };
    Summary {
        version,
        files,
        includes_secrets,
        size_bytes: dir_size(dir),
    }
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = cc_fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter_map(|entry| entry.metadata().ok())
        .filter(std::fs::Metadata::is_file)
        .map(|meta| meta.len())
        .sum()
}

#[allow(clippy::cast_precision_loss)] // Backup sizes are tiny; float rounding is irrelevant.
fn human_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Goal: `human_size` renders bytes, KiB, and MiB with one decimal. Method:
    /// probe one value in each range.
    #[test]
    fn human_size_scales_units() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2048), "2.0 KiB");
        assert_eq!(human_size(3 * 1024 * 1024), "3.0 MiB");
    }

    /// Goal: a backup summary reflects its manifest (version, file count, secrets)
    /// and a non-zero on-disk size. Method: write a manifest plus a file, then
    /// summarize.
    #[test]
    fn summarize_reads_manifest_and_size() {
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let dir = tmp.path().join("b");
            std::fs::create_dir_all(&dir).unwrap();
            let manifest = Manifest::new(
                "2026-07-12T10:00:00+00:00".to_string(),
                vec!["config.toml".to_string()],
                true,
            );
            std::fs::write(dir.join(MANIFEST_FILE_NAME), manifest.to_toml().unwrap()).unwrap();
            std::fs::write(dir.join("config.toml"), "x = 1\n").unwrap();

            let summary = summarize(&dir).await;

            assert_eq!(summary.version, crate::VERSION);
            assert_eq!(summary.files, 1);
            assert!(summary.includes_secrets);
            assert!(summary.size_bytes > 0);
        });
    }
}
