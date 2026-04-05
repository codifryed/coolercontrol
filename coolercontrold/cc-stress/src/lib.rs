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

//! Hardware stress testing for thermal validation.
//!
//! Runs as a subprocess spawned by the daemon's `StressTestActor`. Each
//! stress mode (CPU, RAM, GPU, drive) is designed to push a specific
//! subsystem to maximum thermal output so users can verify their cooling
//! profiles under real load.
//!
//! # Architecture
//!
//! - **CPU**: Spawns N OS threads each running a tight matrix product
//!   loop (C = A x B, 128x128 f64). The triple-nested multiply
//!   balances sustained FMA compute with L1->L2 cache traffic for
//!   maximum heat. Inspired by stress-ng's matrixprod method.
//! - **RAM**: Similar to CPU but uses non-temporal stores (AVX2) to
//!   bypass cache and stream directly to DRAM, stressing DIMMs and the
//!   memory controller.
//! - **GPU**: Uses wgpu compute shaders with mixed FMA + sin/cos +
//!   sqrt to stress both ALU and Special Function Units through a
//!   ring of VRAM buffers. Values are kept bounded via `fract()` to
//!   prevent inf/NaN fast-paths that would defeat the stress.
//! - **Drive**: Performs random 4 KiB `O_DIRECT` reads on a block
//!   device with 16 I/O threads to maximize IOPS and drive controller
//!   heat without write wear.
//!
//! # Unsafe usage
//!
//! Unsafe is used in this crate for:
//! - Manual aligned heap allocation (`alloc::alloc`/`dealloc`) because
//!   standard `Vec` cannot guarantee the 32-byte (SIMD) or 4096-byte
//!   (`O_DIRECT` page) alignment these workloads require.
//! - AVX2/FMA SIMD intrinsics, which are inherently unsafe in Rust.
//! - Linux syscalls (`nice`, `ioctl`, `pread`) that have no safe
//!   wrapper in the `nix` crate at the version we use.

use std::alloc;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::io::AsRawFd;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

/// Mildly reduced scheduling priority. High enough that the schedutil
/// CPU frequency governor still sees significant utilization via PELT
/// (Per-Entity Load Tracking) and boosts clock speeds, but low enough
/// to yield to the daemon and desktop compositor when competing on the
/// same core. Nice 19 suppresses PELT weight to ~1/68th of normal,
/// causing the governor to select low P-states and defeating the
/// purpose of a thermal stress test.
const NICE_LEVEL: i32 = 5;

/// Matrix dimension for CPU stress. Three 128x128 f64 matrices total
/// 384 KiB, which fits in L2 cache (typically 256 KiB to 1 MiB per core)
/// but not L1 (32 KiB). This creates sustained L1 miss -> L2 hit traffic
/// that exercises the cache hierarchy alongside FPU compute, generating
/// more heat than either pure register loops or pure memory streaming.
/// Inspired by stress-ng's matrixprod method, which benchmarks as the
/// highest heat generator on `x86_64` (Colin Ian King, GPL-2+).
const MAT_SIZE: usize = 128;
/// Total element count per matrix (128 x 128 = 16,384 f64 values).
const MAT_ELEMENTS: usize = MAT_SIZE * MAT_SIZE;
/// 32 bytes = 256 bits, matching AVX2 register width. Required for
/// `_mm256_load_pd`/`_mm256_store_pd` which fault on unaligned addresses.
const SIMD_ALIGN: usize = 32;
/// Number of f64 values packed in one AVX2 256-bit register.
#[cfg(target_arch = "x86_64")]
const F64S_PER_AVX2: usize = 4;

/// Fraction of `MemAvailable` to allocate for RAM stress.
pub const RAM_STRESS_ALLOC_FRACTION: f64 = 0.8;

/// Per-buffer size for discrete GPUs (256 MiB, total ring = 2 GiB).
const MEM_STRESS_BUF_DISCRETE: u64 = 256 * 1024 * 1024;
/// Per-buffer size for integrated GPUs (16 MiB, total ring = 128 MiB).
const MEM_STRESS_BUF_INTEGRATED: u64 = 16 * 1024 * 1024;
/// Number of buffers forming the ring.
const MEM_STRESS_RING_SIZE: usize = 8;
/// Workgroup size for the streaming shader.
const MEM_STRESS_WORKGROUP: u32 = 256;
/// PCI vendor IDs for GPU-specific tuning.
const PCI_VENDOR_NVIDIA: u32 = 0x10DE;
const PCI_VENDOR_AMD: u32 = 0x1002;
/// Sleep between GPU submissions for desktop compositing (ms).
const GPU_SUBMIT_SLEEP_MS: u64 = 0;

/// Per-read block size for drive stress (4 KiB). Small random reads
/// maximize IOPS (I/O operations per second), which stresses the drive
/// controller harder than fewer large reads. 4 KiB matches the typical
/// logical sector size and OS page size, ensuring each read is a
/// distinct command to the drive controller.
const DRIVE_STRESS_BLOCK_SIZE: usize = 4 * 1024;
/// Page alignment required for `O_DIRECT` buffers.
const DRIVE_STRESS_ALIGNMENT: usize = 4096;
/// Default number of I/O threads for drive stress. Each thread issues
/// synchronous `pread()` calls, so the thread count equals the maximum
/// I/O queue depth. Modern `NVMe` drives generate the most heat at high
/// queue depth; 16 threads provide a reasonable default without
/// overwhelming the system's thread scheduler.
pub const DRIVE_STRESS_DEFAULT_THREADS: u16 = 16;
/// Number of reads between deadline checks. With 4 KiB blocks, each
/// pread completes in ~10-50us on `NVMe`, so 2048 reads takes ~20-100 ms
/// between time checks.
const READS_PER_DEADLINE_CHECK: u32 = 2048;

