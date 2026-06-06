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

use anyhow::Result;
use std::fs::ReadDir;
use std::path::Path;

/// Reads the entire contents of a sysfs file into a UTF-8 encoded string.
///
/// Tailored for sysfs files, which are typically small and contain few values. Returns an error if
/// the file cannot be opened or read, or if the contents are not valid UTF-8.
///
/// This is the hot read path (every sensor, every tick). The idle-CPU win comes from compio's
/// completion-based IO. A managed buffer pool was tried here for registered buffers, but it
/// corrupts/fails the per-tick concurrent fan-out (many reads share one pool over the `io_uring`
/// buffer ring: cross-contaminated data or "flags are invalid"). The plain read is correct and
/// still completion-based.
pub async fn read_sysfs(path: impl AsRef<Path>) -> Result<String> {
    #[cfg(not(feature = "compio-rt"))]
    {
        Ok(tokio::fs::read_to_string(path).await?)
    }
    #[cfg(feature = "compio-rt")]
    {
        Ok(String::from_utf8(compio::fs::read(path.as_ref()).await?)?)
    }
}

/// Reads the entire contents of a text file into a UTF-8 encoded string.
///
/// Returns an error if the file cannot be opened or read, or if the contents are not valid UTF-8.
pub async fn read_txt(path: impl AsRef<Path>) -> Result<String> {
    #[cfg(not(feature = "compio-rt"))]
    {
        Ok(tokio::fs::read_to_string(path).await?)
    }
    #[cfg(feature = "compio-rt")]
    {
        Ok(String::from_utf8(compio::fs::read(path.as_ref()).await?)?)
    }
}

/// Reads the entire contents of a file into a vector of bytes. Tailored for reading images, which
/// are typically larger than other files.
///
/// Returns an error if the file cannot be opened or read.
pub async fn read_image(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    #[cfg(not(feature = "compio-rt"))]
    {
        Ok(tokio::fs::read(path).await?)
    }
    #[cfg(feature = "compio-rt")]
    {
        Ok(compio::fs::read(path.as_ref()).await?)
    }
}

/// Reads the contents of a directory.
///
/// Returns an iterator over the entries of the directory at `path`, or an error if `path` is not a
/// directory or any I/O fails. As a wrapper for `std::fs::read_dir` it should be called sparingly,
/// but is generally very fast and only used during application startup.
pub fn read_dir(path: impl AsRef<Path>) -> Result<ReadDir> {
    Ok(std::fs::read_dir(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Goal: `read_sysfs` must return EXACTLY the file's bytes, no stale/garbage tail. A wrong
    /// length leaves trailing bytes that break numeric parsing ("invalid digit found in string").
    /// Method: write a longer value first, then shorter ones, and read each back asserting exact
    /// equality. A reused buffer with bad length tracking would leak the longer value's tail into a
    /// later short read.
    #[test]
    fn read_sysfs_returns_exact_contents() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let cases = [
                ("long", "1234567890\n"),
                ("short", "42\n"),
                ("no_newline", "0"),
                ("byte", "255\n"),
            ];
            for (name, contents) in cases {
                std::fs::write(dir.path().join(name), contents).unwrap();
            }
            for (name, contents) in cases {
                let read = read_sysfs(dir.path().join(name)).await.unwrap();
                assert_eq!(
                    read,
                    contents,
                    "read_sysfs returned wrong bytes for {name} (len {} vs {})",
                    read.len(),
                    contents.len()
                );
            }
        });
    }

    /// Goal: reading a real sysfs file (kernfs, reports size 4096, generated per read) must return
    /// only its actual bytes. This reproduces the live-daemon corruption that regular tempfiles do
    /// not. Method: find a readable numeric hwmon input and assert the value parses cleanly and is
    /// short. Skips when /sys has no readable hwmon input (e.g. a CI sandbox).
    #[test]
    fn read_sysfs_real_file_has_no_stale_tail() {
        crate::rt::test_runtime(async {
            let Some(path) = first_readable_hwmon_input() else {
                eprintln!("no readable hwmon *_input found; skipping sysfs read test");
                return;
            };
            let read = read_sysfs(&path).await.unwrap();
            assert!(
                read.len() < 64,
                "sysfs read returned a stale/garbage tail: len {} for {}",
                read.len(),
                path.display()
            );
            assert!(
                read.trim().parse::<i64>().is_ok(),
                "sysfs value did not parse cleanly: {read:?} for {}",
                path.display()
            );
        });
    }

    /// Goal: concurrent `read_sysfs` calls sharing the thread-local buffer pool (as the hwmon hot
    /// path does each tick) must each return their own file's bytes. Method: fan many simultaneous
    /// reads of distinct, varied-length files through a moro scope and assert exact content, which
    /// catches buffer-ring cross-contamination or stale-length reuse.
    #[test]
    fn read_sysfs_concurrent_reads_are_not_cross_contaminated() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            const N: usize = 256; // exceed the 64-buffer pool to stress ring reuse
            let mut expected = Vec::with_capacity(N);
            for i in 0..N {
                let contents = format!("{}\n", "9".repeat((i % 17) + 1));
                std::fs::write(dir.path().join(i.to_string()), &contents).unwrap();
                expected.push(contents);
            }
            let results: std::rc::Rc<std::cell::RefCell<Vec<Option<String>>>> =
                std::rc::Rc::new(std::cell::RefCell::new(vec![None; N]));
            moro_local::async_scope!(|scope| {
                for i in 0..N {
                    let base = dir.path().to_path_buf();
                    let results = std::rc::Rc::clone(&results);
                    scope.spawn(async move {
                        let read = read_sysfs(base.join(i.to_string())).await.unwrap();
                        results.borrow_mut()[i] = Some(read);
                    });
                }
            })
            .await;
            let results = results.borrow();
            for i in 0..N {
                assert_eq!(
                    results[i].as_deref(),
                    Some(expected[i].as_str()),
                    "concurrent read {i} mismatched (cross-contaminated buffer?)"
                );
            }
        });
    }

    fn first_readable_hwmon_input() -> Option<std::path::PathBuf> {
        let hwmons = std::fs::read_dir("/sys/class/hwmon").ok()?;
        for hwmon in hwmons.flatten() {
            let Ok(files) = std::fs::read_dir(hwmon.path()) else {
                continue;
            };
            for file in files.flatten() {
                let name = file.file_name();
                let name = name.to_string_lossy();
                let is_input = (name.starts_with("fan") || name.starts_with("temp"))
                    && name.ends_with("_input");
                if is_input && std::fs::File::open(file.path()).is_ok() {
                    return Some(file.path());
                }
            }
        }
        None
    }
}
