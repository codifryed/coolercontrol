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

//! Portable `.tar.gz` export of a backup directory.
//!
//! Entries are nested under the backup's timestamp directory, so extracting the
//! archive yields the same `<stamp>/config.toml` layout as an on-disk backup and
//! restore can treat the two identically.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Writes a gzipped tarball of `backup_dir` and returns the archive path. `name`
/// is the backup's directory name, used both to nest the entries and to name the
/// default archive file (so distinct backups never collide on the same output).
///
/// The work is synchronous std file IO on a few small files; the backup CLI is a
/// one-shot command about to exit, so briefly running it inline is acceptable.
pub fn write(backup_dir: &Path, name: &str, output: Option<&Path>) -> Result<PathBuf> {
    let dest = resolve_dest(name, output)?;
    let file = std::fs::File::create(&dest)
        .with_context(|| format!("Creating archive {}", dest.display()))?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    // Nest under the backup name so extraction mirrors an on-disk backup directory.
    builder
        .append_dir_all(name, backup_dir)
        .with_context(|| format!("Archiving backup into {}", dest.display()))?;
    builder
        .into_inner()
        .context("Finalizing archive")?
        .finish()
        .context("Finalizing archive compression")?;
    Ok(dest)
}

/// Resolves where the archive is written: the current directory by default, a
/// generated name inside `output` when it is a directory, or `output` verbatim
/// when it names a file.
fn resolve_dest(name: &str, output: Option<&Path>) -> Result<PathBuf> {
    let default_name = format!("coolercontrol-backup-{name}.tar.gz");
    match output {
        None => Ok(std::env::current_dir()
            .context("Resolving current directory for archive")?
            .join(default_name)),
        Some(path) if path.is_dir() => Ok(path.join(default_name)),
        Some(path) => Ok(path.to_path_buf()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Goal: `resolve_dest` must place the archive at the current dir by default,
    /// inside a given directory with the generated name, and at an explicit file
    /// path verbatim. Method: exercise all three branches and assert the result.
    #[test]
    fn resolve_dest_covers_default_dir_and_file() {
        let tmp = tempfile::tempdir().unwrap();

        let default = resolve_dest("2026-07-12T10-00-00", None).unwrap();
        assert_eq!(
            default.file_name().unwrap(),
            "coolercontrol-backup-2026-07-12T10-00-00.tar.gz"
        );

        let into_dir = resolve_dest("2026-07-12T10-00-00", Some(tmp.path())).unwrap();
        assert_eq!(into_dir.parent().unwrap(), tmp.path());
        assert_eq!(
            into_dir.file_name().unwrap(),
            "coolercontrol-backup-2026-07-12T10-00-00.tar.gz"
        );

        let file = tmp.path().join("my-backup.tar.gz");
        let as_file = resolve_dest("2026-07-12T10-00-00", Some(&file)).unwrap();
        assert_eq!(as_file, file);
    }

    /// Goal: an archive must round-trip, extracting to the same files nested under
    /// the timestamp directory. Method: build a backup dir, archive it, unpack the
    /// tarball, and assert a file's contents survive.
    #[test]
    fn archive_round_trips_through_extraction() {
        let tmp = tempfile::tempdir().unwrap();
        let backup_dir = tmp.path().join("2026-07-12T10-00-00");
        std::fs::create_dir_all(&backup_dir).unwrap();
        std::fs::write(backup_dir.join("config.toml"), "answer = 42\n").unwrap();

        let out_dir = tmp.path().join("out");
        std::fs::create_dir_all(&out_dir).unwrap();
        let archive = write(&backup_dir, "2026-07-12T10-00-00", Some(&out_dir)).unwrap();
        assert!(archive.exists());

        let extract = tmp.path().join("extract");
        let decoder = flate2::read::GzDecoder::new(std::fs::File::open(&archive).unwrap());
        tar::Archive::new(decoder).unpack(&extract).unwrap();

        let restored = extract.join("2026-07-12T10-00-00").join("config.toml");
        assert_eq!(std::fs::read_to_string(restored).unwrap(), "answer = 42\n");
    }
}