/// Returns the number of logical processors by counting entries in /proc/cpuinfo.
/// This is not restricted by CPU affinity or cgroup limits
/// (unlike `available_parallelism`).
#[must_use]
pub fn online_cpu_count() -> u16 {
    std::fs::read_to_string("/proc/cpuinfo")
        .map(|content| {
            // CPU count safely capped at u16::MAX (65535 cores).
            u16::try_from(
                content
                    .lines()
                    .filter(|line| line.starts_with("processor"))
                    .count(),
            )
            .unwrap_or(u16::MAX)
        })
        .unwrap_or(1)
        .max(1)
}

/// Returns available system memory in bytes from /proc/meminfo.
///
/// # Errors
///
/// Returns an error if /proc/meminfo cannot be read or parsed.
pub fn available_memory_bytes() -> Result<u64> {
    let content = std::fs::read_to_string("/proc/meminfo")
        .map_err(|e| anyhow!("Failed to read /proc/meminfo: {e}"))?;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("MemAvailable:") {
            let kb_str = rest.trim().strip_suffix("kB").unwrap_or(rest).trim();
            let kb: u64 = kb_str
                .parse()
                .map_err(|e| anyhow!("Failed to parse MemAvailable: {e}"))?;
            return Ok(kb * 1024);
        }
    }
    Err(anyhow!("MemAvailable not found in /proc/meminfo"))
}

/// Reset CPU affinity to all online CPUs.
/// The daemon may be restricted to a single CPU by systemd, and child
/// processes inherit that mask. We need full access for stress testing.
fn reset_cpu_affinity() -> Result<()> {
    let cpu_count = online_cpu_count();
    let mut cpu_set = nix::sched::CpuSet::new();
    for i in 0..cpu_count {
        cpu_set
            .set(i as usize)
            .map_err(|e| anyhow!("Failed to set CPU {i} in affinity mask: {e}"))?;
    }
    nix::sched::sched_setaffinity(nix::unistd::Pid::from_raw(0), &cpu_set)
        .map_err(|e| anyhow!("Failed to reset CPU affinity: {e}"))
}

/// Lower this process's scheduling priority so stress workloads do not
/// starve the daemon or desktop. Uses `libc::nice()` directly because
/// `nix` 0.31 does not provide a safe wrapper for it.
fn set_nice_level() -> Result<()> {
    // SAFETY: nice() is always safe to call. It cannot corrupt memory;
    // it only asks the kernel to adjust this process's scheduling
    // priority. No pointers, no shared state, no invariants to uphold.
    let result = unsafe { libc::nice(NICE_LEVEL) };
    // nice() returns -1 on error, but -1 is also a valid new priority.
    // The only way to distinguish: clear errno before the call (libc
    // does this), then check errno after. If errno is 0, the call
    // succeeded even though the return value is -1.
    if result == -1 {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() != Some(0) {
            return Err(anyhow!("Failed to set nice level: {err}"));
        }
    }
    Ok(())
}

/// Heap buffer with guaranteed 32-byte alignment for AVX2 SIMD loads/stores.
///
/// Standard `Vec<f64>` only guarantees 8-byte alignment (f64's natural
/// alignment). AVX2 aligned instructions (`_mm256_load_pd`,
/// `_mm256_store_pd`) require 32-byte alignment and will segfault on
/// unaligned addresses. Manual allocation via `alloc::alloc` lets us
/// specify the exact alignment the hardware needs.
///
/// Owns its allocation and frees it on drop. Not `Send` by default
/// (raw pointer), but only used within the thread that creates it.
struct AlignedBuffer {
    ptr: *mut f64,
    layout: alloc::Layout,
    len: usize,
}

impl AlignedBuffer {
    fn new(count: usize) -> Self {
        let size = count * size_of::<f64>();
        let layout = alloc::Layout::from_size_align(size, SIMD_ALIGN).expect("valid layout");
        // SAFETY: Layout has non-zero size (count > 0 for all callers)
        // and valid alignment (SIMD_ALIGN is a power of two). alloc()
        // returns a pointer to `size` bytes with the requested alignment,
        // or null on failure.
        let ptr = unsafe { alloc::alloc(layout) };
        if ptr.is_null() {
            alloc::handle_alloc_error(layout);
        }
        // Alignment is sound: SIMD_ALIGN (32) exceeds f64's alignment (8).
        #[allow(clippy::cast_ptr_alignment)]
        let ptr = ptr.cast::<f64>();
        // SAFETY: alloc() just returned a valid, non-null pointer to
        // `count * size_of::<f64>()` bytes. Casting to `*mut f64` and
        // building a slice of `count` elements is in bounds.
        let init_slice = unsafe { std::slice::from_raw_parts_mut(ptr, count) };
        // Initialize to small positive values (1.0 + epsilon). Values
        // near zero risk becoming IEEE 754 denormals, which modern CPUs
        // handle in microcode at 50-100x penalty per operation. That
        // would throttle the stress loop instead of maximizing heat.
        for (i, elem) in init_slice.iter_mut().enumerate() {
            // Precision loss is intentional; exact init values do not matter.
            #[allow(clippy::cast_precision_loss)]
            {
                *elem = 1.0 + (i as f64) * 1e-10;
            }
        }
        Self {
            ptr,
            layout,
            len: count,
        }
    }

    /// Raw pointer access for AVX2 intrinsics that require `*mut f64`.
    #[cfg(target_arch = "x86_64")]
    fn as_mut_ptr(&mut self) -> *mut f64 {
        self.ptr
    }

