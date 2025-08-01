/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon and contributors
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

/**
 * This is a representation of the liquidctl driver instance
 */
export enum LcDriverType {
    Aquacomputer = 'Aquacomputer',
    CommanderPro = 'CommanderPro',
    Kraken2 = 'Kraken2',
    KrakenX3 = 'KrakenX3',
    KrakenZ3 = 'KrakenZ3',
    MockKrakenZ3 = 'MockKrakenZ3',
    SmartDevice = 'SmartDevice',
    SmartDevice2 = 'SmartDevice2',
    H1V2 = 'H1V2',
    HydroPlatinum = 'HydroPlatinum',
    CorsairHidPsu = 'CorsairHidPsu',
    RgbFusion2 = 'RgbFusion2',
    AuraLed = 'AuraLed',
    CommanderCore = 'CommanderCore',
    NzxtEPsu = 'NzxtEPsu',
    Modern690Lc = 'Modern690Lc',
    MsiAcpiEc = 'MsiAcpiEc',
    Hydro690Lc = 'Hydro690Lc',
    Legacy690Lc = 'Legacy690Lc',
    HydroPro = 'HydroPro',
    EvgaPascal = 'EvgaPascal',
    RogTuring = 'RogTuring',
    Ddr4Temperature = 'Ddr4Temperature',
    VengeanceRgb = 'VengeanceRgb',
}

/**
 * Get the localized display name for LcDriverType
 * Note: Since model names typically don't need translation, we only add the function interface here but keep the original names
 * If specific model names need to be localized in the future, corresponding translations can be added here
 * @param type LcDriverType enum value
 * @returns Localized display name
 */
export function getLcDriverTypeDisplayName(type: LcDriverType): string {
    // These device models typically aren't translated, so return the original value
    // If localized display names are needed in the future, appropriate translation keys can be added
    return String(type)
}
