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
import Dropdown from 'primevue/dropdown'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiFileImageOutline, mdiChip } from '@mdi/js'
import Button from 'primevue/button'
import InputNumber from 'primevue/inputnumber'
import Slider from 'primevue/slider'
import FileUpload, { type FileUploadUploaderEvent } from 'primevue/fileupload'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useDeviceStore } from '@/stores/DeviceStore'
import type { UID } from '@/models/Device'
import { nextTick, onMounted, onUnmounted, type Ref, ref, watch } from 'vue'
import { LcdMode, LcdModeType } from '@/models/LcdMode'
import { DeviceSettingReadDTO, DeviceSettingWriteLcdDTO, TempSource } from '@/models/DaemonSettings'
import { useToast } from 'primevue/usetoast'
import { ErrorResponse } from '@/models/ErrorResponse'
import { storeToRefs } from 'pinia'

interface Props {
    deviceId: UID
    name: string
}

const props = defineProps<Props>()

interface AvailableTemp {
    deviceUID: string // needed here as well for the dropdown selector
    tempName: string
    tempFrontendName: string
    lineColor: string
    temp: string
}

interface AvailableTempSources {
    deviceUID: string
    deviceName: string
    temps: Array<AvailableTemp>
}

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const toast = useToast()
const { currentDeviceStatus } = storeToRefs(deviceStore)

let imageWidth: number = 320
let imageSizeMaxBytes: number = 10_000_000
for (const device of deviceStore.allDevices()) {
    if (device.uid === props.deviceId && device.info != null) {
        const channelInfo = device.info.channels.get(props.name)
        if (channelInfo != null && channelInfo.lcd_info != null) {
            imageWidth = channelInfo.lcd_info.screen_width
            imageSizeMaxBytes = channelInfo.lcd_info.max_image_size_bytes * 2 // we double it so that processing can reduce it further
        }
    }
}
const lcdModes: Array<LcdMode> = []
const noneLcdMode = new LcdMode('none', 'None', false, false, false, 0, 0, LcdModeType.NONE)
lcdModes.push(noneLcdMode)
for (const device of deviceStore.allDevices()) {
    if (device.uid != props.deviceId) {
        continue
    }
    for (const mode of device.info?.channels.get(props.name)?.lcd_modes ?? []) {
        lcdModes.push(mode)
    }
}

const tempSources: Ref<Array<AvailableTempSources>> = ref([])
const fillTempSources = () => {
    tempSources.value.length = 0
    for (const device of deviceStore.allDevices()) {
        if (device.status.temps.length === 0 || device.info == null) {
            continue
        }
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
        const deviceSource: AvailableTempSources = {
            deviceUID: device.uid,
            deviceName: deviceSettings.name,
            temps: [],
        }
        for (const temp of device.status.temps) {
            if (deviceSettings.sensorsAndChannels.get(temp.name)!.hide) {
                continue
            }
            deviceSource.temps.push({
                deviceUID: device.uid,
                tempName: temp.name,
                tempFrontendName: deviceSettings.sensorsAndChannels.get(temp.name)!.name,
                lineColor: deviceSettings.sensorsAndChannels.get(temp.name)!.color,
                temp: temp.temp.toFixed(1),
            })
        }
        if (deviceSource.temps.length === 0) {
            continue // when all of a devices temps are hidden
        }
        tempSources.value.push(deviceSource)
    }
}
fillTempSources()
let startingLcdMode: LcdMode = noneLcdMode
let startingBrightness: number = 50
let startingOrientation: number = 0
let startingTempSource: AvailableTemp | undefined = undefined
let startingImagePath: string | undefined = undefined
const startingDeviceSetting: DeviceSettingReadDTO | undefined =
    settingsStore.allDaemonDeviceSettings.get(props.deviceId)?.settings.get(props.name)
if (startingDeviceSetting?.lcd != null) {
    startingLcdMode =
        lcdModes.find((lcdMode: LcdMode) => lcdMode.name === startingDeviceSetting.lcd?.mode) ??
        noneLcdMode
    startingBrightness = startingDeviceSetting.lcd.brightness ?? startingBrightness
    startingOrientation = startingDeviceSetting.lcd.orientation ?? startingOrientation
    startingImagePath = startingDeviceSetting.lcd.image_file_processed
    const savedTempSource: TempSource | undefined = startingDeviceSetting.lcd.temp_source
    if (savedTempSource != null) {
        outer: for (const tempDevice of tempSources.value) {
            for (const tempSource of tempDevice.temps) {
                if (
                    tempSource.deviceUID === savedTempSource.device_uid &&
                    tempSource.tempName === savedTempSource.temp_name
                ) {
                    startingTempSource = tempSource
                    break outer
                }
            }
        }
    }
}

const selectedLcdMode: Ref<LcdMode> = ref(startingLcdMode)
const selectedBrightness: Ref<number> = ref(startingBrightness)
const selectedOrientation: Ref<number> = ref(startingOrientation)
const chosenTemp: Ref<AvailableTemp | undefined> = ref(startingTempSource)
const files: Array<File> = []
const fileDataURLs: Ref<Array<string>> = ref([])