    /// Safe slice view for scalar (non-SIMD) code paths.
    fn as_mut_slice(&mut self) -> &mut [f64] {
        // SAFETY: ptr points to self.len initialized f64 elements that
        // remain valid and exclusively owned until drop. The slice
        // borrows &mut self, preventing aliasing.
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl Drop for AlignedBuffer {
    fn drop(&mut self) {
        // SAFETY: ptr was allocated with alloc::alloc(self.layout) in
        // new(). We pass the same layout back to dealloc, satisfying the
        // allocator contract. No other code frees this pointer.
        unsafe { alloc::dealloc(self.ptr.cast(), self.layout) };
    }
}

/// Run CPU stress test using matrix product on N threads.
///
/// Each thread repeatedly computes C = A x B on 128x128 f64 matrices.
/// This generates maximum sustained heat by balancing FPU compute
/// (one FMA per inner-loop iteration) with L1->L2 cache traffic
/// (column-wise B access defeats the L1 prefetcher).
///
/// # Errors
///
/// Returns an error if CPU affinity or nice level cannot be set.
pub fn run_cpu_stress(threads: Option<u16>, timeout_secs: u16) -> Result<()> {
    reset_cpu_affinity()?;
    set_nice_level()?;

    let num_threads = threads.unwrap_or_else(online_cpu_count);
    eprintln!(
        "CPU stress: {num_threads} threads, {timeout_secs}s (pid: {})",
        std::process::id()
    );

    let duration = Duration::from_secs(u64::from(timeout_secs));
    let deadline = Instant::now() + duration;
    let mut handles = Vec::with_capacity(num_threads as usize);

    for _ in 0..num_threads {
        handles.push(std::thread::spawn(move || {
            cpu_stress_matrixprod(deadline);
        }));
    }

    for handle in handles {
        let _ = handle.join();
    }

    Ok(())
}

/// Matrix product loop for maximum sustained CPU heat generation.
///
/// Computes C = A x B repeatedly using a triple-nested loop. The inner
/// multiply-accumulate (`sum += A[i][k] * B[k][j]`) keeps the FMA unit
/// busy every cycle. Column-wise access to B (stride = 1 KiB between
/// consecutive k values) defeats the L1 prefetcher, creating steady
/// L1 miss -> L2 hit traffic that exercises the cache coherency
/// hardware. The total working set (384 KiB for three matrices) fits
/// in L2 but not L1, so misses resolve quickly (~12 cycles) without
/// long DRAM stalls.
///
/// After each multiply, A and C are swapped so the result feeds back
/// as input. This creates cross-iteration data dependencies that
/// prevent the compiler from hoisting or eliminating the computation.
fn cpu_stress_matrixprod(deadline: Instant) {
    let mut a = vec![0.0_f64; MAT_ELEMENTS];
    let mut b = vec![0.0_f64; MAT_ELEMENTS];
    let mut c = vec![0.0_f64; MAT_ELEMENTS];

    // Initialize with small positive values. Varying per-element avoids
    // degenerate patterns (all-zeros, all-ones) that could let the CPU
    // skip work via fast-path detection.
    for i in 0..MAT_ELEMENTS {
        // Precision loss is intentional; exact init values do not matter.
        #[allow(clippy::cast_precision_loss)]
        let v = 1.0 + (i as f64) * 1e-10;
        a[i] = v;
        b[i] = v;
    }

    while Instant::now() < deadline {
        // C = A x B. Each multiply does MAT_SIZE^3 = ~2M FMA ops,
        // taking ~1-5 ms depending on CPU frequency.
        for i in 0..MAT_SIZE {
            for j in 0..MAT_SIZE {
                let mut sum = 0.0_f64;
                for k in 0..MAT_SIZE {
                    // mul_add compiles to a single FMA instruction on
                    // hardware that supports it (x86_64 FMA3, aarch64).
                    sum = a[i * MAT_SIZE + k].mul_add(b[k * MAT_SIZE + j], sum);
                }
                c[i * MAT_SIZE + j] = sum;
            }
        }
        // Prevent dead-code elimination of the result matrix.
        std::hint::black_box(&c);
        // Feed result back as input for the next iteration. This
        // creates a data dependency across iterations, preventing the
        // compiler from optimizing away repeated identical multiplies.
        std::mem::swap(&mut a, &mut c);
    }
}

/// Run RAM stress test by streaming read-modify-write across a large allocation.
/// Stresses DIMMs and the CPU's memory controller for maximum memory heat.
///
/// # Errors
///
/// Returns an error if CPU affinity or nice level cannot be set.
pub fn run_ram_stress(alloc_bytes: u64, timeout_secs: u16) -> Result<()> {
    reset_cpu_affinity()?;
    set_nice_level()?;

    let num_threads = online_cpu_count();
    let per_thread_f64s = (alloc_bytes / u64::from(num_threads) / 8) as usize;
    let per_thread_bytes = per_thread_f64s * 8;
    let total_mb = u64::from(num_threads) * per_thread_bytes as u64 / (1024 * 1024);
    eprintln!(
        "RAM stress: {num_threads} threads, {total_mb} MiB total, {timeout_secs}s (pid: {})",
        std::process::id()
    );

    let duration = Duration::from_secs(u64::from(timeout_secs));
    let deadline = Instant::now() + duration;
    let mut handles = Vec::with_capacity(num_threads as usize);

    for _ in 0..num_threads {
        handles.push(std::thread::spawn(move || {
            let mut buf = AlignedBuffer::new(per_thread_f64s);
            #[cfg(target_arch = "x86_64")]
            if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
                // SAFETY: AVX2+FMA support confirmed by runtime feature detection.
                unsafe { ram_stress_loop_avx2(buf.as_mut_ptr(), buf.len, deadline) };
                return;
            }
            ram_stress_loop_scalar(buf.as_mut_slice(), deadline);
        }));
    }

    for handle in handles {
        let _ = handle.join();
    }

    Ok(())
}

/// Scalar RAM stress: 4x-unrolled sequential read-modify-write.
///
/// Sequential access is intentional: it lets the hardware prefetcher
/// run at full speed, maximizing sustained memory bandwidth (and thus
/// DIMM and memory controller heat). Random access would bottleneck on
/// TLB misses and prefetch failures instead of raw bandwidth.
///
/// The 4x unroll gives the CPU's out-of-order engine independent
/// operations to overlap, hiding memory latency.
fn ram_stress_loop_scalar(buf: &mut [f64], deadline: Instant) {
    let multiplier = 1.000_000_000_1_f64;
    let addend = 0.999_999_999_9_f64;
    let len = buf.len();
    let unrolled = len / 4 * 4;
    while Instant::now() < deadline {
        // Unrolled sequential sweep for maximum hardware prefetcher utilization.
        let mut i = 0;
        while i < unrolled {
            buf[i] = buf[i].mul_add(multiplier, addend);
            buf[i + 1] = buf[i + 1].mul_add(multiplier, addend);
            buf[i + 2] = buf[i + 2].mul_add(multiplier, addend);
            buf[i + 3] = buf[i + 3].mul_add(multiplier, addend);
            i += 4;
        }
        for elem in &mut buf[unrolled..len] {
            *elem = elem.mul_add(multiplier, addend);
        }
    }
    std::hint::black_box(buf[0]);
}

/// AVX2 RAM stress: non-temporal stores bypass all cache levels and
/// write directly to DRAM via write-combining buffers.
///
/// This is the key difference from CPU stress: `_mm256_stream_pd`
/// (VMOVNTPD) avoids polluting L1/L2/L3 cache, forcing every write to
/// travel the full path to the DIMMs. Combined with sequential access
/// (prefetcher-friendly), this saturates memory bandwidth.
///
/// `_mm_sfence` (SFENCE) after each full sweep ensures all
/// non-temporal stores are globally visible before the next pass.
/// Without it, stores could linger in write-combining buffers
/// indefinitely, reducing effective bandwidth.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
// Suppresses per-intrinsic unsafe blocks inside this already-unsafe fn.
// The buffer is 32-byte aligned (AlignedBuffer guarantees SIMD_ALIGN),
// and all pointer offsets stay within the allocated region.
#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn ram_stress_loop_avx2(buf: *mut f64, len: usize, deadline: Instant) {
    use std::arch::x86_64::{
        _mm256_fmadd_pd, _mm256_load_pd, _mm256_set1_pd, _mm256_stream_pd, _mm_sfence,
    };

