<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2021-2024  Guy Boldon and contributors
  -
  - This program is free software: you can redistribute it and/or modify
  - it under the terms of the GNU General Public License as published by
  - the Free Software Foundation, either version 3 of the License, or
  - (at your option) any later version.
  -
  - This program is distributed in the hope that it will be useful,
  - but WITHOUT ANY WARRANTY; without even the implied warranty of
  - MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  - GNU General Public License for more details.
  -
  - You should have received a copy of the GNU General Public License
  - along with this program.  If not, see <https://www.gnu.org/licenses/>.
  -->

<script setup lang="ts">
import { type UID } from '@/models/Device'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import { DeviceSettingReadDTO, DeviceSettingWriteLightingDTO } from '@/models/DaemonSettings'
import { LightingMode, LightingModeType } from '@/models/LightingMode'
import { computed, nextTick, onMounted, ref, type Ref, watch } from 'vue'
import Dropdown from 'primevue/dropdown'
import InputNumber from 'primevue/inputnumber'
import ToggleButton from 'primevue/togglebutton'
import { ElColorPicker } from 'element-plus'
import 'element-plus/es/components/color-picker/style/css'
import Button from 'primevue/button'

interface Props {
    deviceId: UID
    name: string
}

const props = defineProps<Props>()

const absoluteMaxColors = 48 // Current device max is 40
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const lightingModes: Array<LightingMode> = []
const noneLightingMode = new LightingMode('none', 'None', 0, 0, false, false, LightingModeType.NONE)
lightingModes.push(noneLightingMode)
const lightingSpeeds: Array<string> = []
for (const device of deviceStore.allDevices()) {
    if (device.uid != props.deviceId) {
        continue
    }
    for (const mode of device.info?.channels.get(props.name)?.lighting_modes ?? []) {
        lightingModes.push(mode)
    }
    for (const speed of device.info?.lighting_speeds ?? []) {
        lightingSpeeds.push(speed)
    }
}

let startingMode: LightingMode = noneLightingMode
let startingSpeed: string =
    lightingSpeeds.length === 0
        ? 'none'
        : lightingSpeeds.length === 1
          ? lightingSpeeds[0]
          : lightingSpeeds.length === 5
            ? lightingSpeeds[2]
            : lightingSpeeds[Math.floor(lightingSpeeds.length / 2)]
let startingBackwardEnabled = false
let startingNumberOfColors: number = 0
let colorsUI: Array<Ref<string>> = []
const startingDeviceSetting: DeviceSettingReadDTO | undefined =
    settingsStore.allDaemonDeviceSettings.get(props.deviceId)?.settings.get(props.name)
if (startingDeviceSetting?.lighting != null) {
    startingMode =
        lightingModes.find(
            (mode: LightingMode) => mode.name === startingDeviceSetting.lighting?.mode,
        ) ?? noneLightingMode
    if (startingMode.speed_enabled) {
        startingSpeed =
            lightingSpeeds.find(
                (speed: string) => speed === startingDeviceSetting.lighting?.speed,
            ) ?? startingSpeed
    }
    if (startingMode.backward_enabled) {
        startingBackwardEnabled = startingDeviceSetting.lighting.backward ?? false
    }
    if (startingMode.max_colors > 0) {
        startingNumberOfColors =
            startingDeviceSetting.lighting.colors.length ?? startingNumberOfColors
        for (const rgbTuple of startingDeviceSetting.lighting.colors) {
            colorsUI.push(ref(`rgb(${rgbTuple[0]}, ${rgbTuple[1]}, ${rgbTuple[2]})`))
        }
    }
}
for (let i = 0; i < absoluteMaxColors - startingMode.max_colors; i++) {
    colorsUI.push(ref('rgb(255, 255, 255)')) // default LED color is white
}