/**
 * We intercept the automatic uploader to handle our custom logic here
 * @param event
 */
const filesChosen = async (event: FileUploadUploaderEvent): Promise<void> => {
    if (!Array.isArray(event.files) || event.files.length === 0) {
        console.error('No File attached to the uploader')
        return
    }
    files.length = 0
    fileDataURLs.value.length = 0
    for (const chosenFile of event.files) {
        validateFileSize(chosenFile)
        validateFileType(chosenFile)
    }
    const response: File | ErrorResponse = await deviceStore.daemonClient.processLcdImageFiles(
        props.deviceId,
        props.name,
        event.files,
    )
    if (response instanceof ErrorResponse) {
        console.error(response.error)
        toast.add({ severity: 'error', summary: 'Error', detail: response.error, life: 10_000 })
        return
    }
    fileDataURLs.value.push(URL.createObjectURL(response))
    files.push(response)
}

const validateFileSize = (file: File): void => {
    if (file.size <= imageSizeMaxBytes) {
        return
    }
    console.error('Image is too large for the LCD Screen memory')
    toast.add({
        severity: 'error',
        summary: 'Error',
        detail: 'Image is too large. Please choose a smaller one.',
        life: 4000,
    })
}

const validateFileType = (file: File): void => {
    if (file.type.startsWith('image/')) {
        return
    }
    console.error('File is not an image')
    toast.add({
        severity: 'error',
        summary: 'Error',
        detail: 'Image does not register as an image type',
        life: 4000,
    })
}

const saveLCDSetting = async () => {
    if (selectedLcdMode.value.type === LcdModeType.NONE) {
        return await settingsStore.saveDaemonDeviceSettingReset(props.deviceId, props.name)
    }
    const setting = new DeviceSettingWriteLcdDTO(selectedLcdMode.value.name)
    if (selectedLcdMode.value.brightness) {
        setting.brightness = selectedBrightness.value
    }
    if (selectedLcdMode.value.orientation) {
        setting.orientation = selectedOrientation.value
    }
    if (setting.mode === 'temp' && chosenTemp.value != null) {
        setting.temp_source = new TempSource(chosenTemp.value.deviceUID, chosenTemp.value.tempName)
    }

    if (setting.mode === 'image' && files.length > 0) {
        await settingsStore.saveDaemonDeviceSettingLcdImages(
            props.deviceId,
            props.name,
            setting,
            files,
        )
    } else {
        await settingsStore.saveDaemonDeviceSettingLcd(props.deviceId, props.name, setting)
    }
}

const updateTemps = () => {
    for (const tempDevice of tempSources.value) {
        for (const availableTemp of tempDevice.temps) {
            availableTemp.temp =
                currentDeviceStatus.value.get(availableTemp.deviceUID)!.get(availableTemp.tempName)!
                    .temp || '0.0'
        }
    }
}

const brightnessScrolled = (event: WheelEvent): void => {
    // if (selectedBrightness.value == null) return
    if (event.deltaY < 0) {
        if (selectedBrightness.value < 100) selectedBrightness.value += 1
    } else {
        if (selectedBrightness.value > 0) selectedBrightness.value -= 1
    }
}
const orientationScrolled = (event: WheelEvent): void => {
    if (event.deltaY < 0) {
        if (selectedOrientation.value < 270) selectedOrientation.value += 90
    } else {
        if (selectedOrientation.value > 0) selectedOrientation.value -= 90
    }
}
const addScrollEventListeners = () => {
    // @ts-ignore
    document.querySelector('.brightness-input')?.addEventListener('wheel', brightnessScrolled)
    // @ts-ignore
    document.querySelector('.orientation-input')?.addEventListener('wheel', orientationScrolled)
}

watch(fileDataURLs.value, () => {
    if (fileDataURLs.value.length === 0) {
        return
    } else if (fileDataURLs.value.length > 1) {
        // future feature that requires a Timer
        return
    }
    const img: HTMLImageElement =
        (document.getElementById('lcd-image') as HTMLImageElement) ?? new HTMLImageElement()
    img.src = fileDataURLs.value[0]
})

onMounted(async () => {
    if (startingLcdMode.name === 'image' && startingImagePath != null) {
        const response: File | ErrorResponse =
            await deviceStore.daemonClient.getDeviceSettingLcdImage(props.deviceId, props.name)
        if (response instanceof ErrorResponse) {
            console.error(response.error)
        } else {
            fileDataURLs.value.push(URL.createObjectURL(response))
            files.push(response)
        }
    }
    watch(currentDeviceStatus, () => {
        updateTemps()
    })
    watch(settingsStore.allUIDeviceSettings, () => {
        fillTempSources()
    })

    addScrollEventListeners()
    watch(selectedLcdMode, (): void => {
        nextTick(addScrollEventListeners)
    })
})

