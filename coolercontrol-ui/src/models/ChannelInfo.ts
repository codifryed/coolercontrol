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

import { SpeedOptions } from '@/models/SpeedOptions'
import { LightingMode } from '@/models/LightingMode'
import { LcdMode } from '@/models/LcdMode'
import { Type } from 'class-transformer'
import { LcdInfo } from '@/models/LcdInfo'

export class ChannelInfo {
    readonly label?: string

    @Type(() => SpeedOptions)
    readonly speed_options?: SpeedOptions

    @Type(() => LightingMode)
    readonly lighting_modes: LightingMode[] = []

    @Type(() => LcdMode)
    readonly lcd_modes: LcdMode[] = []

    @Type(() => LcdInfo)
    readonly lcd_info?: LcdInfo

    constructor(
        label?: string,
        speed_options?: SpeedOptions,
        lighting_modes: LightingMode[] = [],
        lcd_modes: LcdMode[] = [],
        lcd_info?: LcdInfo,
    ) {
        this.label = label
        this.lcd_modes = lcd_modes
        this.lighting_modes = lighting_modes
        this.speed_options = speed_options
        this.lcd_info = lcd_info
    }
}