    let vmul = _mm256_set1_pd(1.000_000_000_1);
    let vadd = _mm256_set1_pd(0.999_999_999_9);
    let chunks = len / F64S_PER_AVX2;
    // Process 4 consecutive 32-byte vectors (128 bytes) per iteration.
    let unrolled = chunks / 4 * 4;

    while Instant::now() < deadline {
        let mut i = 0;
        while i < unrolled {
            let off0 = i * F64S_PER_AVX2;
            let p0 = buf.add(off0);
            let v0 = _mm256_load_pd(p0);
            _mm256_stream_pd(p0, _mm256_fmadd_pd(v0, vmul, vadd));

            let p1 = buf.add(off0 + F64S_PER_AVX2);
            let v1 = _mm256_load_pd(p1);
            _mm256_stream_pd(p1, _mm256_fmadd_pd(v1, vmul, vadd));

            let p2 = buf.add(off0 + F64S_PER_AVX2 * 2);
            let v2 = _mm256_load_pd(p2);
            _mm256_stream_pd(p2, _mm256_fmadd_pd(v2, vmul, vadd));

            let p3 = buf.add(off0 + F64S_PER_AVX2 * 3);
            let v3 = _mm256_load_pd(p3);
            _mm256_stream_pd(p3, _mm256_fmadd_pd(v3, vmul, vadd));

            i += 4;
        }
        // Handle remaining chunks.
        for j in unrolled..chunks {
            let p = buf.add(j * F64S_PER_AVX2);
            let v = _mm256_load_pd(p);
            _mm256_stream_pd(p, _mm256_fmadd_pd(v, vmul, vadd));
        }
        _mm_sfence();
    }
}

/// Xorshift64 PRNG for generating random block device read offsets.
///
/// Not cryptographically secure, but that is irrelevant here. We only
/// need a fast, uniform-ish distribution of offsets to prevent the
/// drive's read cache from absorbing all the I/O. The xorshift family
/// has zero allocation, no heap, and compiles to ~6 instructions.
struct XorShift64(u64);

impl XorShift64 {
    fn new(seed: u64) -> Self {
        // Xorshift gets stuck in the zero state forever if initialized
        // with zero. OR-ing with 1 guarantees a non-zero starting state.
        Self(seed | 1)
    }

