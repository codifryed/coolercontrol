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
import { mdiContentSaveOutline, mdiFileImageOutline, mdiMemory } from '@mdi/js'
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
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'radix-vue'
import Listbox, { ListboxChangeEvent } from 'primevue/listbox'
import { onBeforeRouteLeave, onBeforeRouteUpdate } from 'vue-router'
import { useConfirm } from 'primevue/useconfirm'

interface Props {
    deviceId: UID
    channelName: string
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
const confirm = useConfirm()

let contextIsDirty: boolean = false
let imageWidth: number = 320
let imageSizeMaxBytes: number = 10_000_000
for (const device of deviceStore.allDevices()) {
    if (device.uid === props.deviceId && device.info != null) {
        const channelInfo = device.info.channels.get(props.channelName)
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
    for (const mode of device.info?.channels.get(props.channelName)?.lcd_modes ?? []) {
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
    settingsStore.allDaemonDeviceSettings.get(props.deviceId)?.settings.get(props.channelName)
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
        props.channelName,
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
        await settingsStore.saveDaemonDeviceSettingReset(props.deviceId, props.channelName)
        contextIsDirty = false
        return
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
            props.channelName,
            setting,
            files,
        )
    } else {
        await settingsStore.saveDaemonDeviceSettingLcd(props.deviceId, props.channelName, setting)
    }
    contextIsDirty = false
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

const changeLcdMode = (event: ListboxChangeEvent): void => {
    if (event.value === null) {
        return // do not update on unselect
    }
    selectedLcdMode.value = event.value
}

const changeTempSource = (event: ListboxChangeEvent): void => {
    if (event.value === null) {
        return // do not update on unselect
    }
    chosenTemp.value = event.value
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

const checkForUnsavedChanges = (_to: any, _from: any, next: any): void => {
    if (!contextIsDirty) {
        next()
        return
    }
    confirm.require({
        message: 'There are unsaved changes made to these LCD Settings.',
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

onMounted(async () => {
    onBeforeRouteUpdate(checkForUnsavedChanges)
    onBeforeRouteLeave(checkForUnsavedChanges)
    if (startingLcdMode.name === 'image' && startingImagePath != null) {
        const response: File | ErrorResponse =
            await deviceStore.daemonClient.getDeviceSettingLcdImage(
                props.deviceId,
                props.channelName,
            )
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
    watch(
        [fileDataURLs.value, selectedLcdMode, selectedBrightness, selectedOrientation, chosenTemp],
        () => {
            contextIsDirty = true
        },
    )

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
    <div class="flex border-b-4 border-border-one items-center justify-between">
        <div class="pl-4 py-2 text-2xl">{{ props.channelName.toUpperCase() }}</div>
        <div class="flex flex-wrap gap-x-1 justify-end">
            <div class="p-2 flex flex-row">
                <Button
                    class="bg-accent/80 hover:!bg-accent w-32 h-[2.375rem]"
                    label="Save"
                    v-tooltip.bottom="'Save LCD Settings'"
                    @click="saveLCDSetting"
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
                            LCD Mode
                        </small>
                        <Listbox
                            :model-value="selectedLcdMode"
                            :options="lcdModes"
                            option-label="frontend_name"
                            class="w-full"
                            checkmark
                            placeholder="Type"
                            list-style="max-height: 100%"
                            v-tooltip.right="'The currently available LCD Modes to choose from.'"
                            @change="changeLcdMode"
                        />
                    </div>
                    <div v-if="selectedLcdMode.brightness" class="mt-4 mr-4 w-96 border-border-one">
                        <small class="ml-3 font-light text-sm text-text-color-secondary">
                            Brightness<br />
                        </small>
                        <InputNumber
                            placeholder="Brightness"
                            v-model="selectedBrightness"
                            mode="decimal"
                            class="mt-0.5 w-full"
                            suffix="%"
                            showButtons
                            :min="0"
                            :max="100"
                            :use-grouping="false"
                            :step="1"
                            button-layout="horizontal"
                            :input-style="{ width: '8rem' }"
                            v-tooltip.bottom="'Brightness Percent'"
                        >
                            <template #incrementicon>
                                <span class="pi pi-plus" />
                            </template>
                            <template #decrementicon>
                                <span class="pi pi-minus" />
                            </template>
                        </InputNumber>
                        <Slider
                            v-model="selectedBrightness"
                            class="!w-[23.25rem] ml-1.5"
                            :step="1"
                            :min="0"
                            :max="100"
                        />
                    </div>
                    <div
                        v-if="selectedLcdMode.orientation"
                        class="mt-4 mr-4 w-96 border-border-one"
                    >
                        <small class="ml-3 font-light text-sm text-text-color-secondary">
                            Orientation<br />
                        </small>
                        <InputNumber
                            placeholder="Orientation"
                            v-model="selectedOrientation"
                            mode="decimal"
                            class="mt-0.5 w-full"
                            suffix="°"
                            showButtons
                            :min="0"
                            :max="270"
                            :use-grouping="false"
                            :step="90"
                            button-layout="horizontal"
                            :input-style="{ width: '8rem' }"
                            v-tooltip.bottom="'Orientation in degrees'"
                        >
                            <template #incrementicon>
                                <span class="pi pi-plus" />
                            </template>
                            <template #decrementicon>
                                <span class="pi pi-minus" />
                            </template>
                        </InputNumber>
                        <Slider
                            v-model="selectedOrientation"
                            class="!w-[23.25rem] ml-1.5"
                            :step="90"
                            :min="0"
                            :max="270"
                        />
                    </div>
                    <div v-if="selectedLcdMode.image" class="mt-8 mr-4 w-96 border-border-one">
                        <FileUpload
                            mode="basic"
                            accept="image/jpeg,image/png,image/gif,image/tiff,image/bmp"
                            :maxFileSize="imageSizeMaxBytes"
                            chooseLabel="Choose Image&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;"
                            choose-icon="pi pi-images"
                            :show-cancel-button="false"
                            upload-label="Process"
                            :multiple="false"
                            auto
                            class="w-full h-16"
                            customUpload
                            @uploader="filesChosen"
                        >
                            <template #empty>
                                <p>Drag and drop files to here.</p>
                            </template>
                        </FileUpload>
                    </div>
                </div>
                <div
                    id="right-side"
                    v-if="selectedLcdMode.image"
                    class="flex flex-col lg:flex-row mt-4 ml-1 w-96 h-96 min-w-96 min-h-96"
                >
                    <div class="flex w-full mr-0">
                        <img
                            v-if="fileDataURLs.length > 0"
                            :src="fileDataURLs[0]"
                            id="lcd-image"
                            alt="LCD Image"
                        />
                        <svg-icon
                            v-else
                            id="lcd-image"
                            class="text-text-color-secondary"
                            style="padding: 40px"
                            type="mdi"
                            :path="mdiFileImageOutline"
                            :size="imageWidth + 60"
                        />
                    </div>
                </div>
                <div
                    v-if="
                        selectedLcdMode.type === LcdModeType.CUSTOM &&
                        selectedLcdMode.name === 'temp'
                    "
                    class="mt-0 mr-4 w-96"
                >
                    <small class="ml-3 font-light text-sm text-text-color-secondary">
                        Temp Source
                    </small>
                    <Listbox
                        v-model="chosenTemp"
                        class="w-full mt-0"
                        :options="tempSources"
                        filter
                        checkmark
                        option-label="tempFrontendName"
                        option-group-label="deviceName"
                        option-group-children="temps"
                        filter-placeholder="Search"
                        list-style="max-height: 100%"
                        :invalid="chosenTemp == null"
                        v-tooltip.right="'Temperature source to use in LCD display.'"
                        @change="changeTempSource"
                    >
                        <template #optiongroup="slotProps">
                            <div class="flex items-center">
                                <svg-icon
                                    type="mdi"
                                    :path="mdiMemory"
                                    :size="deviceStore.getREMSize(1.3)"
                                    class="mr-2"
                                />
                                <div>{{ slotProps.option.deviceName }}</div>
                            </div>
                        </template>
                        <template #option="slotProps">
                            <div class="flex items-center w-full justify-between">
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
                    </Listbox>
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
#lcd-image {
    border-radius: 50%;
    border-color: rgb(var(--colors-bg-two));
    border-width: 1.5rem;
    border-style: solid;
    background: rgb(var(--colors-bg-one));
}
</style>
