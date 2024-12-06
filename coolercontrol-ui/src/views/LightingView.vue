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
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { type UID } from '@/models/Device'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import { DeviceSettingReadDTO, DeviceSettingWriteLightingDTO } from '@/models/DaemonSettings'
import { LightingMode, LightingModeType } from '@/models/LightingMode'
import { computed, nextTick, onMounted, ref, type Ref, watch } from 'vue'
import InputNumber from 'primevue/inputnumber'
import { ElColorPicker, ElSwitch } from 'element-plus'
import 'element-plus/es/components/color-picker/style/css'
import Button from 'primevue/button'
import { mdiContentSaveOutline } from '@mdi/js'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'radix-vue'
import Select from 'primevue/select'
import Listbox, { ListboxChangeEvent } from 'primevue/listbox'
import Slider from 'primevue/slider'
import { onBeforeRouteLeave, onBeforeRouteUpdate } from 'vue-router'
import { useConfirm } from 'primevue/useconfirm'

interface Props {
    deviceId: UID
    channelName: string
}

const props = defineProps<Props>()

const absoluteMaxColors = 48 // Current device max is 40
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const confirm = useConfirm()

let contextIsDirty: boolean = false
const lightingModes: Array<LightingMode> = []
const noneLightingMode = new LightingMode('none', 'None', 0, 0, false, false, LightingModeType.NONE)
lightingModes.push(noneLightingMode)
const lightingSpeeds: Array<string> = []
for (const device of deviceStore.allDevices()) {
    if (device.uid != props.deviceId) {
        continue
    }
    for (const mode of device.info?.channels.get(props.channelName)?.lighting_modes ?? []) {
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
    settingsStore.allDaemonDeviceSettings.get(props.deviceId)?.settings.get(props.channelName)
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
            colorsUI[colorIndex].value = 'rgb(255, 255, 255)'
        }
    } else {
        if (startingDeviceSetting?.lighting == null) {
            return
        }
        const newRgbTuple = parseRgbString(newColor)
        colorsUI[colorIndex].value = `rgb(${newRgbTuple[0]}, ${newRgbTuple[1]}, ${newRgbTuple[2]})`
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
        await settingsStore.saveDaemonDeviceSettingReset(props.deviceId, props.channelName)
        contextIsDirty = false
        return
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
    await settingsStore.saveDaemonDeviceSettingLighting(props.deviceId, props.channelName, setting)
    contextIsDirty = false
}

