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

use std::collections::{HashMap, HashSet};
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
    pub const REGISTER_OP: u32 = 0x2eb3c140;
}

const MAX_PHYSICAL_GPUS: usize = 64;
const SHORT_STRING_MAX: usize = 64;
const THERMALS_VALUE_COUNT: usize = 40;
const HOTSPOT_INDEX: usize = 9;
/// nvapi reports temperatures in Q8.8 fixed point, so Celsius is the raw value / 256.
const TEMP_SCALE_FACTOR: i32 = 256;
/// Valid temperature range for filtering (exclusive bounds).
const VALID_TEMP_RANGE: std::ops::Range<i32> = 1..255;

/// Aggregated hotspot register. Blackwell stopped publishing hotspot through the
/// thermals array, so on those cards it has to be read out of this register instead.
const REGISTER_OFFSET_HOTSPOT: u32 = 0x00ad_0aa0;
/// Batch length fixed by the nvapi ABI: the request encodes its own size in the version
/// field, so the array cannot be trimmed down to the single op we actually issue.
const REGISTER_OP_COUNT_MAX: usize = 256;
const REGISTER_OP_FLAG_READ: u16 = 1;
const REGISTER_OP_FLAG_32BIT: u16 = 4;
const REGISTER_OP_FLAG_TYPE_GLOBAL: u16 = 16;
const REGISTER_OP_FLAGS_HOTSPOT_READ: u16 =
    REGISTER_OP_FLAG_READ | REGISTER_OP_FLAG_32BIT | REGISTER_OP_FLAG_TYPE_GLOBAL;
/// The temperature sits in the low 16 bits of the register; the rest is unrelated state.
const REGISTER_TEMP_MASK: u64 = 0xFFFF;

// Compile-time invariant checks.
const _: () = assert!(HOTSPOT_INDEX < THERMALS_VALUE_COUNT);
const _: () = assert!(MAX_PHYSICAL_GPUS <= u32::MAX as usize);
// A masked register read always fits an i32.
const _: () = assert!(REGISTER_TEMP_MASK <= i32::MAX as u64);
// Both request structs encode their own size in the low 16 bits of the version field.
const _: () = assert!(size_of::<NvApiThermals>() < u16::MAX as usize);
const _: () = assert!(size_of::<NvGpuRegisterOpData>() < u16::MAX as usize);

type NvHandle = *mut core::ffi::c_void;
type NvStatus = i32;
type QueryInterfaceFn = unsafe extern "C" fn(u32) -> *const ();
type EnumGpusFn = unsafe extern "C" fn(*mut [NvHandle; MAX_PHYSICAL_GPUS], *mut u32) -> NvStatus;
type BusIdFn = unsafe extern "C" fn(NvHandle, *mut u32) -> NvStatus;
type ThermalsFn = unsafe extern "C" fn(NvHandle, *mut NvApiThermals) -> NvStatus;
type RegisterOpFn = unsafe extern "C" fn(NvHandle, *mut NvGpuRegisterOpData) -> NvStatus;

#[repr(C)]
struct NvApiThermals {
    version: u32,
    mask: i32,
    values: [i32; THERMALS_VALUE_COUNT],
}

impl NvApiThermals {
    fn new(mask: i32) -> Self {
        Self {
            version: make_version(size_of::<Self>(), 2),
            mask,
            values: [0; THERMALS_VALUE_COUNT],
        }
    }

    fn hotspot(&self) -> Option<f64> {
        scaled_temp_celsius(self.values[HOTSPOT_INDEX])
    }
}

/// One entry of an nvapi register-op batch, laid out as the driver expects it.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NvGpuRegisterOp {
    flags: u16,
    /// Per-op result. Not consulted: the temperature range check rejects a bad read.
    status: u16,
    offset: u32,
    /// Only meaningful for writes, which this daemon never issues.
    write_mask: u64,
    value: u64,
}

#[repr(C)]
struct NvGpuRegisterOpData {
    version: u32,
    op_count: u32,
    ops: [NvGpuRegisterOp; REGISTER_OP_COUNT_MAX],
}

