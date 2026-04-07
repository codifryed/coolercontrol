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
use std::mem;

use log::{debug, info, warn};

const LIBRARY_NAME: &str = "libnvidia-api.so.1";
const QUERY_INTERFACE_FN: &[u8] = b"nvapi_QueryInterface\0";

const QUERY_INITIALIZE: u32 = 0x0150e828;
const QUERY_UNLOAD: u32 = 0xd22bdd7e;
const QUERY_ENUM_PHYSICAL_GPUS: u32 = 0xe5ac921f;
const QUERY_GET_BUS_ID: u32 = 0x1be0b8e5;
const QUERY_GET_THERMALS: u32 = 0x65fe3aad;

const MAX_PHYSICAL_GPUS: usize = 64;
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
    fn new() -> Self {
        #[allow(clippy::cast_possible_truncation)]
        Self {
            version: (mem::size_of::<Self>() | (2 << 16)) as u32,
            mask: 0,
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

pub struct NvApi {
    _lib: libloading::Library,
    query_interface: QueryInterfaceFn,
    gpu_handles: HashMap<u32, NvHandle>,
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
            QUERY_INITIALIZE,
            "Initialize",
        )?;
        let status = unsafe { init_fn() };
        if status != 0 {
            warn!("nvapi Initialize failed with status: {status:#x}");
            return None;
        }

        // Enumerate GPUs and build bus ID map
        let enum_fn = query_fn::<
            unsafe extern "C" fn(*mut [NvHandle; MAX_PHYSICAL_GPUS], *mut u32) -> NvStatus,
        >(
            query_interface,
            QUERY_ENUM_PHYSICAL_GPUS,
            "EnumPhysicalGPUs",
        )?;
        let bus_id_fn = query_fn::<unsafe extern "C" fn(NvHandle, *mut u32) -> NvStatus>(
            query_interface,
            QUERY_GET_BUS_ID,
            "GetBusId",
        )?;

        let mut handles = [std::ptr::null_mut(); MAX_PHYSICAL_GPUS];
        let mut count: u32 = 0;
        let status = unsafe { enum_fn(&mut handles, &mut count) };
        if status != 0 {
            warn!("nvapi EnumPhysicalGPUs failed with status: {status:#x}");
            return None;
        }

        let mut gpu_handles = HashMap::new();
        for handle in handles.iter().take(count as usize) {
            let mut bus_id: u32 = 0;
            let status = unsafe { bus_id_fn(*handle, &mut bus_id) };
            if status != 0 {
                debug!("nvapi GetBusId failed for a GPU handle: {status:#x}");
                continue;
            }
            gpu_handles.insert(bus_id, *handle);
        }

        if gpu_handles.is_empty() {
            warn!("nvapi found no GPUs with valid bus IDs");
            return None;
        }

        info!(
            "nvapi initialized with {} GPU(s) for hotspot temperature monitoring",
            gpu_handles.len()
        );

        Some(Self {
            _lib: lib,
            query_interface,
            gpu_handles,
        })
    }

    /// Returns the hotspot/junction temperature for the GPU at the given PCI bus ID.
    pub fn get_hotspot_temp(&self, pci_bus: u32) -> Option<f64> {
        let handle = self.gpu_handles.get(&pci_bus)?;
        let thermals_fn = query_fn::<unsafe extern "C" fn(NvHandle, *mut NvApiThermals) -> NvStatus>(
            self.query_interface,
            QUERY_GET_THERMALS,
            "GetThermals",
        )?;

        let mut thermals = NvApiThermals::new();
        let status = unsafe { thermals_fn(*handle, &mut thermals) };
        if status != 0 {
            debug!("nvapi GetThermals failed with status: {status:#x}");
            return None;
        }
        thermals.hotspot()
    }
}

impl Drop for NvApi {
    fn drop(&mut self) {
        if let Some(unload_fn) = query_fn::<unsafe extern "C" fn() -> NvStatus>(
            self.query_interface,
            QUERY_UNLOAD,
            "Unload",
        ) {
            unsafe {
                unload_fn();
            }
        }
    }
}

/// Resolves a function pointer from nvapi_QueryInterface by hash ID.
fn query_fn<F>(query_interface: QueryInterfaceFn, id: u32, name: &str) -> Option<F> {
    let ptr = unsafe { query_interface(id) };
    if ptr.is_null() {
        warn!("nvapi {name} function not found (query {id:#x})");
        return None;
    }
    // The function pointer types are determined by the nvapi interface.
    Some(unsafe { mem::transmute_copy(&ptr) })
}
