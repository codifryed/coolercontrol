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

use std::alloc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

const NICE_LEVEL: i32 = 19;

/// Per-thread working buffer size. Exceeds typical L2 caches, forcing L3 and
/// DRAM traffic across threads for maximum heat generation.
const STRESS_BUF_BYTES: usize = 4 * 1024 * 1024;
const STRESS_BUF_F64S: usize = STRESS_BUF_BYTES / size_of::<f64>();
const AVX2_ALIGN: usize = 32;
const F64S_PER_AVX2: usize = 4;
/// Full buffer sweeps between deadline checks. Higher values reduce the
/// overhead of clock_gettime syscalls and keep CPU utilization closer to 100%.
const CACHE_SWEEPS_PER_CHECK: u32 = 16;

/// Fraction of MemAvailable to allocate for RAM stress.
pub const RAM_STRESS_ALLOC_FRACTION: f64 = 0.8;

/// Per-buffer size for discrete GPUs (256 MiB, total ring = 2 GiB).
const MEM_STRESS_BUF_DISCRETE: u64 = 256 * 1024 * 1024;
/// Per-buffer size for integrated GPUs (16 MiB, total ring = 128 MiB).
const MEM_STRESS_BUF_INTEGRATED: u64 = 16 * 1024 * 1024;
/// Number of buffers forming the ring.
const MEM_STRESS_RING_SIZE: usize = 8;
/// Workgroup size for the streaming shader.
const MEM_STRESS_WORKGROUP: u32 = 256;
/// Compute iterations per element in the streaming shader. Higher values
/// increase ALU utilization and core heat alongside memory stress.
const MEM_STRESS_COMPUTE_ITERS: u32 = 64;
/// Full ring traversals per GPU submission.
const MEM_STRESS_RING_REPS: u32 = 4;
/// Sleep between GPU submissions for desktop compositing (ms).
const GPU_SUBMIT_SLEEP_MS: u64 = 0;

/// Returns the number of logical processors by counting entries in /proc/cpuinfo.
/// This is not restricted by CPU affinity or cgroup limits
/// (unlike `available_parallelism`).
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

fn set_nice_level() -> Result<()> {
    // SAFETY: nice() is always safe to call; it only adjusts scheduling priority.
    let result = unsafe { nix::libc::nice(NICE_LEVEL) };
    if result == -1 {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() != Some(0) {
            return Err(anyhow!("Failed to set nice level: {err}"));
        }
    }
    Ok(())
}

/// 32-byte-aligned heap buffer for SIMD-friendly cache-stressing workloads.
struct AlignedBuffer {
    ptr: *mut f64,
    layout: alloc::Layout,
    len: usize,
}

impl AlignedBuffer {
    fn new(count: usize) -> Self {
        let size = count * size_of::<f64>();
        let layout = alloc::Layout::from_size_align(size, AVX2_ALIGN).expect("valid layout");
        // SAFETY: Layout is valid and non-zero.
        let ptr = unsafe { alloc::alloc(layout) };
        if ptr.is_null() {
            alloc::handle_alloc_error(layout);
        }
        let ptr = ptr.cast::<f64>();
        // Initialize to small positive values to avoid denormals.
        for i in 0..count {
            // SAFETY: ptr has count elements allocated.
            unsafe { ptr.add(i).write(1.0 + (i as f64) * 1e-10) };
        }
        Self {
            ptr,
            layout,
            len: count,
        }
    }

    fn as_mut_ptr(&mut self) -> *mut f64 {
        self.ptr
    }
}

impl Drop for AlignedBuffer {
    fn drop(&mut self) {
        // SAFETY: ptr was allocated with this layout in new().
        unsafe { alloc::dealloc(self.ptr.cast(), self.layout) };
    }
}

/// Run CPU stress test with cache-busting FMA+sqrt loops on N threads.
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
            let mut buf = AlignedBuffer::new(STRESS_BUF_F64S);
            #[cfg(target_arch = "x86_64")]
            if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
                // SAFETY: AVX2+FMA support confirmed by runtime feature detection.
                unsafe { stress_loop_avx2_fma(buf.as_mut_ptr(), buf.len, deadline) };
                return;
            }
            stress_loop_scalar(buf.as_mut_ptr(), buf.len, deadline);
        }));
    }

    for handle in handles {
        let _ = handle.join();
    }

    Ok(())
}

/// Scalar fallback: sweeps buffer with FMA + interleaved sqrt for cache and
/// FP unit stress. Used on non-x86_64 or CPUs without AVX2+FMA.
fn stress_loop_scalar(buf: *mut f64, len: usize, deadline: Instant) {
    let multiplier = 1.000_000_001_f64;
    let addend = 0.999_999_999_f64;
    while Instant::now() < deadline {
        for _ in 0..CACHE_SWEEPS_PER_CHECK {
            for i in 0..len {
                // SAFETY: buf has len elements, all indices in bounds.
                unsafe {
                    let ptr = buf.add(i);
                    let mut v = ptr.read();
                    v = v.mul_add(multiplier, addend);
                    if i % 2 == 0 {
                        v = v.sqrt();
                    }
                    v = v.clamp(0.1, 1e100);
                    ptr.write(v);
                }
            }
        }
    }
    std::hint::black_box(unsafe { buf.read() });
}

