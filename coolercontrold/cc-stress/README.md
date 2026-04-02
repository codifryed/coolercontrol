# cc-stress

CPU, GPU, and RAM stress testing for thermal validation on Linux. Generates sustained thermal load to
help verify fan curves and cooling profiles.

This is not a benchmark. It is a convenience tool for heat generation, not performance measurement.

## What it does

Saturates hardware subsystems to produce maximum heat:

- **CPU** - cache-busting FMA + sqrt loops on all logical cores. Uses AVX2+FMA SIMD when available,
  scalar fallback otherwise. Each thread sweeps a 4 MiB buffer to stress L3 and DRAM.
- **GPU** - memory-bandwidth compute shader stress via wgpu (Vulkan or OpenGL ES). Streams data
  through a ring of storage buffers with interleaved FMA + sqrt to heat both VRAM and shader cores.
  All detected hardware GPUs are stressed in parallel.
- **RAM** - streaming read-modify-write across a large allocation (80% of available memory by
  default). Uses AVX2 non-temporal stores to bypass cache and write directly to DRAM, stressing DIMMs
  and the memory controller.

All stress functions run at nice 19 to avoid starving the desktop, and reset CPU affinity to all
online cores (overriding any systemd restrictions).

## Usage

### As a library

```rust
use cc_stress::{run_cpu_stress, run_gpu_stress, run_ram_stress};
use cc_stress::{online_cpu_count, available_memory_bytes, RAM_STRESS_ALLOC_FRACTION};

// CPU stress: all cores, 60 seconds
run_cpu_stress(None, 60).unwrap();

// CPU stress: 4 threads, 30 seconds
run_cpu_stress(Some(4), 30).unwrap();

// GPU stress: 60 seconds (async)
run_gpu_stress(60).await.unwrap();

// RAM stress: allocate 80% of available memory, 60 seconds
let alloc = (available_memory_bytes().unwrap() as f64 * RAM_STRESS_ALLOC_FRACTION) as u64;
run_ram_stress(alloc, 60).unwrap();
```

### Public API

| Function                  | Description                                          |
| ------------------------- | ---------------------------------------------------- |
| `run_cpu_stress`          | Spawn N threads running FMA+sqrt loops until timeout |
| `run_gpu_stress`          | Async. Stress all detected GPUs via wgpu compute     |
| `run_ram_stress`          | Streaming memory stress across a large allocation    |
| `online_cpu_count`        | Count logical CPUs from `/proc/cpuinfo`              |
| `available_memory_bytes`  | Read `MemAvailable` from `/proc/meminfo`             |
| `RAM_STRESS_ALLOC_FRACTION` | Default fraction of available memory to allocate (0.8) |

## Requirements

- Linux (reads `/proc/cpuinfo`, `/proc/meminfo`, uses `nix` for CPU affinity and nice)
- Vulkan or OpenGL ES drivers for GPU stress (optional, CPU and RAM work without them)
- Tokio runtime for `run_gpu_stress` (async)

## License

GPL-3.0-or-later - part of the [CoolerControl](https://gitlab.com/coolercontrol/coolercontrol)
project.