onUnmounted(() => {
    for (const dataURL of fileDataURLs.value) {
        // make sure these can be garbage collected:
        URL.revokeObjectURL(dataURL)
    }
})
</script>

<template>
    <div class="card pt-3">
        <div class="flex">
            <div class="flex-inline control-column">
                <div class="p-float-label mt-4">
                    <Dropdown
                        v-model="selectedLcdMode"
                        inputId="dd-lcd-mode"
                        :options="lcdModes"
                        option-label="frontend_name"
                        placeholder="Mode"
                        class="w-full"
                        scroll-height="400px"
                    />
                    <label for="dd-lcd-mode">LCD Mode</label>
                </div>
                <div v-if="selectedLcdMode.brightness" class="brightness-input p-float-label mt-6">
                    <InputNumber
                        placeholder="Brightness"
                        v-model="selectedBrightness"
                        inputId="dd-brightness"
                        mode="decimal"
                        class="w-full"
                        suffix="%"
                        :step="1"
                        :input-style="{ width: '60px' }"
                        :min="0"
                        :max="100"
                    />
                    <Slider
                        v-model="selectedBrightness"
                        :step="1"
                        :min="0"
                        :max="100"
                        class="w-full mt-0"
                    />
                    <label for="dd-brightness">Brightness</label>
                </div>
                <div
                    v-if="selectedLcdMode.orientation"
                    class="orientation-input p-float-label mt-6"
                >
                    <InputNumber
                        placeholder="Orientation"
                        v-model="selectedOrientation"
                        inputId="dd-orientation"
                        mode="decimal"
                        class="w-full"
                        suffix="°"
                        :step="90"
                        :input-style="{ width: '60px' }"
                        :min="0"
                        :max="270"
                        readonly
                    />
                    <Slider
                        v-model="selectedOrientation"
                        :step="90"
                        :min="0"
                        :max="270"
                        class="w-full mt-0"
                    />
                    <label for="dd-brightness">Orientation</label>
                </div>
                <div
                    v-if="
                        selectedLcdMode.type === LcdModeType.CUSTOM &&
                        selectedLcdMode.name === 'temp'
                    "
                    class="p-float-label mt-6"
                >
                    <Dropdown
                        v-model="chosenTemp"
                        inputId="dd-temp-source"
                        :options="tempSources"
                        option-label="tempFrontendName"
                        option-group-label="deviceName"
                        option-group-children="temps"
                        placeholder="Temp Source"
                        class="w-full"
                        scroll-height="400px"
                    >
                        <template #optiongroup="slotProps">
                            <div class="flex align-items-center">
                                <svg-icon
                                    type="mdi"
                                    :path="mdiChip"
                                    :size="deviceStore.getREMSize(1.3)"
                                    class="mr-2"
                                />
                                <div>{{ slotProps.option.deviceName }}</div>
                            </div>
                        </template>
                        <template #option="slotProps">
                            <div class="flex align-items-center w-full justify-content-between">
                                <div>
                                    <span
                                        class="pi pi-minus mr-2 ml-1"
                                        :style="{ color: slotProps.option.lineColor }"
                                    />{{ slotProps.option.tempFrontendName }}
                                </div>
                                <div>
                                    {{ slotProps.option.temp + ' °' }}
                                </div>
                            </div>
                        </template>
                    </Dropdown>
                    <label for="dd-temp-source">Temp Source</label>
                </div>
                <div v-if="selectedLcdMode.image" class="p-float-label mt-6">
                    <FileUpload
                        mode="basic"
                        accept="image/jpeg,image/png,image/gif,image/tiff,image/bmp"
                        :maxFileSize="imageSizeMaxBytes"
                        chooseLabel="Choose Image"
                        :show-cancel-button="false"
                        upload-label="Process"
                        :multiple="false"
                        auto
                        class="w-full"
                        customUpload
                        @uploader="filesChosen"
                    >
                        <template #empty>
                            <p>Drag and drop files to here.</p>
                        </template>
                    </FileUpload>
                </div>
                <div class="align-content-end">
                    <div class="mt-7">
                        <Button label="Apply" class="w-full" @click="saveLCDSetting">
                            <span class="p-button-label">Apply</span>
                        </Button>
                    </div>
                </div>
            </div>
            <div v-if="selectedLcdMode.image" class="flex-1 text-center mt-3">
                <img
                    v-if="fileDataURLs.length > 0"
                    :src="fileDataURLs[0]"
                    id="lcd-image"
                    alt="LCD Image"
                />
                <svg-icon
                    v-else
                    id="lcd-image"
                    style="padding: 40px"
                    type="mdi"
                    :path="mdiFileImageOutline"
                    :size="imageWidth + 60"
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

#lcd-image {
    border-radius: 50%;
    border-color: var(--cc-dark-one);
    border-width: 30px;
    border-style: solid;
    background: var(--cc-dark-one);
}
</style>
