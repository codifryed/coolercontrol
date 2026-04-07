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

// NVIDIA nvapi interface on Linux (undocumented).
// Based on reverse engineering work from the LACT project:
// https://github.com/ilya-zlobintsev/LACT (MIT License, Copyright 2023 Ilya Zlobintsev)

use std::collections::HashMap;
use std::ffi::{c_char, CStr};
use std::mem;
use std::ptr;

use log::{debug, info, warn};

const LIBRARY_NAME: &str = "libnvidia-api.so.1";
const QUERY_INTERFACE_FN: &[u8] = b"nvapi_QueryInterface\0";

#[allow(clippy::unreadable_literal)] // nvapi hash IDs are opaque identifiers.
mod query_ids {
    pub const INITIALIZE: u32 = 0x0150e828;
    pub const UNLOAD: u32 = 0xd22bdd7e;
    pub const ENUM_PHYSICAL_GPUS: u32 = 0xe5ac921f;
    pub const GET_BUS_ID: u32 = 0x1be0b8e5;
    pub const GET_THERMALS: u32 = 0x65fe3aad;
    pub const GET_ERROR_MESSAGE: u32 = 0x6c2d048c;
}

const MAX_PHYSICAL_GPUS: usize = 64;
const SHORT_STRING_MAX: usize = 64;
const THERMALS_VALUE_COUNT: usize = 40;
const HOTSPOT_INDEX: usize = 9;
const THERMALS_SCALE_FACTOR: i32 = 256;
/// Valid temperature range for filtering (exclusive bounds).
const VALID_TEMP_RANGE: std::ops::Range<i32> = 1..255;

// Compile-time invariant checks.
const _: () = assert!(HOTSPOT_INDEX < THERMALS_VALUE_COUNT);
const _: () = assert!(MAX_PHYSICAL_GPUS <= u32::MAX as usize);

type NvHandle = *mut core::ffi::c_void;
type NvStatus = i32;
type QueryInterfaceFn = unsafe extern "C" fn(u32) -> *const ();
type ThermalsFn = unsafe extern "C" fn(NvHandle, *mut NvApiThermals) -> NvStatus;

#[repr(C)]
struct NvApiThermals {
    version: u32,
    mask: i32,
    values: [i32; THERMALS_VALUE_COUNT],
}

impl NvApiThermals {
    fn new(mask: i32) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        let version = (size_of::<Self>() | (2 << 16)) as u32;
        debug_assert!(size_of::<Self>() < u16::MAX as usize); // Version encodes size in lower 16 bits.
        Self {
            version,
            mask,
            values: [0; THERMALS_VALUE_COUNT],
        }
    }

    fn hotspot(&self) -> Option<f64> {
        let raw_value = self.values[HOTSPOT_INDEX];
        let temp_celsius = raw_value / THERMALS_SCALE_FACTOR;
        if VALID_TEMP_RANGE.contains(&temp_celsius) {
            return Some(f64::from(temp_celsius));
        }
        None
    }
}

struct GpuEntry {
    handle: NvHandle,
    thermals_mask: i32,
}

pub struct NvApi {
    _lib: libloading::Library,
    query_interface: QueryInterfaceFn,
    gpu_entries: HashMap<u32, GpuEntry>,
}

impl NvApi {
    /// Attempts to load and initialize the nvapi library.
    /// Returns None if the library is not available or initialization fails.
    pub fn try_init() -> Option<Self> {
        let lib = load_library()?;
        let query_interface = load_query_interface(&lib)?;
        initialize_api(query_interface)?;
        let gpu_entries = enumerate_gpu_entries(query_interface)?;
        info!("nvapi initialized with {} GPU(s)", gpu_entries.len());
        Some(Self {
            _lib: lib,
            query_interface,
            gpu_entries,
        })
    }

