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


import {Type} from "class-transformer";
import type {UID} from "@/models/Device";

/**
 * Our internal representation of the DeviceSettingsDTO data
 */
export class DaemonDeviceSettings {
  settings: Map<string, DeviceSettingDTO> = new Map()
}

export type AllDaemonDeviceSettings = Map<UID, DaemonDeviceSettings>

export class DeviceSettingsDTO {
  @Type(() => DeviceSettingDTO)
  settings: Array<DeviceSettingDTO> = []
}

/**
 * Setting is a passed struct used to apply various settings to a specific device.
 * Usually only one specific lighting or speed setting is applied at a time.
 */
export class DeviceSettingDTO {
  channel_name: string

  /**
   * The fixed duty speed to set. eg: 20 (%)
   */
  speed_fixed?: number

  /**
   * The profile temp/duty speeds to set. eg: [(20, 50), (25, 80)]
   */
  speed_profile?: Array<[number, number]>

  /**
   * The associated temperature source
   */
  temp_source?: TempSource

  /**
   * Settings for lighting
   */
  lighting?: LightingSettings

  /**
   * Settings for LCD screens
   */
  lcd?: LcdSettings

  /**
   * the current pwm_mode to set for hwmon devices, eg: 1
   */
  pwm_mode?: number

  /**
   * Used to set hwmon & nvidia channels back to their default 'automatic' values.
   */
  reset_to_default?: boolean

  constructor(channelName: string) {
    this.channel_name = channelName
  }
}

export class TempSource {
  /**
   * The internal name for this Temperature Source. Not the frontend_name or external_name
   */
  temp_name: string

  /**
   * The associated device uid containing current temp values
   */
  device_uid: UID

  constructor(deviceUid: UID, tempName: string) {
    this.device_uid = deviceUid
    this.temp_name = tempName
  }
}

export class LightingSettings {
  /**
   * The lighting mode name
   */
  mode: string

  /**
   * The speed to set
   */
  speed?: string

  /**
   * run backwards or not
   */
  backward?: boolean

  /**
   * a list of RGB tuple values, eg [(20,20,120), (0,0,255)]
   */
  colors: Array<[number, number, number]> = []

  constructor(mode: string) {
    this.mode = mode
  }
}

export class LcdSettings {
  /**
   * The Lcd mode name
   */
  mode: string

  /**
   * The LCD brightness (0-100%)
   */
  brightness?: number

  /**
   * The LCD Image orientation (0,90,180,270)
   */
  orientation?: number

  /**
   * The LCD Source Image file path location
   */
  image_file_src?: string

  /**
   * The LCD Image tmp file path location, where the preprocessed image is located
   */
  image_file_processed?: string

  /**
   * a list of RGB tuple values, eg [(20,20,120), (0,0,255)]
   */
  colors: Array<[number, number, number]> = []

  constructor(mode: string) {
    this.mode = mode
  }
}
