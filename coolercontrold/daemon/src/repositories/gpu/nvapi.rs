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

#[allow(clippy::unreadable_literal)] // nvapi hash IDs are opaque identifiers
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
const HOTSPOT_INDEX: usize = 9;

type NvHandle = *mut core::ffi::c_void;
type NvStatus = i32;
type QueryInterfaceFn = unsafe extern "C" fn(u32) -> *const ();

#[repr(C)]
struct NvApiThermals {
    version: u32,
    mask: i32,
    values: [i32; 40],
}

impl NvApiThermals {
    fn new(mask: i32) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        Self {
            version: (size_of::<Self>() | (2 << 16)) as u32,
            mask,
            values: [0; 40],
        }
    }

    fn hotspot(&self) -> Option<f64> {
        self.values
            .get(HOTSPOT_INDEX)
            .map(|&v| v / 256)
            .filter(|&v| v > 0 && v < 255)
            .map(f64::from)
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
        let lib = unsafe { libloading::Library::new(LIBRARY_NAME) }
            .inspect_err(|err| debug!("nvapi library not available: {err}"))
            .ok()?;

        let query_interface: QueryInterfaceFn = unsafe {
            let sym = lib
                .get::<QueryInterfaceFn>(QUERY_INTERFACE_FN)
                .inspect_err(|err| warn!("nvapi QueryInterface symbol not found: {err}"))
                .ok()?;
            *sym
        };

        // Initialize
        let init_fn = query_fn::<unsafe extern "C" fn() -> NvStatus>(
            query_interface,
            query_ids::INITIALIZE,
            "Initialize",
        )?;
        let status = unsafe { init_fn() };
        if status != 0 {
            warn!(
                "nvapi Initialize failed: {}",
                error_message(query_interface, status)
            );
            return None;
        }

        // Enumerate GPUs
        let enum_fn = query_fn::<
            unsafe extern "C" fn(*mut [NvHandle; MAX_PHYSICAL_GPUS], *mut u32) -> NvStatus,
        >(
            query_interface,
            query_ids::ENUM_PHYSICAL_GPUS,
            "EnumPhysicalGPUs",
        )?;
        let bus_id_fn = query_fn::<unsafe extern "C" fn(NvHandle, *mut u32) -> NvStatus>(
            query_interface,
            query_ids::GET_BUS_ID,
            "GetBusId",
        )?;
        let thermals_fn = query_fn::<unsafe extern "C" fn(NvHandle, *mut NvApiThermals) -> NvStatus>(
            query_interface,
            query_ids::GET_THERMALS,
            "GetThermals",
        )?;

        let mut handles = [ptr::null_mut(); MAX_PHYSICAL_GPUS];
        let mut count: u32 = 0;
        let status = unsafe { enum_fn(&raw mut handles, &raw mut count) };
        if status != 0 {
            warn!(
                "nvapi EnumPhysicalGPUs failed: {}",
                error_message(query_interface, status)
            );
            return None;
        }

        // Build bus ID map with pre-calculated thermal masks
        let mut gpu_entries = HashMap::new();
        for handle in handles.iter().take(count as usize) {
            let mut bus_id: u32 = 0;
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
        let thermals_fn = query_fn::<unsafe extern "C" fn(NvHandle, *mut NvApiThermals) -> NvStatus>(
            self.query_interface,
            query_ids::GET_THERMALS,
            "GetThermals",
        )?;

        let mut thermals = NvApiThermals::new(entry.thermals_mask);
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
            unsafe {
                unload_fn();
            }
        }
    }
}

/// Calculates the valid thermals mask for a GPU by probing bit positions.
fn calculate_thermals_mask(
    query_interface: QueryInterfaceFn,
    thermals_fn: unsafe extern "C" fn(NvHandle, *mut NvApiThermals) -> NvStatus,
    handle: NvHandle,
) -> Option<i32> {
    // First check if thermals work at all with mask=1
    let mut thermals = NvApiThermals::new(1);
    let status = unsafe { thermals_fn(handle, &raw mut thermals) };
    if status != 0 {
        debug!(
            "nvapi thermals not supported for GPU: {}",
            error_message(query_interface, status)
        );
        return None;
    }

    // Probe each bit to find the highest supported mask
    for bit in 0..32 {
        thermals.mask = 1 << bit;
        thermals.values = [0; 40];
        let status = unsafe { thermals_fn(handle, &raw mut thermals) };
        if status != 0 {
            let mask = (1 << bit) - 1;
            debug!("nvapi calculated thermals mask: {mask:#x}");
            return Some(mask);
        }
    }

    // All 32 bits succeeded (unlikely but valid)
    let mask = i32::MAX;
    debug!("nvapi calculated thermals mask: {mask:#x} (all bits)");
    Some(mask)
}

/// Resolves a function pointer from `nvapi_QueryInterface` by hash ID.
fn query_fn<F>(query_interface: QueryInterfaceFn, id: u32, name: &str) -> Option<F> {
    let ptr = unsafe { query_interface(id) };
    if ptr.is_null() {
        warn!("nvapi {name} function not found (query {id:#x})");
        return None;
    }
    // The function pointer types are determined by the nvapi interface.
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

    let mut text = [0i8; SHORT_STRING_MAX];
    let result = unsafe { error_fn(status, &raw mut text) };
    if result != 0 {
        return format!("status {status:#x}");
    }

    let msg = unsafe { CStr::from_ptr(text.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    format!("{msg} ({status:#x})")
}