    /// Returns the hotspot/junction temperature for the GPU at the given PCI bus ID.
    pub fn get_hotspot_temp(&self, pci_bus: u32) -> Option<f64> {
        let entry = self.gpu_entries.get(&pci_bus)?;
        let thermals_fn =
            query_fn::<ThermalsFn>(self.query_interface, query_ids::GET_THERMALS, "GetThermals")?;
        let mut thermals = NvApiThermals::new(entry.thermals_mask);
        // SAFETY: thermals_fn is a validated nvapi function pointer. The NvApiThermals
        // struct is #[repr(C)] with correct version encoding, matching the nvapi ABI.
        let status = unsafe { thermals_fn(entry.handle, &raw mut thermals) };
        if status != 0 {
            debug!(
                "nvapi GetThermals failed: {}",
                error_message(self.query_interface, status)
            );
            return None;
        }
        thermals.hotspot()
    }
}

impl Drop for NvApi {
    fn drop(&mut self) {
        if let Some(unload_fn) = query_fn::<unsafe extern "C" fn() -> NvStatus>(
            self.query_interface,
            query_ids::UNLOAD,
            "Unload",
        ) {
            // SAFETY: Unload is called once during drop to release nvapi resources.
            unsafe {
                unload_fn();
            }
        }
    }
}

/// Loads the nvapi shared library.
fn load_library() -> Option<libloading::Library> {
    // SAFETY: Loading a shared library is inherently unsafe. The library name is a
    // known NVIDIA driver component; the OS linker resolves and loads it.
    unsafe { libloading::Library::new(LIBRARY_NAME) }
        .inspect_err(|err| debug!("nvapi library not available: {err}"))
        .ok()
}

/// Resolves the `nvapi_QueryInterface` entry point from the loaded library.
fn load_query_interface(lib: &libloading::Library) -> Option<QueryInterfaceFn> {
    // SAFETY: QUERY_INTERFACE_FN is a null-terminated symbol name. The returned
    // function pointer is valid for the lifetime of the library.
    unsafe {
        let sym = lib
            .get::<QueryInterfaceFn>(QUERY_INTERFACE_FN)
            .inspect_err(|err| warn!("nvapi QueryInterface symbol not found: {err}"))
            .ok()?;
        Some(*sym)
    }
}

/// Calls nvapi Initialize. Returns None on failure.
fn initialize_api(query_interface: QueryInterfaceFn) -> Option<()> {
    let init_fn = query_fn::<unsafe extern "C" fn() -> NvStatus>(
        query_interface,
        query_ids::INITIALIZE,
        "Initialize",
    )?;
    // SAFETY: init_fn is a validated nvapi function pointer for the Initialize call.
    let status = unsafe { init_fn() };
    if status != 0 {
        warn!(
            "nvapi Initialize failed: {}",
            error_message(query_interface, status)
        );
        return None;
    }
    Some(())
}

/// Enumerates GPUs and builds the bus ID to GPU entry map.
/// Returns None if no GPUs support thermal queries.
fn enumerate_gpu_entries(query_interface: QueryInterfaceFn) -> Option<HashMap<u32, GpuEntry>> {
    let enum_fn =
        query_fn::<unsafe extern "C" fn(*mut [NvHandle; MAX_PHYSICAL_GPUS], *mut u32) -> NvStatus>(
            query_interface,
            query_ids::ENUM_PHYSICAL_GPUS,
            "EnumPhysicalGPUs",
        )?;
    let bus_id_fn = query_fn::<unsafe extern "C" fn(NvHandle, *mut u32) -> NvStatus>(
        query_interface,
        query_ids::GET_BUS_ID,
        "GetBusId",
    )?;
    let thermals_fn =
        query_fn::<ThermalsFn>(query_interface, query_ids::GET_THERMALS, "GetThermals")?;

    let mut handles = [ptr::null_mut(); MAX_PHYSICAL_GPUS];
    let mut count: u32 = 0;
    // SAFETY: enum_fn writes into the fixed-size handles array and count.
    // Both are stack-allocated with known sizes matching the nvapi ABI.
    let status = unsafe { enum_fn(&raw mut handles, &raw mut count) };
    if status != 0 {
        warn!(
            "nvapi EnumPhysicalGPUs failed: {}",
            error_message(query_interface, status)
        );
        return None;
    }

    // Clamp count to array bounds (defensive against driver bugs).
    let gpu_count = (count as usize).min(MAX_PHYSICAL_GPUS);
    let mut gpu_entries = HashMap::with_capacity(gpu_count);
    for handle in handles.iter().take(gpu_count) {
        let mut bus_id: u32 = 0;
        // SAFETY: bus_id_fn writes a u32 bus ID through the provided pointer.
        let status = unsafe { bus_id_fn(*handle, &raw mut bus_id) };
        if status != 0 {
            debug!(
                "nvapi GetBusId failed for a GPU: {}",
                error_message(query_interface, status)
            );
            continue;
        }
        match calculate_thermals_mask(query_interface, thermals_fn, *handle) {
            Some(mask) => {
                gpu_entries.insert(
                    bus_id,
                    GpuEntry {
                        handle: *handle,
                        thermals_mask: mask,
                    },
                );
            }
            None => {
                debug!("nvapi GPU at bus {bus_id} does not support thermal queries");
            }
        }
    }

    if gpu_entries.is_empty() {
        info!("nvapi: no GPUs support hotspot temperature monitoring");
        return None;
    }
    Some(gpu_entries)
}

