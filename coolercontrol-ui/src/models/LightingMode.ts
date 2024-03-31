/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon and contributors
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

export enum LightingModeType {
    NONE = 'None',
    LC = 'Liquidctl',
    CUSTOM = 'Custom',
}

export class LightingMode {
    constructor(
        readonly name: string,
        readonly frontend_name: string,
        readonly min_colors: number,
        readonly max_colors: number,
        readonly speed_enabled: boolean,
        readonly backward_enabled: boolean,
        readonly type: LightingModeType = LightingModeType.LC,
    ) {}
}