impl NvGpuRegisterOpData {
    /// A batch holding a single 32-bit global read of `offset`; the rest stays zeroed.
    fn new_read(offset: u32) -> Self {
        let mut data = Self {
            version: make_version(size_of::<Self>(), 1),
            op_count: 1,
            ops: [NvGpuRegisterOp::default(); REGISTER_OP_COUNT_MAX],
        };
        data.ops[0] = NvGpuRegisterOp {
            flags: REGISTER_OP_FLAGS_HOTSPOT_READ,
            offset,
            ..Default::default()
        };
        data
    }
}

/// How one GPU's hotspot temperature has to be read. Resolved once at init so the
/// per-tick read is a straight dispatch with no capability probing.
#[derive(Clone, Copy)]
enum HotspotSource {
    /// Pre-Blackwell: a fixed slot of the nvapi thermals array.
    Thermals { read: ThermalsFn, mask: i32 },
    /// Blackwell and newer: that slot reads back invalid, so the aggregated hotspot
    /// register is read directly.
    Register { read: RegisterOpFn },
}

/// The nvapi entry points a hotspot read can go through. Either may be missing on a
/// given driver, so source resolution picks from whatever actually resolved.
struct HotspotFunctions {
    thermals: Option<ThermalsFn>,
    register_op: Option<RegisterOpFn>,
}

struct GpuEntry {
    handle: NvHandle,
    hotspot_source: HotspotSource,
}

pub struct NvApi {
    _lib: libloading::Library,
    query_interface: QueryInterfaceFn,
    gpu_entries: HashMap<u32, GpuEntry>,
}

