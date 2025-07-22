<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2021-2025  Guy Boldon and contributors
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
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiPalette } from '@mdi/js'
import { Color, UID } from '@/models/Device.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { ChromePicker, CompactPicker } from 'vue-color'
import Button from 'primevue/button'
import InputText from 'primevue/inputtext'
import Popover from 'primevue/popover'
import { ref, Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore.ts'

//! This is our own custom color picker component, which is similar to the ElColorPicker component,
//! but with more custom control and uses the PrimeVue color picker as a basis.
interface Props {
    deviceUID: UID
    channelName: string
    color: Color
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'colorChange', value: Color): void
    (e: 'open', value: boolean): void
}>()

const settingsStore = useSettingsStore()
const deviceStore = useDeviceStore()
const colorStore = useThemeColorsStore()
const { t } = useI18n()

const currentColor: Ref<Color> = ref(colorStore.RgbToHex(props.color))
let newColorApplied: boolean = false

const popRef = ref()
const saveButton = ref()

const closeAndReset = (): void => {
    settingsStore.allUIDeviceSettings
        .get(props.deviceUID)!
        .sensorsAndChannels.get(props.channelName)!.userColor = undefined
    const defaultColor = colorStore.RgbToHex(
        settingsStore.allUIDeviceSettings
            .get(props.deviceUID)!
            .sensorsAndChannels.get(props.channelName)!.defaultColor,
    )
    currentColor.value = defaultColor
    emit('colorChange', defaultColor)
    newColorApplied = true
    popRef.value.hide()
}
const clickSaveButton = (): void => saveButton.value.$el.click()
const closeAndSave = (): void => {
    const newColor = currentColor.value
    settingsStore.allUIDeviceSettings
        .get(props.deviceUID)!
        .sensorsAndChannels.get(props.channelName)!.userColor = newColor
    emit('colorChange', newColor)
    newColorApplied = true
    popRef.value.hide()
}

const popoverClose = (): void => {
    // hide from the above buttons also triggers this:
    if (!newColorApplied) {
        // reset to starting color
        currentColor.value = colorStore.RgbToHex(props.color)
    }
    newColorApplied = false
    emit('open', false)
}
</script>

<template>
    <div v-tooltip.top="{ value: t('layout.menu.tooltips.chooseColor') }">
        <div
            class="rounded-lg w-8 h-8 border-none p-0 text-text-color-secondary outline-0 text-center justify-center items-center flex hover:text-text-color hover:bg-surface-hover"
            @click.stop.prevent="(event) => popRef.toggle(event)"
        >
            <svg-icon
                class="outline-0"
                type="mdi"
                :path="mdiPalette"
                :size="deviceStore.getREMSize(1.5)"
                :style="{ color: props.color }"
            />
        </div>
        <Popover ref="popRef" @show="emit('open', true)" @hide="popoverClose">
            <div
                class="mt-2 w-full bg-bg-two border border-border-one p-4 rounded-lg text-text-color"
            >
                <div>
                    <ChromePicker
                        v-model="currentColor"
                        disable-alpha
                        disable-fields
                        class="!w-[32rem]"
                    />
                    <CompactPicker
                        v-model="currentColor"
                        class="!w-[32rem]"
                        :palette="settingsStore.predefinedColorOptions"
                    />
                </div>
                <div class="flex flex-row justify-between mt-4 w-full">
                    <InputText
                        ref="inputArea"
                        id="property-color"
                        class="w-20rem"
                        :invalid="!colorStore.isValidHex(currentColor)"
                        v-model="currentColor"
                        @keydown.enter.prevent="clickSaveButton"
                        autofocus
                    />
                    <div class="text-right justify-end">
                        <Button class="mr-4" label="Reset" @click="closeAndReset">
                            {{ t('common.reset') }}
                        </Button>
                        <Button
                            ref="saveButton"
                            class="!bg-accent/80 hover:!bg-accent/100"
                            label="Save"
                            @click="closeAndSave"
                            :disabled="!colorStore.isValidHex(currentColor)"
                        >
                            {{ t('common.save') }}
                        </Button>
                    </div>
                </div>
            </div>
        </Popover>
    </div>
</template>

<style lang="scss">
.vc-chrome-picker {
    box-shadow: none !important;
    --vc-body-bg: rgb(var(--colors-bg-two));
}

.vc-chrome-picker .active-color {
    width: 2rem !important;
    height: 2rem !important;
    border-radius: 1rem !important;
}

.vc-chrome-picker .color-wrap {
    width: 3rem !important;
    height: 2rem !important;
}

.vc-chrome-picker .hue-wrap {
    margin-top: 0.625rem !important;
}

.vc-compact-picker {
    box-shadow: none !important;
    justify-items: center;
    --vc-body-bg: rgb(var(--colors-bg-two));
}

.vc-compact-picker .color-item {
    width: 2rem !important;
    height: 2rem !important;
    margin-right: 1rem !important;
    margin-bottom: 0.5rem !important;
    border-radius: 0.5rem !important;
}
</style>