    /// Shift constants (13, 7, 17) are from Marsaglia's 2003 paper
    /// "Xorshift RNGs". This triple has a full 2^64 - 1 period.
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// Page-aligned (4096-byte) heap buffer for `O_DIRECT` reads.
///
/// Linux `O_DIRECT` requires the user-space buffer to be aligned to the
/// block device's logical sector size (typically 512 or 4096 bytes).
/// Standard `Vec<u8>` only guarantees 1-byte alignment. Manual
/// allocation via `alloc::alloc` lets us specify page alignment.
struct DirectIoBuffer {
    ptr: *mut u8,
    layout: alloc::Layout,
}

// SAFETY: Raw pointers are `!Send` by default as a conservative lint,
// but this buffer is a sole-ownership heap allocation (like a `Box`).
// Only one thread ever holds it, and ownership is moved (not shared)
// into the drive stress thread via `std::thread::spawn(move || ...)`.
unsafe impl Send for DirectIoBuffer {}

impl DirectIoBuffer {
    fn new(size: usize) -> Self {
        let layout =
            alloc::Layout::from_size_align(size, DRIVE_STRESS_ALIGNMENT).expect("valid layout");
        // SAFETY: Layout is non-zero (DRIVE_STRESS_BLOCK_SIZE > 0) and
        // alignment is a power of two (4096). Returns a pointer to
        // `size` bytes at the requested alignment.
        let ptr = unsafe { alloc::alloc(layout) };
        if ptr.is_null() {
            alloc::handle_alloc_error(layout);
        }
        Self { ptr, layout }
    }

    /// Raw pointer for `libc::pread`, which requires `*mut c_void`.
    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr
    }

    /// Safe read-only view, used for `black_box` at loop end.
    fn as_slice(&self) -> &[u8] {
        // SAFETY: ptr points to self.layout.size() allocated bytes that
        // remain valid until drop. The shared borrow of &self prevents
        // concurrent mutation.
        unsafe { std::slice::from_raw_parts(self.ptr, self.layout.size()) }
    }
}

impl Drop for DirectIoBuffer {
    fn drop(&mut self) {
        // SAFETY: ptr was allocated with alloc::alloc(self.layout) in
        // new(). Same layout is passed to dealloc. No double-free: this
        // is the only deallocation path (drop runs exactly once).
        unsafe { alloc::dealloc(self.ptr, self.layout) };
    }
}

// BLKGETSIZE64 ioctl number, computed from the Linux kernel macro:
//   _IOR(0x12, 114, sizeof(u64))
// Breakdown: direction=read (0x80000000) | size=8 bytes (8 << 16) |
//            type=0x12 (block device) | nr=114
// This is a stable kernel ABI, safe to hardcode.
// Type is `libc::Ioctl` to match the ioctl() signature on both glibc
// (c_ulong) and musl (c_int).
// Wrapping and sign loss are intentional: the kernel ioctl number's
// bit pattern must be preserved regardless of the platform's Ioctl type.
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
const BLKGETSIZE64: libc::Ioctl = 0x8008_1272_u32 as i32 as libc::Ioctl;

