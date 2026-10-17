// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! glibc's mmap threshold. By default glibc raises it to the size of each large block freed,
//! up to 32 MiB, so the next block that size comes from a thread's heap and stays resident
//! after it is freed: the login's 19 MiB argon2 buffer, or the full status history response.
//! Pinning it keeps every large block in its own mapping, returned to the system on free.

/// Blocks this size or larger get their own mapping. glibc's own starting value.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
const MMAP_THRESHOLD_BYTES: i32 = 128 * 1024;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
const _: () = assert!(MMAP_THRESHOLD_BYTES > 0);
// glibc refuses a threshold above half its 64 MiB per-thread heap.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
const _: () = assert!(MMAP_THRESHOLD_BYTES <= 32 * 1024 * 1024);

/// Pins the threshold. Call first in `main`, before any thread allocates.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn pin() {
    // SAFETY: `mallopt` only sets an allocator parameter, and the value is in its range.
    let applied = unsafe { nix::libc::mallopt(nix::libc::M_MMAP_THRESHOLD, MMAP_THRESHOLD_BYTES) };
    debug_assert_eq!(applied, 1, "glibc refused the mmap threshold");
}

/// musl keeps no dynamic threshold to pin.
#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn pin() {}

#[cfg(all(
    test,
    target_os = "linux",
    target_env = "gnu",
    target_pointer_width = "64"
))]
mod tests {
    use super::*;
    use std::hint::black_box;

    /// glibc returns a mapped block two words into its first page.
    const MAPPED_OFFSET: usize = 16;
    const PAGE_BYTES: usize = 4096;
    /// Set when the test binary re-runs itself for the probe.
    const PROBE_ENV: &str = "CC_MALLOC_THRESHOLD_PROBE";
    const PROBE_TEST: &str = "malloc_threshold::tests::mapped_block_probe";
    /// THP heaps grow in 2 MiB steps, and a request that fits a heap's free top is served from
    /// it before the threshold is consulted, so probe blocks are larger than that.
    const BLOCK_BYTES: usize = 4 << 20;
    const FREED_BYTES: usize = 4 * BLOCK_BYTES;
    /// THP heaps, the aarch64 default since glibc 2.43, opted into so every platform runs them.
    const THP_TUNABLES: &str = "glibc.malloc.hugetlb=1";

    /// Goal: a large block stays in its own mapping after a larger one is freed, which would
    /// otherwise raise glibc's threshold past it, with the platform's malloc defaults and with
    /// THP heaps. Method: the probe runs in a fresh test process for each, since free heap
    /// space left by other tests serves a request before the threshold is consulted.
    #[test]
    fn large_blocks_stay_mapped_after_a_larger_free() {
        for tunables in [None, Some(THP_TUNABLES)] {
            let mut probe = std::process::Command::new(std::env::current_exe().unwrap());
            probe
                .args(["--exact", PROBE_TEST, "--ignored", "--test-threads=1"])
                .env(PROBE_ENV, "1");
            if let Some(tunables) = tunables {
                probe.env("GLIBC_TUNABLES", tunables);
            }
            let output = probe.output().unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(output.status.success(), "tunables {tunables:?}: {stdout}");
            assert!(
                stdout.contains("1 passed"),
                "the probe did not run with tunables {tunables:?}: {stdout}"
            );
        }
    }

    /// Goal: with the threshold pinned, 4 MiB blocks stay mapped after a 16 MiB free. Method:
    /// free a 16 MiB block, then check four 4 MiB blocks each start 16 bytes into a page, as a
    /// mapped block does; four heap blocks all landing there by chance is negligible. Returns
    /// early without `CC_MALLOC_THRESHOLD_PROBE`, so a direct `--ignored` run checks nothing.
    #[test]
    #[ignore = "run in a fresh process by large_blocks_stay_mapped_after_a_larger_free"]
    fn mapped_block_probe() {
        if std::env::var_os(PROBE_ENV).is_none() {
            return;
        }
        pin();
        drop(black_box(vec![1_u8; FREED_BYTES]));
        let blocks: Vec<Vec<u8>> = (0..4)
            .map(|_| black_box(Vec::with_capacity(BLOCK_BYTES)))
            .collect();
        for block in &blocks {
            assert_eq!(block.as_ptr() as usize % PAGE_BYTES, MAPPED_OFFSET);
        }
    }
}
