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

import { Device, DeviceType, type UID } from '@/models/Device'
import * as d3scale from 'd3-scale'
import * as d3chromatic from 'd3-scale-chromatic'
import { DeviceUISettings, SensorAndChannelSettings } from '@/models/UISettings'

function setDeviceColors(
    devices: Array<Device>,
    deviceSettings: Map<UID, DeviceUISettings>,
    deviceTypes: Array<DeviceType>,
    interpolatedColorFn: (t: number) => string,
): void {
    const selectedDevices = devices.filter((device) => deviceTypes.includes(device.type))
    let numberOfColors: number = 0
    for (const device of selectedDevices) {
        if (!device.status_history.length) {
            continue // no statuses means no colors needed for this device
        }
        numberOfColors += device.status.channels.length
        numberOfColors += device.status.temps.length
    }

    const colors = createColors(numberOfColors, interpolatedColorFn)
    let colorIndex: number = 0
    for (const device of selectedDevices) {
        if (!device.status_history.length) {
            continue
        }
        const settings = deviceSettings.get(device.uid)!
        const sortedChannels = device.status.channels.sort((c1, c2) =>
            c1.name.localeCompare(c2.name, undefined, { numeric: true, sensitivity: 'base' }),
        )
        for (const channelStatus of sortedChannels) {
            let channelSettings = settings.sensorsAndChannels.get(channelStatus.name)
            if (channelSettings == null) {
                channelSettings = new SensorAndChannelSettings()
                settings.sensorsAndChannels.set(channelStatus.name, channelSettings)
            }
            channelSettings.defaultColor = colors[colorIndex]
            colorIndex++
        }
        const sortedTemps = device.status.temps.sort((t1, t2) =>
            t1.name.localeCompare(t2.name, undefined, { numeric: true, sensitivity: 'base' }),
        )
        for (const tempStatus of sortedTemps) {
            let tempSettings = settings.sensorsAndChannels.get(tempStatus.name)
            if (tempSettings == null) {
                tempSettings = new SensorAndChannelSettings()
                settings.sensorsAndChannels.set(tempStatus.name, tempSettings)
            }
            tempSettings.defaultColor = colors[colorIndex]
            colorIndex++
        }
    }
}

function createColors(
    numberOfColors: number,
    interpolatedColorFn: (t: number) => string,
): Array<string> {
    const colors: Array<string> = []
    if (!numberOfColors) {
        return colors
    }
    const scaleValue = d3scale.scaleLinear([0, numberOfColors], getRange(interpolatedColorFn))
    const colorScale = d3scale.scaleSequential(interpolatedColorFn)
    for (let i = 0; i < numberOfColors; i++) {
        colors.push(colorScale(scaleValue(i)))
    }
    return colors
}

/**
 * Sets a range for the color scheme, so that we use the parts of the color schemes that we want.
 * @param interpolatedColorFn
 */
function getRange(interpolatedColorFn: (t: number) => string): Array<number> {
    switch (interpolatedColorFn) {
        case d3chromatic.interpolateReds:
            return [0.55, 0.9]
        case d3chromatic.interpolateOranges:
            return [0.3, 0.7]
        case d3chromatic.interpolatePlasma:
            return [0.7, 1.0]
        case d3chromatic.interpolateCool:
            return [0.0, 0.8]
        case d3chromatic.interpolateYlOrBr:
            return [0.5, 0.7]
        case d3chromatic.interpolateBuGn:
            return [0.7, 1.0]
        default:
            return [0.4, 0.9]
    }
}

export default function setDefaultSensorAndChannelColors(
    devices: Array<Device>,
    deviceSettings: Map<UID, DeviceUISettings>,
): void {
    setDeviceColors(devices, deviceSettings, [DeviceType.CPU], d3chromatic.interpolateReds)
    setDeviceColors(devices, deviceSettings, [DeviceType.GPU], d3chromatic.interpolatePlasma)
    setDeviceColors(
        devices,
        deviceSettings,
        [DeviceType.LIQUIDCTL, DeviceType.HWMON],
        d3chromatic.interpolateCool,
    )
    setDeviceColors(
        devices,
        deviceSettings,
        [DeviceType.CUSTOM_SENSORS],
        d3chromatic.interpolateBuGn,
    )
}