/// Returns the size of a block device in bytes via the BLKGETSIZE64
/// ioctl. Used to compute the valid offset range for random reads.
fn block_device_size(path: &str) -> Result<u64> {
    let file = std::fs::File::open(path)
        .map_err(|e| anyhow!("Failed to open block device {path}: {e}"))?;
    let fd = file.as_raw_fd();
    let mut size: u64 = 0;
    // SAFETY: Three conditions for a sound ioctl call:
    // 1. fd is a valid, open file descriptor (File::open succeeded).
    // 2. BLKGETSIZE64 expects a `*mut u64` as its third argument, and
    //    `&mut size` provides exactly that with correct alignment.
    // 3. The kernel writes exactly 8 bytes (u64) into the pointer;
    //    our local variable has that size.
    let ret = unsafe { libc::ioctl(fd, BLKGETSIZE64, &mut size) };
    if ret < 0 {
        return Err(anyhow!(
            "BLKGETSIZE64 ioctl failed on {path}: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(size)
}

/// Run drive stress test with random read-only I/O using `O_DIRECT`.
/// Performs random reads on the specified block device to generate heat
/// without causing any drive wear (reads only, no writes).
///
/// # Errors
///
/// Returns an error if the device path is not a block device, is too
/// small, or if CPU affinity/nice level cannot be set.
pub fn run_drive_stress(device_path: &str, threads: u16, timeout_secs: u16) -> Result<()> {
    reset_cpu_affinity()?;
    set_nice_level()?;

    let metadata =
        std::fs::metadata(device_path).map_err(|e| anyhow!("Failed to stat {device_path}: {e}"))?;
    if !metadata.file_type().is_block_device() {
        return Err(anyhow!("{device_path} is not a block device"));
    }

    let device_size = block_device_size(device_path)?;
    let block_size = DRIVE_STRESS_BLOCK_SIZE as u64;
    if device_size < block_size {
        return Err(anyhow!(
            "Device {device_path} too small ({device_size} bytes) for stress testing"
        ));
    }
    let max_offset = device_size - block_size;

    eprintln!(
        "Drive stress: {device_path}, {threads} threads, {timeout_secs}s, \
         {device_size} bytes (pid: {})",
        std::process::id()
    );

    let duration = Duration::from_secs(u64::from(timeout_secs));
    let deadline = Instant::now() + duration;
    let path = device_path.to_string();
    let mut handles = Vec::with_capacity(threads as usize);

    for thread_idx in 0..threads {
        let path = path.clone();
        handles.push(std::thread::spawn(move || {
            drive_stress_thread(&path, thread_idx, max_offset, deadline);
        }));
    }

    for handle in handles {
        let _ = handle.join();
    }

    Ok(())
}

/// Per-thread drive stress worker.
///
/// Opens the block device with `O_DIRECT` to bypass the kernel page
/// cache. Without `O_DIRECT`, repeated reads would be served from RAM
/// after the first pass, generating zero drive activity. `O_DIRECT`
/// forces every read to hit the physical drive.
///
/// Reads are random to defeat the drive's internal read-ahead cache
/// and force seek operations (on HDDs) or spread wear evenly across
/// NAND pages (on SSDs). Read-only by design to avoid drive wear.
fn drive_stress_thread(path: &str, thread_idx: u16, max_offset: u64, deadline: Instant) {
    use std::fs::OpenOptions;
    use std::os::unix::fs::OpenOptionsExt;

    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECT)
        .open(path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Thread {thread_idx}: failed to open {path}: {e}");
            return;
        }
    };
    let fd = file.as_raw_fd();

    let mut buf = DirectIoBuffer::new(DRIVE_STRESS_BLOCK_SIZE);
    // Mix thread index with a large prime (from PCG's default
    // multiplier) and XOR with nanosecond time. This gives each thread
    // a different random offset sequence, spreading I/O across the
    // full device instead of all threads reading the same blocks.
    // Truncation is intentional: we only need entropy for PRNG seeding.
    #[allow(clippy::cast_possible_truncation)]
    let seed = u64::from(thread_idx).wrapping_mul(6_364_136_223_846_793_005)
        ^ (Instant::now().elapsed().as_nanos() as u64);
    let mut rng = XorShift64::new(seed);

    while Instant::now() < deadline {
        for _ in 0..READS_PER_DEADLINE_CHECK {
            // Align offset to DRIVE_STRESS_BLOCK_SIZE boundary.
            let offset = (rng.next() % (max_offset / DRIVE_STRESS_BLOCK_SIZE as u64))
                * DRIVE_STRESS_BLOCK_SIZE as u64;
            // SAFETY: pread reads into a user-supplied buffer without
            // modifying the file descriptor's offset (thread-safe).
            // - fd: valid open file descriptor (OpenOptions above).
            // - buf: page-aligned (DirectIoBuffer), satisfying O_DIRECT.
            // - count: DRIVE_STRESS_BLOCK_SIZE matches buf's allocation.
            // - offset: within [0, device_size - block_size], guaranteed
            //   by max_offset computation in run_drive_stress.
            // Wrap is not possible: max device offset fits in i64.
            #[allow(clippy::cast_possible_wrap)]
            let _ret = unsafe {
                libc::pread(
                    fd,
                    buf.as_mut_ptr().cast(),
                    DRIVE_STRESS_BLOCK_SIZE,
                    offset as libc::off_t,
                )
            };
            // Transient I/O errors are expected on some drives; ignore.
        }
    }
    // Prevent the compiler from optimizing away the read loop by
    // marking the buffer content as observable.
    std::hint::black_box(buf.as_slice()[0]);
}

/// Run GPU stress test using wgpu compute shaders for memory-bandwidth stress.
/// Enumerates all available GPU adapters and stresses them in parallel.
///
/// # Errors
///
/// Returns an error if no hardware GPU adapter is found, if all adapters
/// fail to initialize, or if CPU affinity/nice level cannot be set.
pub async fn run_gpu_stress(timeout_secs: u16) -> Result<()> {
    reset_cpu_affinity()?;
    set_nice_level()?;

    let duration = Duration::from_secs(u64::from(timeout_secs));
    let backends = wgpu::Backends::VULKAN | wgpu::Backends::GL;

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends,
        ..Default::default()
    });

    let adapters = instance.enumerate_adapters(backends);
    if adapters.is_empty() {
        return Err(anyhow!(
            "No GPU adapter found. \
             Ensure Vulkan or OpenGL ES drivers are installed."
        ));
    }

    // Deduplicate by device ID, preferring Vulkan over GL to avoid
    // software-rendered GL backends consuming CPU instead of GPU.
    let mut seen_devices = std::collections::HashSet::with_capacity(adapters.len());
    let mut unique_adapters = Vec::with_capacity(adapters.len());
    for adapter in adapters {
        let info = adapter.get_info();
        if info.device_type == wgpu::DeviceType::Cpu {
            continue;
        }
        let device_key = (info.vendor, info.device);
        if seen_devices.insert(device_key) {
            unique_adapters.push(adapter);
        }
    }
    if unique_adapters.is_empty() {
        return Err(anyhow!(
            "No hardware GPU adapter found. \
             Ensure Vulkan or OpenGL ES drivers are installed."
        ));
    }

    let adapter_count = unique_adapters.len();
    let mut handles = Vec::with_capacity(adapter_count);
    for adapter in unique_adapters {
        let info = adapter.get_info();
        eprintln!("Stressing GPU: {} ({:?})", info.name, info.backend);
        handles.push(tokio::spawn(stress_adapter(adapter, duration)));
    }

    let mut errors = Vec::new();
    for handle in handles {
        if let Err(e) = handle.await? {
            errors.push(e);
        }
    }
    if errors.len() == adapter_count {
        return Err(anyhow!(
            "All GPU adapters failed: {}",
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }

    Ok(())
}

/// WebGPU/Vulkan spec limit for workgroups per dispatch dimension.
/// When the total workgroup count exceeds this, we split across X and Y.
const MAX_WORKGROUPS_PER_DIM: u32 = 65535;

/// Holds all GPU objects needed for the stress loop. Kept as a struct so
/// `stress_adapter` can reference them without passing 7 arguments.
/// `_buffers` is kept alive to prevent deallocation while bind groups
/// reference them.
struct GpuStressResources {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_groups: Vec<wgpu::BindGroup>,
    dispatch_x: u32,
    dispatch_y: u32,
    ring_reps: u32,
    _buffers: Vec<wgpu::Buffer>,
}

fn create_storage_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    })
}

