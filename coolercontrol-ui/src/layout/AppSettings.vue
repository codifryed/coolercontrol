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
import { inject, type Ref, ref } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useConfirm } from 'primevue/useconfirm'
import { useToast } from 'primevue/usetoast'
import { defaultCustomTheme, ThemeMode } from '@/models/UISettings.ts'
import { CoolerControlDeviceSettingsDTO } from '@/models/CCSettings.ts'
import { mdiDnsOutline, mdiLaptop, mdiMonitor, mdiRestartAlert, mdiViewQuiltOutline } from '@mdi/js'
import Tabs from 'primevue/tabs'
import Tab from 'primevue/tab'
import TabList from 'primevue/tablist'
import TabPanels from 'primevue/tabpanels'
import TabPanel from 'primevue/tabpanel'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'radix-vue'
import { ElColorPicker, ElSwitch } from 'element-plus'
import 'element-plus/es/components/switch/style/css'
import 'element-plus/es/components/color-picker/style/css'
import Listbox, { ListboxChangeEvent } from 'primevue/listbox'
import Select from 'primevue/select'
import InputNumber from 'primevue/inputnumber'
import Button from 'primevue/button'
import InputText from 'primevue/inputtext'
import { Color } from '@/models/Device.ts'
import { Emitter, EventType } from 'mitt'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const confirm = useConfirm()
const toast = useToast()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!

const applyThinkPadFanControl = (value: boolean | string | number) => {
    settingsStore.applyThinkPadFanControl(Boolean(value))
}
const themeModeOptions = [
    { value: ThemeMode.SYSTEM, label: deviceStore.toTitleCase(ThemeMode.SYSTEM) },
    { value: ThemeMode.DARK, label: deviceStore.toTitleCase(ThemeMode.DARK) },
    { value: ThemeMode.LIGHT, label: deviceStore.toTitleCase(ThemeMode.LIGHT) },
    {
        value: ThemeMode.HIGH_CONTRAST_DARK,
        label: deviceStore.toTitleCase(ThemeMode.HIGH_CONTRAST_DARK),
    },
    {
        value: ThemeMode.HIGH_CONTRAST_LIGHT,
        label: deviceStore.toTitleCase(ThemeMode.HIGH_CONTRAST_LIGHT),
    },
    { value: ThemeMode.CUSTOM, label: deviceStore.toTitleCase(ThemeMode.CUSTOM) },
]
const changeThemeMode = async (event: ListboxChangeEvent) => {
    if (event.value === null) {
        return // do not update on unselect
    }
    settingsStore.themeMode = event.value
    await deviceStore.waitAndReload(1.05)
}
const lineThicknessOptions = ref([
    { optionSize: 1, value: 0.5 },
    { optionSize: 2, value: 1.0 },
    { optionSize: 3, value: 1.5 },
    { optionSize: 4, value: 2.0 },
    { optionSize: 6, value: 3.0 },
])
const customThemeAccent: Ref<Color> = ref(`rgb(${settingsStore.customTheme.accent})`)
const customThemeBgOne: Ref<Color> = ref(`rgb(${settingsStore.customTheme.bgOne})`)
const customThemeBgTwo: Ref<Color> = ref(`rgb(${settingsStore.customTheme.bgTwo})`)
const customThemeBorder: Ref<Color> = ref(`rgb(${settingsStore.customTheme.borderOne})`)
const customThemeText: Ref<Color> = ref(`rgb(${settingsStore.customTheme.textColor})`)
const customThemeTextSecondary: Ref<Color> = ref(
    `rgb(${settingsStore.customTheme.textColorSecondary})`,
)
const setNewColorAccent = (newColor: Color | null): void => {
    if (newColor == null) {
        customThemeAccent.value = `rgb(${defaultCustomTheme.accent})`
    }
    settingsStore.customTheme.accent = customThemeAccent.value.replaceAll(/[a-z]|[(),]/g, '')
    document.documentElement.style.setProperty('--colors-accent', settingsStore.customTheme.accent)
}
const setNewColorBgOne = (newColor: Color | null): void => {
    if (newColor == null) {
        customThemeBgOne.value = `rgb(${defaultCustomTheme.bgOne})`
    }
    settingsStore.customTheme.bgOne = customThemeBgOne.value.replaceAll(/[a-z]|[(),]/g, '')
    document.documentElement.style.setProperty('--colors-bg-one', settingsStore.customTheme.bgOne)
}
const setNewColorBgTwo = (newColor: Color | null): void => {
    if (newColor == null) {
        customThemeBgTwo.value = `rgb(${defaultCustomTheme.bgTwo})`
    }
    settingsStore.customTheme.bgTwo = customThemeBgTwo.value.replaceAll(/[a-z]|[(),]/g, '')
    document.documentElement.style.setProperty('--colors-bg-two', settingsStore.customTheme.bgTwo)
}
const setNewColorBorder = (newColor: Color | null): void => {
    if (newColor == null) {
        customThemeBorder.value = `rgb(${defaultCustomTheme.borderOne})`
    }
    settingsStore.customTheme.borderOne = customThemeBorder.value.replaceAll(/[a-z]|[(),]/g, '')
    document.documentElement.style.setProperty(
        '--colors-border-one',
        settingsStore.customTheme.borderOne,
    )
}
const setNewColorText = (newColor: Color | null): void => {
    if (newColor == null) {
        customThemeText.value = `rgb(${defaultCustomTheme.textColor})`
    }
    settingsStore.customTheme.textColor = customThemeText.value.replaceAll(/[a-z]|[(),]/g, '')
    document.documentElement.style.setProperty(
        '--colors-text-color',
        settingsStore.customTheme.textColor,
    )
}
const setNewColorTextSecondary = (newColor: Color | null): void => {
    if (newColor == null) {
        customThemeTextSecondary.value = `rgb(${defaultCustomTheme.textColorSecondary})`
    }
    settingsStore.customTheme.textColorSecondary = customThemeTextSecondary.value.replaceAll(
        /[a-z]|[(),]/g,
        '',
    )
    document.documentElement.style.setProperty(
        '--colors-text-color-secondary',
        settingsStore.customTheme.textColorSecondary,
    )
}

