/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2023  Guy Boldon
 * |
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 * |
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 * |
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */


import {SpeedOptions} from "@/models/SpeedOptions";
import {LightingMode} from "@/models/LightingMode";
import {LcdMode} from "@/models/LcdMode";
import {Type} from "class-transformer";

export class ChannelInfo {

    @Type(() => SpeedOptions)
    readonly speedOptions?: SpeedOptions;

    @Type(() => LightingMode)
    readonly lightingModes: LightingMode[] = [];

    @Type(() => LcdMode)
    readonly lcdModes: LcdMode[] = [];

    constructor(
            speedOptions?: SpeedOptions,
            lightingModes: LightingMode[] = [],
            lcdModes: LcdMode[] = []
    ) {
        this.lcdModes = lcdModes;
        this.lightingModes = lightingModes;
        this.speedOptions = speedOptions;
    }
}