fn storage_bgl_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Returns (`compute_iters`, `ring_reps`) tuned for the GPU vendor.
///
/// Different GPU architectures have different ALU-to-SFU ratios:
/// - Tested basic `AMD` and `Nvidia`, good results with 64, 4
/// - Others (Intel, etc.): Conservative defaults to avoid timeouts on
///   less capable or less tested hardware.
fn gpu_stress_tuning(vendor: u32) -> (u32, u32) {
    match vendor {
        PCI_VENDOR_NVIDIA | PCI_VENDOR_AMD => (64, 4),
        _ => (32, 2),
    }
}

/// Creates ring-buffer resources for memory-bandwidth GPU stress.
async fn create_gpu_stress_resources(adapter: wgpu::Adapter) -> Result<GpuStressResources> {
    let info = adapter.get_info();
    let label = format!("{} ({:?})", info.name, info.backend);

    let (compute_iters, ring_reps) = gpu_stress_tuning(info.vendor);
    eprintln!(
        "  Tuning: vendor 0x{:04X}, {compute_iters} iters, {ring_reps} ring reps",
        info.vendor
    );

    // Request the adapter's actual limits so large buffer bindings are allowed.
    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                required_limits: adapter.limits(),
                ..Default::default()
            },
            None,
        )
        .await
        .map_err(|e| anyhow!("Failed to create device for {label}: {e}"))?;

    // Scale buffer size by device type: discrete GPUs have dedicated VRAM,
    // integrated GPUs share system RAM. Clamp to the adapter's binding limit.
    let max_binding = u64::from(adapter.limits().max_storage_buffer_binding_size);
    let buf_bytes = match info.device_type {
        wgpu::DeviceType::DiscreteGpu => MEM_STRESS_BUF_DISCRETE.min(max_binding),
        _ => MEM_STRESS_BUF_INTEGRATED.min(max_binding),
    };
    // buf_bytes is at most 256 MiB, so buf_bytes/16 fits in u32.
    #[allow(clippy::cast_possible_truncation)]
    let vec4_count = (buf_bytes / 16) as u32;
    let total_mb = (buf_bytes * MEM_STRESS_RING_SIZE as u64) / (1024 * 1024);
    eprintln!("  VRAM stress: {total_mb} MiB ({MEM_STRESS_RING_SIZE} x {buf_bytes} B)");

    let mut buffers = Vec::with_capacity(MEM_STRESS_RING_SIZE);
    for i in 0..MEM_STRESS_RING_SIZE {
        buffers.push(create_storage_buffer(
            &device,
            &format!("ring_{i}"),
            buf_bytes,
        ));
    }

    let total_groups = vec4_count.div_ceil(MEM_STRESS_WORKGROUP);
    let dispatch_x = total_groups.min(MAX_WORKGROUPS_PER_DIM);
    let dispatch_y = total_groups.div_ceil(dispatch_x);
    let stride = dispatch_x * MEM_STRESS_WORKGROUP;

    let (pipeline, bind_groups) =
        create_stress_pipeline_and_bindings(&device, &buffers, vec4_count, stride, compute_iters);

    Ok(GpuStressResources {
        device,
        queue,
        pipeline,
        bind_groups,
        dispatch_x,
        dispatch_y,
        ring_reps,
        _buffers: buffers,
    })
}

/// Creates the compute pipeline and ring bind groups for the streaming
/// shader.
///
/// The WGSL shader runs N iterations per element using four different
/// transcendental functions (sin, cos, exp, sqrt) all driven by the
/// loop index, plus FMA and division on the accumulator. This engages
/// both ALU (FMA, division) and all Special Function Unit hardware
/// (sin, cos, exp, sqrt) simultaneously for maximum power draw.
///
/// The loop index drives all transcendentals (not the accumulator).
/// Ring bind groups rotate src/dst so each submission reads what the
/// previous one wrote, keeping the data "hot" in VRAM.
fn create_stress_pipeline_and_bindings(
    device: &wgpu::Device,
    buffers: &[wgpu::Buffer],
    vec4_count: u32,
    stride: u32,
    compute_iters: u32,
) -> (wgpu::ComputePipeline, Vec<wgpu::BindGroup>) {
    let shader_src = format!(
        r"
@group(0) @binding(0) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;

const N: u32 = {vec4_count}u;
const STRIDE: u32 = {stride}u;
const ITERS: u32 = {compute_iters}u;

@compute @workgroup_size({MEM_STRESS_WORKGROUP})
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {{
    let idx = gid.y * STRIDE + gid.x;
    if idx >= N {{ return; }}
    var a = src[idx] + vec4<f32>(0.01);
    for (var i: u32 = 0u; i < ITERS; i = i + 1u) {{
        // Loop index drives all transcendentals -- deterministic inputs
        // ensure every iteration does real work with no degenerate
        // convergence. Using sin+cos+exp+sqrt engages all SFU hardware.
        let f = f32(i) + 1.0;
        let sv = sin(vec4<f32>(f));
        let cv = cos(vec4<f32>(f));
        let ev = exp(vec4<f32>(f * 0.1));
        let sq = sqrt(vec4<f32>(f));
        // FMA exercises ALU multiply-add units.
        a = fma(a, sv, cv);
        // Division is ~4x the cost of multiply on most GPUs.
        // clamp keeps divisor bounded and positive.
        a = a / clamp(ev * sq, vec4<f32>(0.1), vec4<f32>(0.9));
    }}
    // clamp prevents overflow across ring passes.
    dst[idx] = clamp(a, vec4<f32>(-1.0), vec4<f32>(1.0));
}}
"
    );

    let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("mem_stress_shader"),
        source: wgpu::ShaderSource::Wgsl(shader_src.into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("mem_stress_bgl"),
        entries: &[storage_bgl_entry(0, true), storage_bgl_entry(1, false)],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("mem_stress_pipeline_layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("mem_stress_pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader_module,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    // Ring bind groups: each reads buf[i], writes buf[(i+1) % N].
    let ring_size = buffers.len();
    let mut bind_groups = Vec::with_capacity(ring_size);
    for i in 0..ring_size {
        let dst_idx = (i + 1) % ring_size;
        bind_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("ring_bg_{i}")),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffers[i].as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffers[dst_idx].as_entire_binding(),
                },
            ],
        }));
    }

    (pipeline, bind_groups)
}

