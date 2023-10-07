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


import {DefaultDictionary} from "typescript-collections";
import type {Color} from "@/models/Device";

/**
 * A DTO Class to hold all the UI settings to be persisted by the daemon. (snake case)
 */
export class UISettings {

}

/**
 * A Device's Settings
 */
export class DeviceSettings {

  /**
   * A Map of Sensor and Channel Names to associated Settings.
   */
  readonly sensorsAndChannels: DefaultDictionary<string, SensorAndChannelSettings> =
      new DefaultDictionary(() => new SensorAndChannelSettings())
}

export class SensorAndChannelSettings {

  defaultColor: Color
  userColor: Color | undefined
  icon: string | undefined
  hide: boolean

  constructor(
      defaultColor: Color = '#568af2',
      icon: string | undefined = undefined,
      userColor: Color | undefined = undefined,
      hide: boolean = false,
  ) {
    this.defaultColor = defaultColor
    this.icon = icon
    this.userColor = userColor
    this.hide = hide
  }

  get color(): Color {
    return this.userColor == null ? this.defaultColor : this.userColor
  }
}