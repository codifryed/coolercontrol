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

use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

const NICE_LEVEL: i32 = 19;

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
            content
                .lines()
                .filter(|line| line.starts_with("processor"))
                .count() as u16
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

/// Run CPU stress test with tight f64 FMA loops on N threads.
pub fn run_cpu_stress(threads: Option<u16>, timeout_secs: u16) -> Result<()> {
    reset_cpu_affinity()?;
    set_nice_level()?;

    let num_threads = threads.unwrap_or_else(online_cpu_count);
    eprintln!("CPU stress: {num_threads} threads, {timeout_secs}s (pid: {})", std::process::id());

    let duration = Duration::from_secs(u64::from(timeout_secs));
    let deadline = Instant::now() + duration;
    let mut handles = Vec::with_capacity(num_threads as usize);

    for _ in 0..num_threads {
        handles.push(std::thread::spawn(move || {
            let mut a: f64 = 1.0;
            let mut b: f64 = 2.0;
            let mut c: f64 = 3.0;
            while Instant::now() < deadline {
                for _ in 0..10_000 {
                    a = a.mul_add(b, c);
                    b = b.mul_add(c, a);
                    c = c.mul_add(a, b);
                }
            }
            std::hint::black_box((a, b, c));
        }));
    }

    for handle in handles {
        let _ = handle.join();
    }

    Ok(())
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

async fn stress_adapter(adapter: wgpu::Adapter, duration: Duration) -> Result<()> {
    let info = adapter.get_info();
    let label = format!("{} ({:?})", info.name, info.backend);

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default(), None)
        .await
        .map_err(|e| anyhow!("Failed to create device for {label}: {e}"))?;

    let buffer_size = u64::from(MATRIX_DIM) * u64::from(MATRIX_DIM) * 4; // f32 = 4 bytes

    let buffer_a = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Matrix A"),
        size: buffer_size,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let buffer_b = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Matrix B"),
        size: buffer_size,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let buffer_c = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Matrix C"),
        size: buffer_size,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });

    let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("stress_shader"),
        source: wgpu::ShaderSource::Wgsl(STRESS_SHADER_WGSL.into()),
    });

    let bgl_entry = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("stress_bgl"),
        entries: &[bgl_entry(0, true), bgl_entry(1, true), bgl_entry(2, false)],
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

    let workgroup_size = 16u32;
    let dispatch_count = MATRIX_DIM.div_ceil(workgroup_size);

    let deadline = Instant::now() + duration;

    while Instant::now() < deadline {
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&compute_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            for _ in 0..DISPATCHES_PER_SUBMISSION {
                pass.dispatch_workgroups(dispatch_count, dispatch_count, 1);
            }
        }
        queue.submit(std::iter::once(encoder.finish()));
        device.poll(wgpu::Maintain::Wait);
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
}