/// Main GPU stress loop: submits ring traversals and syncs with
/// `device.poll(Wait)` after each. Ring reps per submission are tuned
/// per GPU vendor (see `gpu_stress_tuning`). The tight while loop
/// keeps the GPU continuously busy by submitting the next batch
/// immediately after the previous one completes.
async fn stress_adapter(adapter: wgpu::Adapter, duration: Duration) -> Result<()> {
    let res = create_gpu_stress_resources(adapter).await?;
    let deadline = Instant::now() + duration;

    while Instant::now() < deadline {
        let mut encoder = res
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        // Multiple ring traversals per submission increase sustained load.
        // Each ring step needs its own compute pass to avoid conflicting
        // buffer usage (a buffer is read-only src in one step and
        // read-write dst in the adjacent step).
        for _ in 0..res.ring_reps {
            for bg in &res.bind_groups {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: None,
                    timestamp_writes: None,
                });
                pass.set_pipeline(&res.pipeline);
                pass.set_bind_group(0, bg, &[]);
                pass.dispatch_workgroups(res.dispatch_x, res.dispatch_y, 1);
            }
        }
        res.queue.submit(std::iter::once(encoder.finish()));
        res.device.poll(wgpu::Maintain::Wait);
        tokio::time::sleep(Duration::from_millis(GPU_SUBMIT_SLEEP_MS)).await;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_stress_runs_briefly() {
        // Just verify it doesn't panic with a 1-second run
        run_cpu_stress(Some(1), 1).unwrap();
    }

    #[test]
    fn online_cpu_count_returns_at_least_one() {
        assert!(online_cpu_count() >= 1);
    }

    #[test]
    fn reset_cpu_affinity_succeeds() {
        reset_cpu_affinity().unwrap();
    }

    #[test]
    fn aligned_buffer_is_32_byte_aligned() {
        // AlignedBuffer is used by RAM stress. Verify SIMD alignment.
        let buf = AlignedBuffer::new(1024);
        assert_eq!(buf.ptr as usize % SIMD_ALIGN, 0);
        assert_eq!(buf.len, 1024);
    }

    #[test]
    fn cpu_stress_matrixprod_runs_briefly() {
        // Verify the matrix product loop runs and terminates on deadline.
        let deadline = Instant::now() + Duration::from_millis(200);
        cpu_stress_matrixprod(deadline);
    }

    #[test]
    fn cpu_stress_matrixprod_produces_nonzero_result() {
        // Guard against the compiler optimizing away the computation.
        // A single matrix multiply of positive inputs must produce
        // positive, non-zero results.
        let mut a = vec![0.0_f64; MAT_ELEMENTS];
        let mut b = vec![0.0_f64; MAT_ELEMENTS];
        let mut c = vec![0.0_f64; MAT_ELEMENTS];
        for i in 0..MAT_ELEMENTS {
            #[allow(clippy::cast_precision_loss)]
            let v = 1.0 + (i as f64) * 1e-10;
            a[i] = v;
            b[i] = v;
        }
        for i in 0..MAT_SIZE {
            for j in 0..MAT_SIZE {
                let mut sum = 0.0_f64;
                for k in 0..MAT_SIZE {
                    sum = a[i * MAT_SIZE + k].mul_add(b[k * MAT_SIZE + j], sum);
                }
                c[i * MAT_SIZE + j] = sum;
            }
        }
        // Every element of C should be positive and finite.
        assert!(c.iter().all(|v| v.is_finite() && *v > 0.0));
    }

    #[test]
    fn ram_stress_runs_briefly() {
        run_ram_stress(4 * 1024 * 1024, 1).unwrap();
    }

    #[test]
    fn available_memory_returns_positive() {
        assert!(available_memory_bytes().unwrap() > 0);
    }

    #[test]
    fn direct_io_buffer_is_page_aligned() {
        let buf = DirectIoBuffer::new(DRIVE_STRESS_BLOCK_SIZE);
        assert_eq!(buf.ptr as usize % DRIVE_STRESS_ALIGNMENT, 0);
        assert_eq!(buf.layout.size(), DRIVE_STRESS_BLOCK_SIZE);
    }

    #[test]
    fn xorshift64_produces_varied_offsets() {
        let mut rng = XorShift64::new(42);
        let mut values = std::collections::HashSet::new();
        for _ in 0..1000 {
            values.insert(rng.next());
        }
        // With 1000 iterations, we should get at least 990 unique values.
        assert!(values.len() > 990, "PRNG produced too few unique values");
    }

    #[test]
    fn drive_stress_rejects_non_block_device() {
        let result = run_drive_stress("/dev/null", 1, 1);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not a block device"));
    }

    #[test]
    fn drive_stress_rejects_nonexistent_device() {
        let result = run_drive_stress("/dev/nonexistent_drive_xyz", 1, 1);
        assert!(result.is_err());
    }
}
