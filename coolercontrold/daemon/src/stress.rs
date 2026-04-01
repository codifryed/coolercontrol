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

/// Matrix dimension for GPU stress test (N x N).
const MATRIX_DIM: u32 = 1024;

/// Number of compute dispatches per GPU submission for sustained saturation.
const DISPATCHES_PER_SUBMISSION: u32 = 8;

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

/// Run GPU stress test using wgpu compute shader (matrix multiplication).
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

struct GpuStressResources {
    device: wgpu::Device,
    queue: wgpu::Queue,
    compute_pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    dispatch_count: u32,
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

/// Creates the wgpu device, pipeline, and bind group for a single adapter.
async fn create_gpu_stress_resources(adapter: wgpu::Adapter) -> Result<GpuStressResources> {
    let info = adapter.get_info();
    let label = format!("{} ({:?})", info.name, info.backend);

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default(), None)
        .await
        .map_err(|e| anyhow!("Failed to create device for {label}: {e}"))?;
    let buffer_size = u64::from(MATRIX_DIM) * u64::from(MATRIX_DIM) * 4; // f32 = 4 bytes
    let buffer_a = create_storage_buffer(&device, "Matrix A", buffer_size);
    let buffer_b = create_storage_buffer(&device, "Matrix B", buffer_size);
    let buffer_c = create_storage_buffer(&device, "Matrix C", buffer_size);

    let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("stress_shader"),
        source: wgpu::ShaderSource::Wgsl(STRESS_SHADER_WGSL.into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("stress_bgl"),
        entries: &[
            storage_bgl_entry(0, true),
            storage_bgl_entry(1, true),
            storage_bgl_entry(2, false),
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("stress_pipeline_layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("stress_pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader_module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("stress_bind_group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer_a.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: buffer_b.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: buffer_c.as_entire_binding(),
            },
        ],
    });
    let dispatch_count = MATRIX_DIM.div_ceil(16);

    Ok(GpuStressResources {
        device,
        queue,
        compute_pipeline,
        bind_group,
        dispatch_count,
    })
}

/// Submits compute dispatches in a tight loop until the deadline.
async fn stress_adapter(adapter: wgpu::Adapter, duration: Duration) -> Result<()> {
    let res = create_gpu_stress_resources(adapter).await?;
    let deadline = Instant::now() + duration;

    while Instant::now() < deadline {
        let mut encoder = res
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&res.compute_pipeline);
            pass.set_bind_group(0, &res.bind_group, &[]);
            for _ in 0..DISPATCHES_PER_SUBMISSION {
                pass.dispatch_workgroups(res.dispatch_count, res.dispatch_count, 1);
            }
        }
        res.queue.submit(std::iter::once(encoder.finish()));
        res.device.poll(wgpu::Maintain::Wait);
        // Brief pause to leave a small gap for desktop compositing.
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    Ok(())
}

const STRESS_SHADER_WGSL: &str = r"
@group(0) @binding(0) var<storage, read> a: array<f32>;
@group(0) @binding(1) var<storage, read> b: array<f32>;
@group(0) @binding(2) var<storage, read_write> c: array<f32>;

const N: u32 = 1024u;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let row = gid.x;
    let col = gid.y;
    if row >= N || col >= N {
        return;
    }
    var sum: f32 = 0.0;
    for (var k: u32 = 0u; k < N; k = k + 1u) {
        sum = fma(a[row * N + k], b[k * N + col], sum);
    }
    c[row * N + col] = sum;
}
";

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
}