const selectedMode: Ref<LightingMode> = ref(startingMode)
const selectedSpeed: Ref<string> = ref(startingSpeed)
const selectedBackwardEnabled: Ref<boolean> = ref(startingBackwardEnabled)
const selectedNumberOfColors: Ref<number> = ref(startingNumberOfColors)

const colorsToShow = computed(() => {
    return colorsUI.slice(0, selectedNumberOfColors.value)
})

const setNewColor = (colorIndex: number, newColor: string | null) => {
    if (newColor == null) {
        if (
            startingDeviceSetting?.lighting != null &&
            startingDeviceSetting.lighting.colors.length > colorIndex
        ) {
            const rgbTuple = startingDeviceSetting.lighting.colors[colorIndex]
            colorsUI[colorIndex].value = `rgb(${rgbTuple[0]}, ${rgbTuple[1]}, ${rgbTuple[2]})`
        } else {
            colorsUI[colorIndex].value = 'rbg(255, 255, 255)'
        }
    }
}

const parseRgbString = (rgbColor: string): [number, number, number] => {
    const matchArray = rgbColor.match(/\d{1,3}/g)
    if (matchArray?.length != 3) {
        console.error(`Invalid rgb value: ${rgbColor}`)
        return [255, 255, 255]
    }
    const rbg: Array<number> = matchArray.map((value: string) => Number(value))
    return [rbg[0], rbg[1], rbg[2]]
}

const saveLighting = async (): Promise<void> => {
    if (selectedMode.value.type === LightingModeType.NONE) {
        return await settingsStore.saveDaemonDeviceSettingReset(props.deviceId, props.name)
    }
    const setting = new DeviceSettingWriteLightingDTO(selectedMode.value.name)
    if (selectedMode.value.speed_enabled) {
        setting.speed = selectedSpeed.value
    }
    if (selectedMode.value.backward_enabled) {
        setting.backward = selectedBackwardEnabled.value
    }
    if (selectedMode.value.max_colors > 0) {
        for (let i = 0; i < selectedNumberOfColors.value; i++) {
            setting.colors.push(parseRgbString(colorsUI[i].value))
        }
    }
    await settingsStore.saveDaemonDeviceSettingLighting(props.deviceId, props.name, setting)
}

const numberColorsScrolled = (event: WheelEvent): void => {
    if (event.deltaY < 0) {
        if (selectedNumberOfColors.value < selectedMode.value.max_colors)
            selectedNumberOfColors.value += 1
    } else {
        if (selectedNumberOfColors.value > selectedMode.value.min_colors)
            selectedNumberOfColors.value -= 1
    }
}
const addScrollEventListeners = () => {
    // @ts-ignore
    document?.querySelector('.number-colors-input')?.addEventListener('wheel', numberColorsScrolled)
}

watch(selectedMode, () => {
    if (selectedMode.value.max_colors > 0) {
        if (selectedMode.value.max_colors === selectedMode.value.min_colors) {
            selectedNumberOfColors.value = selectedMode.value.max_colors
        } else {
            selectedNumberOfColors.value = Math.max(
                Math.min(selectedNumberOfColors.value, selectedMode.value.max_colors),
                selectedMode.value.min_colors,
            )
        }
    } else {
        selectedNumberOfColors.value = 0
    }
})

onMounted(() => {
    addScrollEventListeners()
    watch(selectedMode, () => {
        nextTick(addScrollEventListeners)
    })
})
</script>