const blacklistedDevices: Ref<Array<CoolerControlDeviceSettingsDTO>> = ref([])
for (const deviceSettings of settingsStore.ccBlacklistedDevices.values()) {
    blacklistedDevices.value.push(deviceSettings)
}
const selectedBlacklistedDevices: Ref<Array<CoolerControlDeviceSettingsDTO>> = ref([])
const applyGenericDaemonChange = () => {
    confirm.require({
        message:
            'Changing this setting requires a daemon and UI restart. ' +
            'Are you sure want to do this now?',
        header: 'Apply Setting and Restart',
        icon: 'pi pi-exclamation-triangle',
        defaultFocus: 'accept',
        accept: async () => {
            toast.add({
                severity: 'success',
                summary: 'Success',
                detail: 'Restarting now',
                life: 6000,
            })
            await deviceStore.daemonClient.shutdownDaemon()
            await deviceStore.waitAndReload(5)
        },
    })
}
const reEnableSelected = () => {
    if (selectedBlacklistedDevices.value.length === 0) {
        return
    }
    confirm.require({
        message:
            'Re-enabling these devices requires a daemon and UI restart. ' +
            'Are you sure want to do this now?',
        header: 'Enable Devices',
        icon: 'pi pi-exclamation-triangle',
        accept: async () => {
            let successful: boolean = true
            for (const ccSetting of selectedBlacklistedDevices.value) {
                ccSetting.disable = false
                successful =
                    (await deviceStore.daemonClient.saveCCDeviceSettings(
                        ccSetting.uid,
                        ccSetting,
                    )) && successful
            }
            if (successful) {
                toast.add({
                    severity: 'success',
                    summary: 'Success',
                    detail: 'Devices Enabled. Restarting now',
                    life: 6000,
                })
                await deviceStore.daemonClient.shutdownDaemon()
                await deviceStore.waitAndReload(5)
            } else {
                toast.add({
                    severity: 'error',
                    summary: 'Error',
                    detail: 'Unknown error trying to set re-enable devices. See logs for details.',
                    life: 4000,
                })
            }
        },
    })
}

