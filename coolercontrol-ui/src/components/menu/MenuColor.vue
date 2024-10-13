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
import { ElColorPicker } from 'element-plus'
import 'element-plus/es/components/color-picker/style/css'
import { Color, UID } from '@/models/Device.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { computed, onMounted, ref, Ref } from 'vue'

interface Props {
    deviceUID: UID
    channelName: string
    color: Color
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'colorChange', value: Color): void
    (e: 'colorReset', value: Color): void
    (e: 'open', value: boolean): void
}>()

const settingsStore = useSettingsStore()
const currentColor: Ref<Color> = ref(props.color)
const deviceChannelHidden = computed(
    (): boolean =>
        settingsStore.allUIDeviceSettings
            .get(props.deviceUID)
            ?.sensorsAndChannels.get(props.channelName)?.hide ?? false,
)

const setNewColor = (newColor: Color | null): void => {
    if (newColor == null) {
        settingsStore.allUIDeviceSettings
            .get(props.deviceUID)!
            .sensorsAndChannels.get(props.channelName)!.userColor = undefined
        const defaultColor = settingsStore.allUIDeviceSettings
            .get(props.deviceUID)!
            .sensorsAndChannels.get(props.channelName)!.defaultColor
        emit('colorReset', defaultColor)
        currentColor.value = defaultColor
    } else {
        settingsStore.allUIDeviceSettings
            .get(props.deviceUID)!
            .sensorsAndChannels.get(props.channelName)!.userColor = newColor
        emit('colorChange', newColor)
    }
}
onMounted(async () => {
    const picker_clear_elements = document.querySelectorAll(
        'div.el-color-dropdown__btns > button.el-button.el-button--small.is-text.el-color-dropdown__link-btn > span',
    )
    for (const el of picker_clear_elements) {
        el.textContent = 'Default'
    }
    const picker_ok_elements = document.querySelectorAll(
        'div.el-color-dropdown__btns > button.el-button.el-button--small.is-plain.el-color-dropdown__btn > span',
    )
    for (const el of picker_ok_elements) {
        el.textContent = 'Ok'
    }
})
</script>

<template>
    <div
        class="rounded-lg w-8 h-8 p-2 !ml-[-1px] text-center justify-center items-center flex"
        v-tooltip.top="{ value: 'Choose Color', disabled: deviceChannelHidden }"
    >
        <div class="color-wrapper">
            <el-color-picker
                :teleported="false"
                v-model="currentColor"
                color-format="hex"
                :predefine="settingsStore.predefinedColorOptions"
                :disabled="deviceChannelHidden"
                @change="setNewColor"
                :validate-event="false"
                @focus="emit('open', true)"
                @blur="emit('open', false)"
            />
        </div>
    </div>
</template>

<style scoped lang="scss">
.color-wrapper {
    line-height: normal;
    height: 1rem;
    width: 1rem;
}

.color-wrapper :deep(.el-color-picker__trigger) {
    border: 0 !important;
    padding: 0 !important;
    margin: 0 !important;
    height: 1rem !important;
    width: 1rem !important;
}

.color-wrapper :deep(.el-color-picker__mask) {
    border: 0 !important;
    padding: 0 !important;
    margin: 0 !important;
    height: 1rem !important;
    width: 1rem !important;
    border-radius: 0.5rem !important;
    background-color: rgba(0, 0, 0, 0);
    cursor: default;
}

.color-wrapper :deep(.el-color-picker__color) {
    border: 0 !important;
    border-radius: 0.5rem !important;
}

.color-wrapper :deep(.el-color-picker.is-disabled .el-color-picker__color) {
    opacity: 0.2;
}

.color-wrapper :deep(.el-color-picker.is-disabled .el-color-picker__trigger) {
    cursor: default;
}

.color-wrapper :deep(.el-color-picker.is-disabled) {
    cursor: default;
}

.color-wrapper :deep(.el-color-picker__color-inner) {
    border-radius: 0.5rem !important;
    opacity: 0.8;
    width: 1rem !important;
    height: 1rem !important;
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

<style lang="scss">
/******************************************************************************************
* Unscoped Style needed to deeply affect the element components
*/
.el-color-picker__panel {
    padding: 1rem;
    border-radius: 0.5rem;
    background-color: rgb(var(--colors-bg-two));
}

.el-color-picker__panel.el-popper {
    border-color: rgb(var(--colors-border-one));
}

.el-color-svpanel {
    width: 27rem;
    height: 16rem;
}

.el-color-hue-slider.is-vertical {
    height: 16rem;
    width: 2rem;
}

.el-color-dropdown {
    width: 30rem;
    height: 20.5rem;
}

.el-button {
    font-size: 1rem;
    color: rgb(var(--colors-text-color) / 0.5);
    border-radius: 0.5rem;
    border-color: rgb(var(--colors-border-one));
    background-color: rgb(var(--colors-accent) / 0.8);
}

el-button:focus,
.el-button:hover {
    color: rgb(var(--colors-text-color));
    border-color: rgb(var(--colors-border-one));
    background-color: rgb(var(--colors-accent));
}

.el-button.is-text:not(.is-disabled) {
    font-size: 1rem;
    border: 1px solid rgb(var(--colors-border-one));
    border-radius: 0.5rem;
}

.el-button.is-text:not(.is-disabled):focus,
.el-button.is-text:not(.is-disabled):hover {
    background-color: rgba(255, 255, 255, 0.05);
}

.el-input__wrapper {
    background-color: rgb(var(--colors-bg-one));
    box-shadow: none;
    --el-border-radius-base: 0.5rem;
    --el-input-border-radius: 0.5rem;
}

.el-input__inner {
    font-size: 1rem;
    color: rgb(var(--colors-text-color-secondary));
}

.el-input__wrapper:hover,
.el-input__wrapper:active,
.el-input__wrapper:focus {
    //box-shadow: rgb(var(--colors-text-color));
    box-shadow: none;
}

.el-tree-node__content > .el-tree-node__expand-icon {
    padding: 0;
}

.el-color-predefine {
    width: 30rem;
}

.el-color-predefine__color-selector {
    width: 2.25rem;
    height: 1.25rem;
}

.el-color-predefine__color-selector > div {
    border-radius: 0.5rem;
}

.el-color-predefine__color-selector.selected {
    border-radius: 0.5rem;
    box-shadow: 0 0 3px 2px rgb(var(--colors-accent));
}
</style>