/// AVX2+FMA hot loop: 4-wide f64 FMA with interleaved sqrt and integer ops.
/// Engages SIMD FP units, divider, integer ports, and cache hierarchy.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn stress_loop_avx2_fma(buf: *mut f64, len: usize, deadline: Instant) {
    use std::arch::x86_64::{
        __m256d, __m256i, _mm256_add_epi32, _mm256_fmadd_pd, _mm256_load_pd, _mm256_max_pd,
        _mm256_min_pd, _mm256_mullo_epi32, _mm256_set1_pd, _mm256_set_epi32, _mm256_sqrt_pd,
        _mm256_store_pd,
    };

    let vmul: __m256d = _mm256_set1_pd(1.000_000_001);
    let vadd: __m256d = _mm256_set1_pd(0.999_999_999);
    let vclamp_lo: __m256d = _mm256_set1_pd(0.1);
    let vclamp_hi: __m256d = _mm256_set1_pd(1e100);

    // Integer accumulators to engage integer execution ports.
    let mut iacc: __m256i = _mm256_set_epi32(1, 2, 3, 4, 5, 6, 7, 8);
    let imul: __m256i = _mm256_set_epi32(7, 11, 13, 17, 19, 23, 29, 31);

    let chunks = len / F64S_PER_AVX2;
    while Instant::now() < deadline {
        for _ in 0..CACHE_SWEEPS_PER_CHECK {
            for i in 0..chunks {
                // SAFETY: buf is 32-byte aligned with len elements.
                let ptr = unsafe { buf.add(i * F64S_PER_AVX2) };
                let mut v = unsafe { _mm256_load_pd(ptr) };

                v = _mm256_fmadd_pd(v, vmul, vadd);

                // Sqrt on even chunks engages the divider unit (VSQRTPD,
                // 15-20 cycle latency). Alternating lets OOO overlap FMA
                // and sqrt on independent data.
                if i % 2 == 0 {
                    v = _mm256_sqrt_pd(v);
                }

                v = _mm256_max_pd(v, vclamp_lo);
                v = _mm256_min_pd(v, vclamp_hi);
                // SAFETY: ptr is aligned and in-bounds.
                unsafe { _mm256_store_pd(ptr, v) };

                // Integer multiply every 8 chunks to keep integer ports
                // active without starving FP throughput.
                if i % 8 == 0 {
                    iacc = _mm256_mullo_epi32(iacc, imul);
                    iacc = _mm256_add_epi32(iacc, imul);
                }
            }
        }
    }
    std::hint::black_box(iacc);
}

/// Run RAM stress test by streaming read-modify-write across a large allocation.
/// Stresses DIMMs and the CPU's memory controller for maximum memory heat.
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
            ram_stress_loop_scalar(buf.as_mut_ptr(), buf.len, deadline);
        }));
    }

    for handle in handles {
        let _ = handle.join();
    }

    Ok(())
}

/// Scalar RAM stress: unrolled sequential read-modify-write across buffer.
fn ram_stress_loop_scalar(buf: *mut f64, len: usize, deadline: Instant) {
    let multiplier = 1.000_000_000_1_f64;
    let addend = 0.999_999_999_9_f64;
    let unrolled = len / 4 * 4;
    while Instant::now() < deadline {
        // Unrolled sequential sweep for maximum hardware prefetcher utilization.
        let mut i = 0;
        while i < unrolled {
            // SAFETY: buf has len elements, all indices in bounds.
            unsafe {
                let p0 = buf.add(i);
                p0.write(p0.read().mul_add(multiplier, addend));
                let p1 = buf.add(i + 1);
                p1.write(p1.read().mul_add(multiplier, addend));
                let p2 = buf.add(i + 2);
                p2.write(p2.read().mul_add(multiplier, addend));
                let p3 = buf.add(i + 3);
                p3.write(p3.read().mul_add(multiplier, addend));
            }
            i += 4;
        }
        for j in unrolled..len {
            unsafe {
                let p = buf.add(j);
                p.write(p.read().mul_add(multiplier, addend));
            }
        }
    }
    std::hint::black_box(unsafe { buf.read() });
}

