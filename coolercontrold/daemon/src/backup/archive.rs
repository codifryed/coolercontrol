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

use std::fs::Permissions;
use std::io::Read;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::manifest::MANIFEST_FILE_NAME;

/// Cap on total decompressed bytes read while extracting an archive. A config
/// backup is tiny (KBs); this exists only to stop a malicious archive (a
/// decompression bomb) from filling the temp filesystem during restore.
const MAX_EXTRACT_BYTES: u64 = 128 * 1024 * 1024;

/// Writes a gzipped tarball of `backup_dir` and returns the archive path. `name`
/// is the backup's directory name, used both to nest the entries and to name the
/// default archive file (so distinct backups never collide on the same output).
/// `secure` forces owner-only permissions, used when the backup contains
/// credentials so the portable archive is never world-readable.
///
/// The work is synchronous std file IO on a few small files; the backup CLI is a
/// one-shot command about to exit, so briefly running it inline is acceptable.
pub fn write(
    backup_dir: &Path,
    name: &str,
    output: Option<&Path>,
    secure: bool,
) -> Result<PathBuf> {
    let dest = resolve_dest(name, output)?;
    let mut open_opts = std::fs::OpenOptions::new();
    open_opts.write(true).create(true).truncate(true);
    if secure {
        // Owner-only from creation: no window where a credential archive is world-readable.
        open_opts.mode(0o600);
    }
    let file = open_opts
        .open(&dest)
        .with_context(|| format!("Creating archive {}", dest.display()))?;
    if secure {
        // Enforce 0600 even when the destination already existed at looser perms.
        file.set_permissions(Permissions::from_mode(0o600))
            .with_context(|| format!("Securing archive {}", dest.display()))?;
    }
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

/// Extracts a `.tar.gz` backup into `dest` and returns the backup root directory
/// (the extracted directory that holds the manifest).
pub fn extract(archive_path: &Path, dest: &Path) -> Result<PathBuf> {
    let file = std::fs::File::open(archive_path)
        .with_context(|| format!("Opening archive {}", archive_path.display()))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let reader = LimitReader::new(decoder, MAX_EXTRACT_BYTES);
    tar::Archive::new(reader)
        .unpack(dest)
        .with_context(|| format!("Extracting archive {}", archive_path.display()))?;
    find_backup_root(dest).with_context(|| {
        format!(
            "Archive {} does not contain a backup",
            archive_path.display()
        )
    })
}

/// Locates the extracted backup root: `dest` when it holds a manifest, otherwise
/// its single manifest-bearing subdirectory (backups nest under their name).
fn find_backup_root(dest: &Path) -> Result<PathBuf> {
    if dest.join(MANIFEST_FILE_NAME).exists() {
        return Ok(dest.to_path_buf());
    }
    for entry in std::fs::read_dir(dest)?.flatten() {
        let path = entry.path();
        if path.join(MANIFEST_FILE_NAME).exists() {
            return Ok(path);
        }
    }
    anyhow::bail!("no manifest.toml found in archive")
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

/// Read adapter that fails once more than `remaining` bytes have been read,
/// bounding how much an archive is allowed to decompress to.
struct LimitReader<R> {
    inner: R,
    remaining: u64,
}

impl<R> LimitReader<R> {
    fn new(inner: R, limit: u64) -> Self {
        Self {
            inner,
            remaining: limit,
        }
    }
}

impl<R: Read> Read for LimitReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "backup archive is larger than the allowed maximum",
            ));
        }
        let cap = usize::try_from(self.remaining)
            .unwrap_or(usize::MAX)
            .min(buf.len());
        let read = self.inner.read(&mut buf[..cap])?;
        self.remaining -= read as u64;
        Ok(read)
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
        let archive = write(&backup_dir, "2026-07-12T10-00-00", Some(&out_dir), false).unwrap();
        assert!(archive.exists());

        let extract = tmp.path().join("extract");
        let decoder = flate2::read::GzDecoder::new(std::fs::File::open(&archive).unwrap());
        tar::Archive::new(decoder).unpack(&extract).unwrap();

        let restored = extract.join("2026-07-12T10-00-00").join("config.toml");
        assert_eq!(std::fs::read_to_string(restored).unwrap(), "answer = 42\n");
    }

    /// Goal: a `secure` archive (one that carries credentials) is owner-only,
    /// even when it overwrites a pre-existing looser file. Method: pre-create the
    /// destination at 0644, write a secure archive over it, and assert the mode.
    #[test]
    fn secure_archive_is_owner_only() {
        let tmp = tempfile::tempdir().unwrap();
        let backup_dir = tmp.path().join("2026-07-12T10-00-00");
        std::fs::create_dir_all(&backup_dir).unwrap();
        std::fs::write(backup_dir.join(".passwd"), "hash").unwrap();

        let dest = tmp.path().join("s.tar.gz");
        std::fs::write(&dest, b"stale").unwrap();
        std::fs::set_permissions(&dest, Permissions::from_mode(0o644)).unwrap();

        let secure = write(&backup_dir, "s", Some(&dest), true).unwrap();
        let mode = std::fs::metadata(&secure).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    /// Goal: the limit reader yields bytes up to the cap and then errors, so a
    /// decompression bomb cannot stream unbounded data into extraction. Method:
    /// read a 100-byte source through a 10-byte limit and assert the failure.
    #[test]
    fn limit_reader_errors_past_cap() {
        let data = vec![0_u8; 100];
        let mut reader = LimitReader::new(&data[..], 10);
        let mut out = Vec::new();
        let err = reader.read_to_end(&mut out).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(out.len() <= 10, "read {} bytes past the cap", out.len());
    }
}
