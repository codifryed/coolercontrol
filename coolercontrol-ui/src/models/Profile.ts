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

import {Transform, Type} from "class-transformer"
import type {UID} from "@/models/Device"
// @ts-ignore
import {v4 as uuidV4} from 'uuid'

/**
 * This is currently an internal model that will be also used externally by daemon at some point. The existing external
 * model will be transformed into this one until then.
 */
export class Profile {

  /**
   * The Unique identifier for this Profile
   */
  uid: UID = uuidV4()

  /**
   * The type of this Profile
   */
  p_type: ProfileType = ProfileType.Default

  /**
   * The name of this Profile
   */
  name: string

  /**
   * The fixed duty speed to set. eg: 20 (%)
   */
  speed_fixed?: number

  /**
   * The profile temp/duty speeds to set. eg: `[(20, 50), (25, 80)]`
   */
  @Transform(({value}) => {
    const profile: Array<[number, number]> | undefined = value
    if (profile != null) {
      for (const point of profile) {
        // temp:
        point[0] = Math.round(point[0] * 10) / 10;
        // duty:
        point[1] = Math.round(point[1]);
      }
    }
    return profile
  })
  speed_profile: Array<[number, number]> = []

  /**
   * The associated temperature source
   */
  @Type(() => ProfileTempSource)
  temp_source?: ProfileTempSource

  /**
   * The function UID to apply to this profile
   */
  function_uid: UID = "0"

  constructor(
      name: string,
      type: ProfileType,
      speed_fixed?: number,
      temp_source?: ProfileTempSource,
      speed_profile: Array<[number, number]> = [],
  ) {
    this.name = name
    this.p_type = type
    this.speed_fixed = speed_fixed
    this.speed_profile = speed_profile
    this.temp_source = temp_source
  }

  static createDefault(): Profile {
    const profile = new Profile('Default Profile', ProfileType.Default)
    profile.uid = '0' // this indicates a special once-only non-deleteable default profile that we always need to have available
    return profile
  }
}

export enum ProfileType {
  Default = 'Default',
  Fixed = 'Fixed',
  Graph = 'Graph',
  // Mix = 'Mix',
}

export class ProfileTempSource {
  // todo: TempSourceTypes to enable Custom Temp Sources
  constructor(
      /**
       * The internal name for this Temperature Source. Not the frontend_name or external_name
       */
      readonly temp_name: string,
      /**
       * The associated device uid containing current temp values
       */
      readonly device_uid: UID,
  ) {
  }
}

export class ProfilesDTO {
  @Type(() => Profile)
  profiles: Array<Profile> = []
}

export enum FunctionType {
  Identity = 'Identity',
  Standard = 'Standard',
  ExponentialMovingAvg = 'ExponentialMovingAvg',
}

export class Function {
  /**
   * The Unique identifier for this function
   */
  uid: UID = uuidV4()

  /**
   * The user given name for this function
   */
  name: string

  /**
   * The type of this function
   */
  f_type: FunctionType = FunctionType.Identity

  /**
   * The response delay in seconds
   */
  response_delay?: number

  /**
   * The temperature deviance threshold in degrees
   */
  deviance?: number

  /**
   * The sample window this function should use, particularly applicable to moving averages
   */
  sample_window?: number

  constructor(
      name: string,
      f_type: FunctionType = FunctionType.Identity,
      response_delay: number | undefined = undefined,
      deviance: number | undefined = undefined,
      sample_window: number | undefined = undefined
  ) {
    this.name = name
    this.f_type = f_type
    this.response_delay = response_delay
    this.deviance = deviance
    this.sample_window = sample_window
  }

  static createDefault(): Function {
    return new Function('Identity')
  }
}

export class FunctionsDTO {
  @Type(() => Function)
  functions: Array<Function> = []
}