const daemonPort: Ref<number> = ref(deviceStore.getDaemonPort())
const daemonAddress: Ref<string> = ref(deviceStore.getDaemonAddress())
const daemonSslEnabled: Ref<boolean> = ref(deviceStore.getDaemonSslEnabled())
const saveDaemonSettings = () => {
    deviceStore.setDaemonAddress(daemonAddress.value)
    deviceStore.setDaemonPort(daemonPort.value)
    deviceStore.setDaemonSslEnabled(daemonSslEnabled.value)
    deviceStore.reloadUI()
}
const resetDaemonSettings = () => {
    deviceStore.clearDaemonAddress()
    deviceStore.clearDaemonPort()
    deviceStore.clearDaemonSslEnabled()
    deviceStore.reloadUI()
}
</script>

<template>
    <div class="flex h-[3.5rem] border-b-4 border-border-one items-center justify-between">
        <div class="pl-4 py-2 text-2xl">Application Settings</div>
    </div>
    <ScrollAreaRoot style="--scrollbar-size: 10px">
        <ScrollAreaViewport class="pb-16 h-screen w-full">
            <Tabs value="0">
                <TabList>
                    <Tab value="0" as="div" class="flex w-60 justify-center items-center gap-2">
                        <svg-icon
                            type="mdi"
                            :path="mdiViewQuiltOutline"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                        Interface
                    </Tab>
                    <Tab value="1" as="div" class="flex w-60 justify-center items-center gap-2">
                        <svg-icon
                            type="mdi"
                            :path="mdiDnsOutline"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                        Daemon
                    </Tab>
                    <Tab
                        value="2"
                        as="div"
                        class="flex w-60 justify-center items-center gap-2"
                        :disabled="!deviceStore.isTauriApp()"
                    >
                        <svg-icon
                            type="mdi"
                            :path="mdiMonitor"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                        Desktop
                    </Tab>
                    <Tab
                        value="3"
                        as="div"
                        class="flex w-60 justify-center items-center gap-2"
                        :disabled="!deviceStore.isThinkPad"
                    >
                        <svg-icon
                            type="mdi"
                            :path="mdiLaptop"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                        ThinkPad
                    </Tab>
                    <Tab
                        value="4"
                        as="div"
                        class="flex w-full justify-center items-center"
                        :disabled="true"
                    />
                </TabList>
                <TabPanels class="mt-2">
                    <TabPanel value="0" class="flex flex-col lg:flex-row">
                        <!--UI Settings-->
                        <table class="bg-bg-two rounded-lg">
                            <tbody>
                                <tr v-tooltip.right="'Time format: 12-hour (AM/PM) or 24-hour'">
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        Time Format
                                    </td>
                                    <td
                                        class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <el-switch
                                            v-model="settingsStore.time24"
                                            size="large"
                                            active-text="24hr"
                                            inactive-text="12hr"
                                            style="--el-switch-off-color: rgb(var(--colors-accent))"
                                        />
                                    </td>
                                </tr>
                                <tr
                                    v-tooltip.right="
                                        'Adjust the precision of displayed frequency values.'
                                    "
                                >
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        Frequency Precision
                                    </td>
                                    <td
                                        class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <el-switch
                                            v-model="settingsStore.frequencyPrecision"
                                            size="large"
                                            :active-value="1000"
                                            active-text="Ghz"
                                            :inactive-value="1"
                                            inactive-text="Mhz"
                                            style="--el-switch-off-color: rgb(var(--colors-accent))"
                                        />
                                    </td>
                                </tr>
                                <tr
                                    v-tooltip.right="
                                        'Adjust the line thickness of charts on the dashboard'
                                    "
                                >
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        Line Size
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 leading-none border-border-one border-l-2 border-t-2"
                                    >
                                        <Select
                                            v-model="settingsStore.chartLineScale"
                                            :options="lineThicknessOptions"
                                            option-label="optionSize"
                                            option-value="value"
                                            class="w-full h-10"
                                            scroll-height="100%"
                                            :pt="{
                                                // @ts-ignore
                                                option: ({ context }) => ({
                                                    class: [
                                                        'py-2 px-5',
                                                        'bg-bg-one',
                                                        'hover:bg-surface-hover',
                                                        'transition-shadow',
                                                        'duration-200',
                                                        { 'bg-surface-hover': context.selected },
                                                    ],
                                                }),
                                                overlay: {
                                                    class: [
                                                        'border-2 border-border-one',
                                                        'rounded-lg',
                                                        'shadow-lg',
                                                        'bg-bg-one',
                                                    ],
                                                },
                                                // @ts-ignore
                                                label: ({ props, parent }) => ({
                                                    class: [
                                                        'p-2',
                                                        'flex-auto',
                                                        'transition',
                                                        'duration-200',
                                                        'focus:outline-none focus:shadow-none',
                                                        'cursor-pointer',
                                                        'whitespace-nowrap',
                                                        'appearance-none',
                                                    ],
                                                }),
                                            }"
                                        >
                                            <template #value="slotProps">
                                                <div class="content-center h-full w-full">
                                                    <div
                                                        :style="`border-bottom: ${slotProps.value * 2}px solid rgb(var(--colors-text-color))`"
                                                    />
                                                </div>
                                            </template>
                                            <template #option="slotProps">
                                                <div class="content-center h-6 w-full">
                                                    <div
                                                        :style="`border-bottom: ${slotProps.option.optionSize}px
                                                    solid rgb(var(${slotProps.selected ? '--colors-text-color' : '--colors-text-color-secondary'}))`"
                                                    />
                                                </div>
                                            </template>
                                        </Select>
                                    </td>
                                </tr>
                                <tr
                                    v-tooltip.right="
                                        'Control the visibility of menu items.\nYou can choose to show them ' +
                                        'greyed out, or hide them altogether.'
                                    "
                                >
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        Hidden Menu Items
                                    </td>
                                    <td
                                        class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <el-switch
                                            v-model="settingsStore.displayHiddenItems"
                                            size="large"
                                            active-text="show"
                                            inactive-text="&nbsp;&nbsp;hide"
                                        />
                                    </td>
                                </tr>
                                <tr v-tooltip.right="'Change the overall UI color scheme.'">
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        <div
                                            class="float-left"
                                            v-tooltip.top="'Triggers a restart of the UI'"
                                        >
                                            <svg-icon
                                                type="mdi"
                                                :path="mdiRestartAlert"
                                                :size="deviceStore.getREMSize(1.0)"
                                            />
                                        </div>
                                        <div class="text-right float-right">Theme Style</div>
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <Listbox
                                            :model-value="settingsStore.themeMode"
                                            :options="themeModeOptions"
                                            class="w-full"
                                            checkmark
                                            list-style="max-height: 100%"
                                            option-label="label"
                                            option-value="value"
                                            @change="changeThemeMode"
                                            :pt="{
                                                // @ts-ignore
                                                root: ({ props }) => ({
                                                    class: [
                                                        'min-w-[12rem]',
                                                        'rounded-lg',
                                                        'bg-bg-one',
                                                        'text-text-color',
                                                        'border-2',
                                                        { 'border-border-one': !props.invalid },
                                                        { 'border-red': props.invalid },
                                                    ],
                                                }),
                                            }"
                                        />
                                    </td>
                                </tr>
                                <tr v-tooltip.right="'Start the application introduction tour.'">
                                    <td
                                        class="py-5 px-4 w-60 leading-none content-center items-center border-border-one border-r-2 border-t-2"
                                    >
                                        <div class="float-right">Introduction</div>
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <Button
                                            label="Start Tour"
                                            class="bg-accent/80 hover:!bg-accent w-full h-[2.375rem]"
                                            @click="emitter.emit('start-tour')"
                                        />
                                    </td>
                                </tr>
                            </tbody>
                        </table>
                        <table
                            v-if="settingsStore.themeMode === ThemeMode.CUSTOM"
                            class="lg:ml-4 h-full bg-bg-two rounded-lg"
                        >
                            <tbody>
                                <tr>
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        Accent Color
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <div
                                            class="w-full h-full content-center flex justify-center"
                                        >
                                            <div class="color-wrapper my-1">
                                                <el-color-picker
                                                    v-model="customThemeAccent"
                                                    color-format="rgb"
                                                    :predefine="
                                                        settingsStore.predefinedColorOptions
                                                    "
                                                    :validate-event="false"
                                                    @change="setNewColorAccent"
                                                />
                                            </div>
                                        </div>
                                    </td>
                                </tr>
                                <tr>
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2"
                                    >
                                        Background One
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2"
                                    >
                                        <div
                                            class="w-full h-full content-center flex justify-center"
                                        >
                                            <div class="color-wrapper my-1">
                                                <el-color-picker
                                                    v-model="customThemeBgOne"
                                                    color-format="rgb"
                                                    :predefine="
                                                        settingsStore.predefinedColorOptions
                                                    "
                                                    :validate-event="false"
                                                    @change="setNewColorBgOne"
                                                />
                                            </div>
                                        </div>
                                    </td>
                                </tr>
                                <tr>
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        Background Two
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <div
                                            class="w-full h-full content-center flex justify-center"
                                        >
                                            <div
                                                class="color-wrapper my-1 rounded-lg border-2 border-border-one"
                                            >
                                                <el-color-picker
                                                    v-model="customThemeBgTwo"
                                                    color-format="rgb"
                                                    :predefine="
                                                        settingsStore.predefinedColorOptions
                                                    "
                                                    :validate-event="false"
                                                    @change="setNewColorBgTwo"
                                                />
                                            </div>
                                        </div>
                                    </td>
                                </tr>
                                <tr>
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        Border
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <div
                                            class="w-full h-full content-center flex justify-center"
                                        >
                                            <div class="color-wrapper my-1">
                                                <el-color-picker
                                                    v-model="customThemeBorder"
                                                    color-format="rgb"
                                                    :predefine="
                                                        settingsStore.predefinedColorOptions
                                                    "
                                                    :validate-event="false"
                                                    @change="setNewColorBorder"
                                                />
                                            </div>
                                        </div>
                                    </td>
                                </tr>
                                <tr>
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        Text
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <div
                                            class="w-full h-full content-center flex justify-center"
                                        >
                                            <div class="color-wrapper my-1">
                                                <el-color-picker
                                                    v-model="customThemeText"
                                                    color-format="rgb"
                                                    :predefine="
                                                        settingsStore.predefinedColorOptions
                                                    "
                                                    :validate-event="false"
                                                    @change="setNewColorText"
                                                />
                                            </div>
                                        </div>
                                    </td>
                                </tr>
                                <tr>
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        Text Secondary
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <div
                                            class="w-full h-full content-center flex justify-center"
                                        >
                                            <div class="color-wrapper my-1">
                                                <el-color-picker
                                                    v-model="customThemeTextSecondary"
                                                    color-format="rgb"
                                                    :predefine="
                                                        settingsStore.predefinedColorOptions
                                                    "
                                                    :validate-event="false"
                                                    @change="setNewColorTextSecondary"
                                                />
                                            </div>
                                        </div>
                                    </td>
                                </tr>
                            </tbody>
                        </table>
                    </TabPanel>
                    <TabPanel value="1" class="flex flex-col lg:flex-row">
                        <!--Daemon Settings-->
                        <table class="bg-bg-two rounded-lg mb-4">
                            <tbody>
                                <tr
                                    v-tooltip.right="
                                        'Automatically apply settings on daemon startup and when waking from sleep'
                                    "
                                >
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        Apply Settings on Startup
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <el-switch
                                            v-model="settingsStore.ccSettings.apply_on_boot"
                                            size="large"
                                        />
                                    </td>
                                </tr>
                                <tr
                                    v-tooltip.right="
                                        'Delay before starting device communication (in seconds).\n' +
                                        'Helps with devices that take time to initialize or are intermittently detected'
                                    "
                                >
                                    <td
                                        class="py-4 px-4 w-60 leading-none text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        Device Delay at Startup
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <InputNumber
                                            v-model="settingsStore.ccSettings.startup_delay"
                                            show-buttons
                                            :min="1"
                                            :max="10"
                                            suffix=" s"
                                            button-layout="horizontal"
                                            :input-style="{ width: '5rem' }"
                                        >
                                            <template #incrementicon>
                                                <span class="pi pi-plus" />
                                            </template>
                                            <template #decrementicon>
                                                <span class="pi pi-minus" />
                                            </template>
                                        </InputNumber>
                                    </td>
                                </tr>
                                <tr
                                    v-tooltip.right="
                                        'Enable response compression to reduce API payload size,\n' +
                                        'but note that this will increase CPU usage.'
                                    "
                                >
                                    <td
                                        class="py-4 px-4 w-60 leading-none items-center border-border-one border-r-2 border-b-2"
                                    >
                                        <div
                                            class="float-left"
                                            v-tooltip.top="'Triggers a daemon restart'"
                                        >
                                            <svg-icon
                                                type="mdi"
                                                :path="mdiRestartAlert"
                                                :size="deviceStore.getREMSize(1.0)"
                                            />
                                        </div>
                                        <div class="text-right float-right">
                                            Compress API Payload
                                        </div>
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <el-switch
                                            v-model="settingsStore.ccSettings.compress"
                                            size="large"
                                            @click="applyGenericDaemonChange"
                                        />
                                    </td>
                                </tr>
                                <tr
                                    v-tooltip.right="
                                        'Disabling this will fully deactivate Liquidctl integration, \n' +
                                        'regardless of the installation status of the coolercontrol-liqctld \n' +
                                        'package. If available, HWMon drivers will be utilized instead.'
                                    "
                                >
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        <div
                                            class="float-left"
                                            v-tooltip.top="'Triggers a daemon restart'"
                                        >
                                            <svg-icon
                                                type="mdi"
                                                :path="mdiRestartAlert"
                                                :size="deviceStore.getREMSize(1.0)"
                                            />
                                        </div>
                                        <div class="text-right float-right">
                                            Liquidctl Integration
                                        </div>
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <el-switch
                                            v-model="settingsStore.ccSettings.liquidctl_integration"
                                            size="large"
                                        />
                                    </td>
                                </tr>
                                <tr
                                    v-tooltip.right="
                                        'Caution: Disable this ONLY if you, or another program,\n' +
                                        'are handling liquidctl device initialization.' +
                                        '\nThis can help avoid conflicts with other programs.'
                                    "
                                >
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        Liquidctl Device Initialization
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <el-switch
                                            v-model="settingsStore.ccSettings.no_init"
                                            :disabled="
                                                !settingsStore.ccSettings.liquidctl_integration
                                            "
                                            :active-value="false"
                                            :inactive-value="true"
                                            size="large"
                                        />
                                    </td>
                                </tr>
                                <tr
                                    v-tooltip.right="
                                        'Some devices are supported by both Liquidctl and HWMon drivers.' +
                                        '\nLiquidctl is used by default for its extra features. ' +
                                        'To use HWMon drivers instead,\ndisable this and the liquidctl ' +
                                        'device to avoid driver conflicts.'
                                    "
                                >
                                    <td
                                        class="py-4 px-4 w-60 leading-none items-center border-border-one border-r-2 border-b-2"
                                    >
                                        <div
                                            class="float-left"
                                            v-tooltip.top="'Triggers a daemon restart'"
                                        >
                                            <svg-icon
                                                type="mdi"
                                                :path="mdiRestartAlert"
                                                :size="deviceStore.getREMSize(1.0)"
                                            />
                                        </div>
                                        <div class="text-right float-right">
                                            Hide Duplicate Devices
                                        </div>
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <el-switch
                                            v-model="
                                                settingsStore.ccSettings.hide_duplicate_devices
                                            "
                                            :disabled="
                                                !settingsStore.ccSettings.liquidctl_integration
                                            "
                                            size="large"
                                            @change="applyGenericDaemonChange"
                                        />
                                    </td>
                                </tr>
                                <tr v-tooltip.right="'Re-enable selected devices.'">
                                    <td
                                        class="py-5 px-4 w-60 leading-none border-border-one border-r-2 border-t-2"
                                    >
                                        <div
                                            class="float-left"
                                            v-tooltip.top="'Triggers a daemon restart'"
                                        >
                                            <svg-icon
                                                type="mdi"
                                                :path="mdiRestartAlert"
                                                :size="deviceStore.getREMSize(1.0)"
                                            />
                                        </div>
                                        <div class="text-right float-right">Disabled Devices</div>
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <div v-if="blacklistedDevices.length">
                                            <Listbox
                                                v-model="selectedBlacklistedDevices"
                                                :options="blacklistedDevices"
                                                class="w-full"
                                                multiple
                                                checkmark
                                                list-style="max-height: 100%"
                                                option-label="name"
                                                option-value="uid"
                                                :pt="{
                                                    // @ts-ignore
                                                    root: ({ props }) => ({
                                                        class: [
                                                            'min-w-[12rem]',
                                                            'rounded-lg',
                                                            'bg-bg-one',
                                                            'text-text-color',
                                                            'border-2',
                                                            { 'border-border-one': !props.invalid },
                                                            { 'border-red': props.invalid },
                                                        ],
                                                    }),
                                                }"
                                            />
                                            <Button
                                                label="Enable"
                                                class="mt-4 bg-accent/80 hover:!bg-accent w-full h-[2.375rem]"
                                                :disabled="selectedBlacklistedDevices.length === 0"
                                                @click="reEnableSelected"
                                            />
                                        </div>
                                        <div v-else class="text-center italic">None</div>
                                    </td>
                                </tr>
                            </tbody>
                        </table>
                        <table class="lg:ml-4 h-full bg-bg-two rounded-lg">
                            <tbody>
                                <tr
                                    v-tooltip.right="
                                        'The IP address or domain name of the daemon to establish a ' +
                                        'connection with.\nSupports IPv4, IPv6, and DNS-resolvable hostnames.'
                                    "
                                >
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        Address
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <InputText
                                            v-model="daemonAddress"
                                            class="min-w-48 w-full"
                                            placeholder="localhost"
                                        />
                                    </td>
                                </tr>
                                <tr
                                    v-tooltip.right="
                                        'The port used to establish a connection with the daemon.'
                                    "
                                >
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        Port
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <InputNumber
                                            v-model="daemonPort"
                                            show-buttons
                                            :min="80"
                                            :max="65535"
                                            :useGrouping="false"
                                            button-layout="horizontal"
                                            :input-style="{ width: '6rem' }"
                                        >
                                            <template #incrementicon>
                                                <span class="pi pi-plus" />
                                            </template>
                                            <template #decrementicon>
                                                <span class="pi pi-minus" />
                                            </template>
                                        </InputNumber>
                                    </td>
                                </tr>
                                <tr
                                    v-tooltip.right="
                                        'Whether to connect to the daemon using SSL/TLS.\nA proxy setup is required.'
                                    "
                                >
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        SSL/TLS
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <el-switch v-model="daemonSslEnabled" size="large" />
                                    </td>
                                </tr>
                                <tr>
                                    <td
                                        class="py-5 px-4 w-60 leading-none content-center items-center border-border-one border-r-2 border-t-2"
                                    >
                                        <div
                                            class="float-left h-[2.375rem] content-center"
                                            v-tooltip.top="'Triggers a daemon restart'"
                                        >
                                            <svg-icon
                                                type="mdi"
                                                :path="mdiRestartAlert"
                                                :size="deviceStore.getREMSize(1.0)"
                                            />
                                        </div>
                                        <div class="float-right">
                                            <Button
                                                label="Defaults"
                                                class="h-[2.375rem]"
                                                @click="resetDaemonSettings"
                                                v-tooltip.top="'Reset to default settings'"
                                            />
                                        </div>
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <Button
                                            label="Apply"
                                            class="bg-accent/80 hover:!bg-accent w-full h-[2.375rem]"
                                            @click="saveDaemonSettings"
                                            v-tooltip.top="'Save and reload the UI'"
                                        />
                                    </td>
                                </tr>
                            </tbody>
                        </table>
                    </TabPanel>
                    <TabPanel value="2">
                        <!--Desktop Settings-->
                        <table class="bg-bg-two rounded-lg">
                            <tbody>
                                <tr
                                    v-tooltip.right="
                                        'Upon startup, the main UI window will be hidden and only ' +
                                        'the system tray icon will be visible.'
                                    "
                                >
                                    <td
                                        class="py-4 px-2 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        Start in Tray
                                    </td>
                                    <td
                                        class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <el-switch
                                            v-model="settingsStore.startInSystemTray"
                                            size="large"
                                        />
                                    </td>
                                </tr>
                                <tr
                                    v-tooltip.right="
                                        'Closing the application window will leave the app running in the system tray'
                                    "
                                >
                                    <td
                                        class="py-4 px-2 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        Close to Tray
                                    </td>
                                    <td
                                        class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <el-switch
                                            v-model="settingsStore.closeToSystemTray"
                                            size="large"
                                        />
                                    </td>
                                </tr>
                            </tbody>
                        </table>
                    </TabPanel>
                    <TabPanel value="3">
                        <!--ThinkPad Settings-->
                        <table class="bg-bg-two rounded-lg">
                            <tbody>
                                <tr
                                    v-tooltip.right="
                                        'This is a helper to enable ThinkPad ACPI Fan Control.\nFan control operations are disabled by ' +
                                        'default for safety reasons. CoolerControl can try to enable this for you, but you should be aware of the risks ' +
                                        'to your hardware.\nProceed at your own risk.'
                                    "
                                >
                                    <td
                                        class="py-4 px-2 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        Fan Control
                                    </td>
                                    <td
                                        class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <el-switch
                                            v-model="settingsStore.thinkPadFanControlEnabled"
                                            size="large"
                                            @change="applyThinkPadFanControl"
                                        />
                                    </td>
                                </tr>
                                <tr
                                    v-tooltip.right="
                                        'For Thinkpad Laptops this enables Full-Speed mode.\nThis allows the fans to ' +
                                        'spin up to their absolute maximum when set to 100%, but will run the fans out of ' +
                                        'specification and cause increased wear.\nUse with caution.'
                                    "
                                >
                                    <td
                                        class="py-4 px-2 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        Full Speed
                                    </td>
                                    <td
                                        class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <el-switch
                                            v-model="settingsStore.ccSettings.thinkpad_full_speed"
                                            size="large"
                                        />
                                    </td>
                                </tr>
                            </tbody>
                        </table>
                    </TabPanel>
                </TabPanels>
            </Tabs>
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
    line-height: normal;
    height: 2rem;
    width: 2rem;
}

.color-wrapper :deep(.el-color-picker__trigger) {
    border: 0 !important;
    padding: 0 !important;
    margin: 0 !important;
    height: 2rem !important;
    width: 2rem !important;
}

.color-wrapper :deep(.el-color-picker__mask) {
    border: 0 !important;
    padding: 0 !important;
    margin: 0 !important;
    height: 2rem !important;
    width: 2rem !important;
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
    width: 2rem !important;
    height: 2rem !important;
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