impl NvApi {
    /// Attempts to load and initialize the nvapi library.
    /// `register_hotspot_buses` holds the PCI bus IDs of GPUs whose architecture keeps
    /// hotspot out of the thermals array (Blackwell and newer); those are probed for the
    /// register read path first.
    /// Returns None if the library is not available or initialization fails.
    pub fn try_init(register_hotspot_buses: &HashSet<u32>) -> Option<Self> {
        let lib = load_library()?;
        let query_interface = load_query_interface(&lib)?;
        initialize_api(query_interface)?;
        let gpu_entries = enumerate_gpu_entries(query_interface, register_hotspot_buses)?;
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
        match entry.hotspot_source {
            HotspotSource::Thermals { read, mask } => {
                read_thermals_hotspot(self.query_interface, read, entry.handle, mask)
            }
            HotspotSource::Register { read } => {
                read_register_hotspot(self.query_interface, read, entry.handle)
            }
        }
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

/// Reads the hotspot out of the nvapi thermals array (pre-Blackwell path).
fn read_thermals_hotspot(
    query_interface: QueryInterfaceFn,
    thermals_fn: ThermalsFn,
    handle: NvHandle,
    mask: i32,
) -> Option<f64> {
    let mut thermals = NvApiThermals::new(mask);
    // SAFETY: thermals_fn was resolved once at init and is a validated nvapi
    // function pointer. The NvApiThermals struct is #[repr(C)] with correct
    // version encoding, matching the nvapi ABI.
    let status = unsafe { thermals_fn(handle, &raw mut thermals) };
    if status != 0 {
        debug!(
            "nvapi GetThermals failed: {}",
            error_message(query_interface, status)
        );
        return None;
    }
    thermals.hotspot()
}

/// Reads the aggregated hotspot register (Blackwell and newer path).
fn read_register_hotspot(
    query_interface: QueryInterfaceFn,
    register_op_fn: RegisterOpFn,
    handle: NvHandle,
) -> Option<f64> {
    let raw_value = read_register(
        query_interface,
        register_op_fn,
        handle,
        REGISTER_OFFSET_HOTSPOT,
    )?;
    register_temp_celsius(raw_value)
}

/// Issues a single 32-bit global register read and returns the raw value.
fn read_register(
    query_interface: QueryInterfaceFn,
    register_op_fn: RegisterOpFn,
    handle: NvHandle,
    offset: u32,
) -> Option<u64> {
    let mut data = NvGpuRegisterOpData::new_read(offset);
    // SAFETY: register_op_fn was resolved once at init and is a validated nvapi
    // function pointer. NvGpuRegisterOpData is #[repr(C)] with the version field
    // encoding its own size, which is how the driver validates the layout.
    let status = unsafe { register_op_fn(handle, &raw mut data) };
    if status != 0 {
        debug!(
            "nvapi register read of {offset:#x} failed: {}",
            error_message(query_interface, status)
        );
        return None;
    }
    debug_assert_eq!(data.op_count, 1, "nvapi altered the size of a read batch");
    Some(data.ops[0].value)
}

/// Decodes the temperature out of a raw register read. The reading occupies the low 16
/// bits in the same fixed-point encoding the thermals array uses; the upper bits carry
/// unrelated state and are masked off.
fn register_temp_celsius(raw_value: u64) -> Option<f64> {
    // Cannot fail: REGISTER_TEMP_MASK is asserted to fit an i32 above.
    let masked_value = i32::try_from(raw_value & REGISTER_TEMP_MASK).ok()?;
    scaled_temp_celsius(masked_value)
}

/// Converts a fixed-point nvapi reading to Celsius, rejecting values outside the
/// plausible range. Both read paths share this: hardware lies, and an unpopulated
/// sensor reads back as zero or saturated.
fn scaled_temp_celsius(raw_value: i32) -> Option<f64> {
    let temp_celsius = raw_value / TEMP_SCALE_FACTOR;
    if VALID_TEMP_RANGE.contains(&temp_celsius) {
        return Some(f64::from(temp_celsius));
    }
    None
}

/// nvapi request structs carry a version that packs the struct size together with the
/// ABI revision, which is how the driver validates the layout it was handed.
#[allow(clippy::cast_possible_truncation)] // Both struct sizes are asserted to fit above.
const fn make_version(size_bytes: usize, revision: u32) -> u32 {
    (size_bytes as u32) | (revision << 16)
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

/// Builds the bus ID to GPU entry map.
/// Returns None if no GPU can report a hotspot temperature.
fn enumerate_gpu_entries(
    query_interface: QueryInterfaceFn,
    register_hotspot_buses: &HashSet<u32>,
) -> Option<HashMap<u32, GpuEntry>> {
    let enum_fn = query_fn::<EnumGpusFn>(
        query_interface,
        query_ids::ENUM_PHYSICAL_GPUS,
        "EnumPhysicalGPUs",
    )?;
    let bus_id_fn = query_fn::<BusIdFn>(query_interface, query_ids::GET_BUS_ID, "GetBusId")?;
    let functions = resolve_hotspot_functions(query_interface, register_hotspot_buses);

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
    debug_assert!(gpu_count <= handles.len());
    let gpu_entries = collect_gpu_entries(
        query_interface,
        bus_id_fn,
        &functions,
        &handles[..gpu_count],
        register_hotspot_buses,
    );
    if gpu_entries.is_empty() {
        info!("nvapi: no GPUs support hotspot temperature monitoring");
        return None;
    }
    Some(gpu_entries)
}

/// Resolves both read paths up front. The register op is only looked up when some GPU
/// actually needs it, so drivers without it stay quiet on the common path.
fn resolve_hotspot_functions(
    query_interface: QueryInterfaceFn,
    register_hotspot_buses: &HashSet<u32>,
) -> HotspotFunctions {
    let thermals = query_fn::<ThermalsFn>(query_interface, query_ids::GET_THERMALS, "GetThermals");
    if register_hotspot_buses.is_empty() {
        return HotspotFunctions {
            thermals,
            register_op: None,
        };
    }
    HotspotFunctions {
        thermals,
        register_op: query_fn::<RegisterOpFn>(
            query_interface,
            query_ids::REGISTER_OP,
            "RegisterOp",
        ),
    }
}

/// Maps each enumerated GPU handle onto its PCI bus and the hotspot read path it needs.
/// A GPU with no working path is left out, so a later lookup simply finds nothing.
fn collect_gpu_entries(
    query_interface: QueryInterfaceFn,
    bus_id_fn: BusIdFn,
    functions: &HotspotFunctions,
    handles: &[NvHandle],
    register_hotspot_buses: &HashSet<u32>,
) -> HashMap<u32, GpuEntry> {
    debug_assert!(handles.len() <= MAX_PHYSICAL_GPUS);
    let mut gpu_entries = HashMap::with_capacity(handles.len());
    for handle in handles {
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
        let prefers_register = register_hotspot_buses.contains(&bus_id);
        match resolve_hotspot_source(query_interface, functions, *handle, prefers_register) {
            Some(hotspot_source) => {
                gpu_entries.insert(
                    bus_id,
                    GpuEntry {
                        handle: *handle,
                        hotspot_source,
                    },
                );
            }
            None => debug!("nvapi GPU at bus {bus_id} reports no readable hotspot sensor"),
        }
    }
    gpu_entries
}

/// Picks the read path for one GPU. Blackwell and newer are probed for the register
/// first, since their thermals array carries no hotspot. The probe also covers the
/// architectures NVML could not classify: guessing register there costs one read, and a
/// card that turns out to be older just falls back to the thermals array.
fn resolve_hotspot_source(
    query_interface: QueryInterfaceFn,
    functions: &HotspotFunctions,
    handle: NvHandle,
    prefers_register: bool,
) -> Option<HotspotSource> {
    if prefers_register {
        if let Some(read) = functions.register_op {
            if read_register_hotspot(query_interface, read, handle).is_some() {
                return Some(HotspotSource::Register { read });
            }
            debug!("nvapi hotspot register unreadable, falling back to the thermals array");
        }
    }
    let read = functions.thermals?;
    let mask = calculate_thermals_mask(query_interface, read, handle)?;
    Some(HotspotSource::Thermals { read, mask })
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;

    /// The driver validates the request layout byte for byte, so a silent change to
    /// either struct would be rejected, or worse misread, at runtime. Pin the sizes and
    /// field offsets the nvapi ABI dictates.
    #[test]
    fn register_op_structs_match_the_nvapi_abi() {
        assert_eq!(size_of::<NvGpuRegisterOp>(), 24);
        assert_eq!(offset_of!(NvGpuRegisterOp, flags), 0);
        assert_eq!(offset_of!(NvGpuRegisterOp, status), 2);
        assert_eq!(offset_of!(NvGpuRegisterOp, offset), 4);
        assert_eq!(offset_of!(NvGpuRegisterOp, write_mask), 8);
        assert_eq!(offset_of!(NvGpuRegisterOp, value), 16);

        let ops_size = 24 * REGISTER_OP_COUNT_MAX;
        assert_eq!(size_of::<NvGpuRegisterOpData>(), 8 + ops_size);
        assert_eq!(offset_of!(NvGpuRegisterOpData, version), 0);
        assert_eq!(offset_of!(NvGpuRegisterOpData, op_count), 4);
        assert_eq!(offset_of!(NvGpuRegisterOpData, ops), 8);
    }

    /// A request the driver accepts has to declare exactly one op, flagged as a 32-bit
    /// global read, with every unused slot left zeroed. Build one and inspect it.
    #[test]
    fn register_read_request_holds_one_global_32bit_read() {
        let data = NvGpuRegisterOpData::new_read(REGISTER_OFFSET_HOTSPOT);
        let expected_version = make_version(size_of::<NvGpuRegisterOpData>(), 1);
        assert_eq!(data.version, expected_version);
        assert_eq!(data.op_count, 1);
        assert_eq!(data.ops[0].offset, REGISTER_OFFSET_HOTSPOT);
        assert_eq!(data.ops[0].flags, 21); // read | 32-bit | global
        assert_eq!(data.ops[0].value, 0);
        assert_eq!(data.ops[0].write_mask, 0);
        assert!(data.ops[1..].iter().all(|op| op.flags == 0));
        assert!(data.ops[1..].iter().all(|op| op.offset == 0));
    }

    /// The version field packs the struct size with the ABI revision. Both request
    /// structs depend on it, so check the packing against the size it encodes.
    #[test]
    fn version_packs_size_with_revision() {
        assert_eq!(make_version(0x1808, 1), 0x0001_1808);
        assert_eq!(make_version(0xB0, 2), 0x0002_00B0);
        let thermals = NvApiThermals::new(1);
        let thermals_size = u32::try_from(size_of::<NvApiThermals>()).unwrap();
        assert_eq!(thermals.version & 0xFFFF, thermals_size);
        assert_eq!(thermals.version >> 16, 2);
    }

    /// Positive space: readings inside the plausible range decode to Celsius, truncating
    /// the fractional part of the fixed-point value.
    #[test]
    fn valid_fixed_point_readings_decode_to_celsius() {
        assert_eq!(scaled_temp_celsius(45 * TEMP_SCALE_FACTOR), Some(45.));
        assert_eq!(scaled_temp_celsius(TEMP_SCALE_FACTOR), Some(1.));
        assert_eq!(scaled_temp_celsius(254 * TEMP_SCALE_FACTOR), Some(254.));
        // 45.5C truncates down rather than rounding up.
        assert_eq!(scaled_temp_celsius(45 * TEMP_SCALE_FACTOR + 128), Some(45.));
    }

    /// Negative space: an absent or saturated sensor must not surface as a temperature.
    /// Zero, the 255 ceiling, anything above it, and negatives are all rejected.
    #[test]
    fn implausible_readings_are_rejected() {
        assert_eq!(scaled_temp_celsius(0), None);
        assert_eq!(scaled_temp_celsius(TEMP_SCALE_FACTOR - 1), None); // 0.99C
        assert_eq!(scaled_temp_celsius(255 * TEMP_SCALE_FACTOR), None);
        assert_eq!(scaled_temp_celsius(i32::MAX), None);
        assert_eq!(scaled_temp_celsius(-TEMP_SCALE_FACTOR), None);
        assert_eq!(scaled_temp_celsius(i32::MIN), None);
    }

    /// The register carries unrelated state above the temperature, so decoding has to
    /// mask it off. Feed values with the upper bits set and confirm only the low word
    /// is read.
    #[test]
    fn register_decoding_masks_off_the_upper_bits() {
        assert_eq!(register_temp_celsius(0x2D00), Some(45.));
        assert_eq!(register_temp_celsius(0xDEAD_BEEF_0000_2D00), Some(45.));
        // Upper bits alone leave no temperature behind.
        assert_eq!(register_temp_celsius(0xDEAD_BEEF_0000_0000), None);
        assert_eq!(register_temp_celsius(u64::MAX), None); // Low word 0xFFFF is 255C.
    }

    /// The hotspot lives at a fixed slot of the thermals array. Fill its neighbours with
    /// different values to prove the accessor reads that slot and no other.
    #[test]
    fn thermals_reads_the_hotspot_slot() {
        let mut thermals = NvApiThermals::new(0x7);
        thermals.values[HOTSPOT_INDEX] = 61 * TEMP_SCALE_FACTOR;
        thermals.values[HOTSPOT_INDEX - 1] = 39 * TEMP_SCALE_FACTOR;
        thermals.values[HOTSPOT_INDEX + 1] = 72 * TEMP_SCALE_FACTOR;
        assert_eq!(thermals.hotspot(), Some(61.));
    }

    /// A fresh thermals request carries the caller's mask and no stale readings, so an
    /// unsupported slot cannot be mistaken for a real temperature.
    #[test]
    fn thermals_request_starts_zeroed() {
        let thermals = NvApiThermals::new(0x1F);
        assert_eq!(thermals.mask, 0x1F);
        assert!(thermals.values.iter().all(|value| *value == 0));
        assert_eq!(thermals.hotspot(), None);
    }
}