const changeLightingSpeed = (event: ListboxChangeEvent): void => {
    if (event.value === null) {
        return // do not update on unselect
    }
    selectedSpeed.value = event.value
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

const checkForUnsavedChanges = (_to: any, _from: any, next: any): void => {
    if (!contextIsDirty) {
        next()
        return
    }
    confirm.require({
        message: 'There are unsaved changes made to these Lighting Settings.',
        header: 'Unsaved Changes',
        icon: 'pi pi-exclamation-triangle',
        defaultFocus: 'accept',
        rejectLabel: 'Stay',
        acceptLabel: 'Discard',
        accept: () => {
            next()
            contextIsDirty = false
        },
        reject: () => next(false),
    })
}

onMounted(() => {
    onBeforeRouteUpdate(checkForUnsavedChanges)
    onBeforeRouteLeave(checkForUnsavedChanges)
    addScrollEventListeners()
    watch(selectedMode, () => {
        nextTick(addScrollEventListeners)
    })
    watch([selectedMode, selectedSpeed, selectedBackwardEnabled, selectedNumberOfColors], () => {
        contextIsDirty = true
    })
})
</script>

<template>
    <div class="flex border-b-4 border-border-one items-center justify-between">
        <div class="pl-4 py-2 text-2xl">{{ props.channelName.toUpperCase() }}</div>
        <div class="flex justify-end">
            <div class="border-l-2 px-4 py-2 border-border-one flex flex-row">
                <Button
                    class="bg-accent/80 hover:!bg-accent w-32 h-[2.375rem]"
                    label="Save"
                    v-tooltip.bottom="'Save LCD Settings'"
                    @click="saveLighting"
                >
                    <svg-icon
                        class="outline-0"
                        type="mdi"
                        :path="mdiContentSaveOutline"
                        :size="deviceStore.getREMSize(1.5)"
                    />
                </Button>
            </div>
        </div>
    </div>
    <ScrollAreaRoot style="--scrollbar-size: 10px">
        <ScrollAreaViewport class="p-4 pb-16 h-screen w-full">
            <div class="w-full flex flex-col lg:flex-row">
                <div id="left-side">
                    <div class="mt-0 mr-4 w-96">
                        <small class="ml-3 font-light text-sm text-text-color-secondary">
                            Lighting Mode<br />
                        </small>
                        <Select
                            v-model="selectedMode"
                            :options="lightingModes"
                            option-label="frontend_name"
                            placeholder="Mode"
                            class="w-full mt-1"
                            dropdown-icon="pi pi-sun"
                            scroll-height="40rem"
                            v-tooltip.bottom="'Lighting Mode'"
                            filter
                            size="large"
                            variant="filled"
                        />
                    </div>
                    <div v-if="selectedMode.speed_enabled" class="mt-4 mr-4 w-96">
                        <small class="ml-3 font-light text-sm text-text-color-secondary">
                            Speed
                        </small>
                        <Listbox
                            :model-value="selectedSpeed"
                            :options="lightingSpeeds"
                            :option-label="(value: string) => deviceStore.toTitleCase(value)"
                            class="w-full"
                            checkmark
                            placeholder="Speed"
                            list-style="max-height: 100%"
                            v-tooltip.right="'The speed of the Lighting Mode'"
                            @change="changeLightingSpeed"
                        />
                    </div>
                    <div v-if="selectedMode.backward_enabled" class="mt-4 mr-4 w-96">
                        <small class="ml-3 font-light text-sm text-text-color-secondary">
                            Direction<br />
                        </small>
                        <div
                            class="bg-bg-two border border-border-one p-1 rounded-lg text-center items-center"
                        >
                            <el-switch
                                v-model="selectedBackwardEnabled"
                                size="large"
                                active-text="Backward"
                                inactive-text="Forward"
                                style="--el-switch-off-color: rgb(var(--colors-accent))"
                            />
                        </div>
                    </div>
                    <div
                        v-if="selectedMode.max_colors > 0"
                        class="mt-4 mr-4 w-96 border-border-one"
                    >
                        <small class="ml-3 font-light text-sm text-text-color-secondary">
                            Number of Colors<br />
                        </small>
                        <InputNumber
                            placeholder="Number of Colors"
                            v-model="selectedNumberOfColors"
                            mode="decimal"
                            class="mt-0.5 w-full"
                            showButtons
                            :min="selectedMode.min_colors"
                            :max="selectedMode.max_colors"
                            :use-grouping="false"
                            :step="1"
                            button-layout="horizontal"
                            :input-style="{ width: '8rem' }"
                            v-tooltip.bottom="
                                'Number of Colors to use for the chosen Lighting Mode.'
                            "
                            :disabled="selectedMode.min_colors == selectedMode.max_colors"
                        >
                            <template #incrementicon>
                                <span class="pi pi-plus" />
                            </template>
                            <template #decrementicon>
                                <span class="pi pi-minus" />
                            </template>
                        </InputNumber>
                        <Slider
                            v-model="selectedNumberOfColors"
                            class="!w-[23.25rem] ml-1.5"
                            :step="1"
                            :min="selectedMode.min_colors"
                            :max="selectedMode.max_colors"
                            :disabled="selectedMode.min_colors == selectedMode.max_colors"
                        />
                    </div>
                </div>
                <div id="right-side" v-if="selectedMode.max_colors > 0" class="flex mt-4 ml-1">
                    <div class="content-center flex justify-center">
                        <div class="color-wrapper mt-1">
                            <el-color-picker
                                v-for="(color, index) in colorsToShow"
                                class="m-2"
                                :key="index"
                                v-model="color.value"
                                color-format="rgb"
                                :predefine="settingsStore.predefinedColorOptions"
                                :validate-event="false"
                                @change="(newColor: string | null) => setNewColor(index, newColor)"
                            />
                        </div>
                    </div>
                </div>
            </div>
        </ScrollAreaViewport>
        <ScrollAreaScrollbar
            class="flex select-none touch-none p-0.5 bg-transparent transition-colors duration-[120ms] ease-out data-[orientation=vertical]:w-2.5"
            orientation="vertical"
        >
            <ScrollAreaThumb
                class="flex-1 bg-border-one opacity-80 rounded-lg relative before:content-[''] before:absolute before:top-1/2 before:left-1/2 before:-translate-x-1/2 before:-translate-y-1/2 before:w-full before:h-full before:min-w-[44px] before:min-h-[44px]"
            />
        </ScrollAreaScrollbar>
    </ScrollAreaRoot>
</template>

<style scoped lang="scss">
.el-switch {
    --el-switch-on-color: rgb(var(--colors-accent));
    --el-switch-off-color: rgb(var(--colors-bg-one));
    --el-color-white: rgb(var(--colors-bg-two));
    // switch active text color:
    --el-color-primary: rgb(var(--colors-text-color));
    // switch inactive text color:
    --el-text-color-primary: rgb(var(--colors-text-color));
}

.color-wrapper {
    display: flex;
    flex-wrap: wrap;
    line-height: normal;
    min-width: 10rem;
}

.color-wrapper :deep(.el-color-picker__trigger) {
    border: 0 !important;
    padding: 0 !important;
    margin: 0 !important;
    height: 4rem !important;
    width: 4rem !important;
}

.color-wrapper :deep(.el-color-picker__mask) {
    border: 0 !important;
    padding: 0 !important;
    margin: 0 !important;
    height: 4rem !important;
    width: 4rem !important;
    border-radius: 0.5rem !important;
    background-color: rgba(0, 0, 0, 0);
    cursor: default;
}

.color-wrapper :deep(.el-color-picker__color) {
    border: 0 !important;
    border-radius: 0.5rem !important;
}

.color-wrapper :deep(.el-color-picker.is-disabled .el-color-picker__trigger) {
    cursor: default;
}

.color-wrapper :deep(.el-color-picker.is-disabled) {
    cursor: default;
}

.color-wrapper :deep(.el-color-picker__color-inner) {
    border-radius: 0.5rem !important;
    opacity: 1;
    width: 4rem !important;
    height: 4rem !important;
}

.color-wrapper :deep(.el-color-picker__color-inner):hover {
    opacity: 1;
}

.color-wrapper :deep(.el-color-picker .el-color-picker__icon) {
    display: none;
    height: 0;
    width: 0;
}

.color-wrapper :deep(.el-color-picker .el-color-picker__empty) {
    display: none;
    height: 0;
    width: 0;
}
</style>

<style></style>