/// AVX2 RAM stress: unrolled sequential non-temporal stores bypass cache and
/// write directly to DRAM. Sequential access lets the hardware prefetcher
/// run at full speed for maximum memory bandwidth and heat.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
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
            // SAFETY: buf is 32-byte aligned with len elements.
            let p0 = unsafe { buf.add(off0) };
            let v0 = unsafe { _mm256_load_pd(p0) };
            unsafe { _mm256_stream_pd(p0, _mm256_fmadd_pd(v0, vmul, vadd)) };

            let p1 = unsafe { buf.add(off0 + F64S_PER_AVX2) };
            let v1 = unsafe { _mm256_load_pd(p1) };
            unsafe { _mm256_stream_pd(p1, _mm256_fmadd_pd(v1, vmul, vadd)) };

            let p2 = unsafe { buf.add(off0 + F64S_PER_AVX2 * 2) };
            let v2 = unsafe { _mm256_load_pd(p2) };
            unsafe { _mm256_stream_pd(p2, _mm256_fmadd_pd(v2, vmul, vadd)) };

            let p3 = unsafe { buf.add(off0 + F64S_PER_AVX2 * 3) };
            let v3 = unsafe { _mm256_load_pd(p3) };
            unsafe { _mm256_stream_pd(p3, _mm256_fmadd_pd(v3, vmul, vadd)) };

            i += 4;
        }
        // Handle remaining chunks.
        for j in unrolled..chunks {
            let p = unsafe { buf.add(j * F64S_PER_AVX2) };
            let v = unsafe { _mm256_load_pd(p) };
            unsafe { _mm256_stream_pd(p, _mm256_fmadd_pd(v, vmul, vadd)) };
        }
        _mm_sfence();
    }
}

/// Run GPU stress test using wgpu compute shaders for memory-bandwidth stress.
/// Enumerates all available GPU adapters and stresses them in parallel.
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
    let mut seen_devices = std::collections::HashSet::new();
    let mut unique_adapters = Vec::new();
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

const MAX_WORKGROUPS_PER_DIM: u32 = 65535;

struct GpuStressResources {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_groups: Vec<wgpu::BindGroup>,
    dispatch_x: u32,
    dispatch_y: u32,
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

/// Creates ring-buffer resources for memory-bandwidth GPU stress.
async fn create_gpu_stress_resources(adapter: wgpu::Adapter) -> Result<GpuStressResources> {
    let info = adapter.get_info();
    let label = format!("{} ({:?})", info.name, info.backend);

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

    // Split dispatch across X and Y dimensions to stay within the
    // 65535 per-dimension workgroup limit.
    let total_groups = vec4_count.div_ceil(MEM_STRESS_WORKGROUP);
    let dispatch_x = total_groups.min(MAX_WORKGROUPS_PER_DIM);
    let dispatch_y = total_groups.div_ceil(dispatch_x);
    let stride = dispatch_x * MEM_STRESS_WORKGROUP;

    let shader_src = format!(
        r"
@group(0) @binding(0) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;

const N: u32 = {vec4_count}u;
const STRIDE: u32 = {stride}u;
const ITERS: u32 = {MEM_STRESS_COMPUTE_ITERS}u;

@compute @workgroup_size({MEM_STRESS_WORKGROUP})
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {{
    let idx = gid.y * STRIDE + gid.x;
    if idx >= N {{ return; }}
    var v = src[idx];
    var acc = dst[idx];
    for (var i: u32 = 0u; i < ITERS; i = i + 1u) {{
        acc = fma(v, acc, v);
        v = sqrt(abs(acc));
    }}
    dst[idx] = acc;
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
        compilation_options: Default::default(),
        cache: None,
    });

    // Ring bind groups: each reads buf[i], writes buf[(i+1) % N].
    let mut bind_groups = Vec::with_capacity(MEM_STRESS_RING_SIZE);
    for i in 0..MEM_STRESS_RING_SIZE {
        let src_idx = i;
        let dst_idx = (i + 1) % MEM_STRESS_RING_SIZE;
        bind_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("ring_bg_{i}")),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffers[src_idx].as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffers[dst_idx].as_entire_binding(),
                },
            ],
        }));
    }

    Ok(GpuStressResources {
        device,
        queue,
        pipeline,
        bind_groups,
        dispatch_x,
        dispatch_y,
        _buffers: buffers,
    })
}

/// Streams data through ring buffers until deadline, stressing GPU memory.
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
        for _ in 0..MEM_STRESS_RING_REPS {
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
        let buf = AlignedBuffer::new(STRESS_BUF_F64S);
        assert_eq!(buf.ptr as usize % AVX2_ALIGN, 0);
        assert_eq!(buf.len, STRESS_BUF_F64S);
    }

    #[test]
    fn scalar_stress_loop_runs_briefly() {
        let mut buf = AlignedBuffer::new(1024);
        let deadline = Instant::now() + Duration::from_millis(100);
        stress_loop_scalar(buf.as_mut_ptr(), buf.len, deadline);
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn avx2_stress_loop_runs_if_supported() {
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            let mut buf = AlignedBuffer::new(STRESS_BUF_F64S);
            let deadline = Instant::now() + Duration::from_millis(200);
            // SAFETY: AVX2+FMA support confirmed above.
            unsafe { stress_loop_avx2_fma(buf.as_mut_ptr(), buf.len, deadline) };
        }
    }

    #[test]
    fn ram_stress_runs_briefly() {
        run_ram_stress(4 * 1024 * 1024, 1).unwrap();
    }

    #[test]
    fn available_memory_returns_positive() {
        assert!(available_memory_bytes().unwrap() > 0);
    }
}
