/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
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
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize, JsonSchema,
)]
pub enum BaseDriver {
    // with associated liquidctl python driver filename
    Aquacomputer,    // aquacomputer.py
    Legacy690Lc,     // asetek.py
    Modern690Lc,     // asetek.py
    Hydro690Lc,      // asetek.py
    HydroPro,        // asetek_pro.py
    AsusRyujin,      // asus_ryujin.py
    AuraLed,         // aura_led.py
    CommanderCore,   // commander_core.py
    CommanderPro,    // commander_pro.py
    CorsairHidPsu,   // corsair_hid_psu.py
    Ddr4Temperature, // ddr4.py - NOT currently Supported - requires unsafe ops
    VengeanceRgb,    // ddr4.py - NOT currently Supported - requires unsafe ops
    HydroPlatinum,   // hydro_platinum.py
    Kraken2,         // kraken2.py
    KrakenX3,        // kraken3.py
    KrakenZ3,        // kraken3.py
    MockKrakenZ3,    // kraken3.py
    MpgCooler,       // msi.py
    EvgaPascal,      // nvidia.py - NOT currently Supported - requires unsafe ops
    RogTuring,       // nvidia.py - NOT currently Supported - requires unsafe ops
    NzxtEPsu,        // nzxt_epsu.py
    RgbFusion2,      // rgb_fusion2.py
    SmartDevice,     // smart_device.py
    SmartDevice2,    // smart_device.py
    H1V2,            // smart_device.py
    MsiAcpiEc,       // custom out-of-tree driver
}