<template>
    <div class="card pt-3">
        <div class="flex">
            <div class="flex-inline control-column">
                <div class="p-float-label mt-4">
                    <Dropdown
                        v-model="selectedMode"
                        inputId="dd-lighting-mode"
                        :options="lightingModes"
                        option-label="frontend_name"
                        placeholder="Mode"
                        class="w-full"
                        scroll-height="400px"
                    />
                    <label for="dd-lighting-mode">Lighting Mode</label>
                </div>
                <div v-if="selectedMode.speed_enabled" class="p-float-label mt-6">
                    <Dropdown
                        v-model="selectedSpeed"
                        :options="lightingSpeeds"
                        class="w-full"
                        :option-label="(value: string) => deviceStore.toTitleCase(value)"
                        scroll-height="400px"
                        placeholder="Speed"
                    />
                    <label for="dd-speed">Speed</label>
                </div>
                <div v-if="selectedMode.backward_enabled" class="mt-5">
                    <div class="direction-label-wrapper">
                        <label for="direction">Direction</label>
                    </div>
                    <ToggleButton
                        v-model="selectedBackwardEnabled"
                        on-label="Backward"
                        off-label="Forward"
                        class="w-full"
                    />
                </div>
                <div v-if="selectedMode.max_colors > 0" class="p-float-label mt-6">
                    <InputNumber
                        v-model="selectedNumberOfColors"
                        showButtons
                        buttonLayout="horizontal"
                        class="number-colors-input w-full"
                        :min="selectedMode.min_colors"
                        :max="selectedMode.max_colors"
                        :input-style="{ width: '58px' }"
                        incrementButtonIcon="pi pi-plus"
                        decrementButtonIcon="pi pi-minus"
                    />
                    <label for="dd-number-of-colors">Number of Colors</label>
                </div>
                <div class="mt-8">
                    <Button label="Apply" class="w-full" @click="saveLighting">
                        <span class="p-button-label">Apply</span>
                    </Button>
                </div>
            </div>
            <div v-if="selectedMode.max_colors > 0" class="flex-1 text-center mt-4 color-wrapper">
                <el-color-picker
                    v-for="(color, index) in colorsToShow"
                    :key="index"
                    v-model="color.value"
                    color-format="rgb"
                    @change="(newColor: string | null) => setNewColor(index, newColor)"
                    :predefine="settingsStore.predefinedColorOptions"
                />
            </div>
        </div>
    </div>
</template>

<style scoped lang="scss">
.control-column {
    width: 14rem;
    padding-right: 1rem;
}

.direction-label-wrapper {
    margin-left: 0.75rem;
    margin-bottom: 0.25rem;
    padding: 0;
    font-size: 0.75rem;
    color: var(--text-color-secondary);
}

.color-wrapper :deep(.el-color-picker__trigger) {
    border: 0 !important;
    padding: 1rem !important;
    height: 10rem !important;
    width: 10rem !important;
}

.color-wrapper :deep(.el-color-picker__mask) {
    border: 0 !important;
    padding: 1rem !important;
    height: 10rem !important;
    width: 10rem !important;
    top: 0;
    left: 0;
    background-color: rgba(0, 0, 0, 0.7);
}

.color-wrapper :deep(.el-color-picker__color) {
    border: 0 !important;
    //border-radius: 10px !important;
}

.color-wrapper :deep(.el-color-picker__color-inner) {
    border-radius: 4px !important;
}

.color-wrapper :deep(.el-color-picker .el-color-picker__icon) {
    display: none;
}
</style>

<style>
.el-color-picker__panel {
    padding: 1rem;
    border-radius: 12px;
    background-color: var(--surface-card);
}

.el-color-picker__panel.el-popper {
    border-color: var(--surface-border);
}

.el-button {
    border-color: var(--surface-border);
    background-color: var(--cc-bg-two);
}

el-button:focus,
.el-button:hover {
    color: var(--text-color);
    border-color: var(--surface-border);
    background-color: var(--cc-bg-three);
}

.el-button.is-text:not(.is-disabled):focus,
.el-button.is-text:not(.is-disabled):hover {
    background-color: var(--surface-card);
}

.el-input__wrapper {
    background-color: var(--cc-bg-three);
    box-shadow: none;
}

.el-input__inner {
    color: var(--text-color-secondary);
}

.el-input__wrapper:hover,
.el-input__wrapper:active,
.el-input__wrapper:focus {
    box-shadow: var(--cc-context-color);
}
</style>