/// Calculates the valid thermals mask for a GPU by probing bit positions.
fn calculate_thermals_mask(
    query_interface: QueryInterfaceFn,
    thermals_fn: ThermalsFn,
    handle: NvHandle,
) -> Option<i32> {
    // First verify thermals work at all with mask=1.
    let mut thermals = NvApiThermals::new(1);
    // SAFETY: thermals_fn writes into the #[repr(C)] NvApiThermals struct
    // which has correct version encoding and is stack-allocated.
    let status = unsafe { thermals_fn(handle, &raw mut thermals) };
    if status != 0 {
        debug!(
            "nvapi thermals not supported for GPU: {}",
            error_message(query_interface, status)
        );
        return None;
    }

    // Probe each bit to find the highest supported mask.
    for bit in 0..i32::BITS {
        thermals.mask = 1 << bit;
        thermals.values = [0; THERMALS_VALUE_COUNT];
        // SAFETY: Same as above; probing with different mask values.
        let status = unsafe { thermals_fn(handle, &raw mut thermals) };
        if status != 0 {
            let mask = (1 << bit) - 1;
            debug!("nvapi calculated thermals mask: {mask:#x}");
            return Some(mask);
        }
    }

    // All 32 bits succeeded (unlikely but valid).
    let mask = i32::MAX;
    debug!("nvapi calculated thermals mask: {mask:#x} (all bits)");
    Some(mask)
}

/// Resolves a function pointer from `nvapi_QueryInterface` by hash ID.
fn query_fn<F>(query_interface: QueryInterfaceFn, id: u32, name: &str) -> Option<F> {
    // SAFETY: query_interface is a validated symbol from the nvapi library.
    // The returned pointer is transmuted to the caller-specified function type,
    // which must match the nvapi ABI for the given query ID.
    let ptr = unsafe { query_interface(id) };
    if ptr.is_null() {
        warn!("nvapi {name} function not found (query {id:#x})");
        return None;
    }
    Some(unsafe { mem::transmute_copy(&ptr) })
}

/// Gets a human-readable error message from the nvapi driver.
fn error_message(query_interface: QueryInterfaceFn, status: NvStatus) -> String {
    let Some(error_fn) =
        query_fn::<unsafe extern "C" fn(NvStatus, *mut [c_char; SHORT_STRING_MAX]) -> NvStatus>(
            query_interface,
            query_ids::GET_ERROR_MESSAGE,
            "GetErrorMessage",
        )
    else {
        return format!("status {status:#x}");
    };

    let mut error_text = [0 as c_char; SHORT_STRING_MAX];
    // SAFETY: error_fn writes a null-terminated C string into the fixed-size buffer.
    let result = unsafe { error_fn(status, &raw mut error_text) };
    if result != 0 {
        return format!("status {status:#x}");
    }

    // SAFETY: The buffer was zero-initialized and written by nvapi with a
    // null-terminated string. from_ptr reads until the first null byte.
    let msg = unsafe { CStr::from_ptr(error_text.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    format!("{msg} ({status:#x})")
}
