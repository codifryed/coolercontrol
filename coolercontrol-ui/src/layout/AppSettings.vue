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
import { inject, onMounted, type Ref, ref, watch, computed, onUnmounted } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useConfirm } from 'primevue/useconfirm'
import { useToast } from 'primevue/usetoast'
import { defaultCustomTheme, ThemeMode } from '@/models/UISettings.ts'
import { CoolerControlDeviceSettingsDTO } from '@/models/CCSettings.ts'
import {
    mdiDnsOutline,
    mdiFormatListBulleted,
    mdiLaptop,
    mdiMonitor,
    mdiRestart,
    mdiViewQuiltOutline,
} from '@mdi/js'
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
import LanguageSwitcher from '@/components/LanguageSwitcher.vue'
import { useI18n } from 'vue-i18n'
import { api as fullscreenApi } from 'vue-fullscreen'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const confirm = useConfirm()
const toast = useToast()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!
import _ from 'lodash'
import AppSettingsDevices from '@/layout/AppSettingsDevices.vue'

const { t } = useI18n()

const applyThinkPadFanControl = (value: boolean | string | number) => {
    settingsStore.applyThinkPadFanControl(Boolean(value))
}

const isFullScreen = ref(fullscreenApi.isFullscreen)
if (deviceStore.isQtApp()) {
    // @ts-ignore
    const ipc = window.ipc
    isFullScreen.value = await ipc.getIsFullScreen()
    ipc.fullScreenToggled.connect((fullscreen: boolean) => {
        isFullScreen.value = fullscreen
    })
}
const toggleFullScreen = async (_enable: string | number | boolean): Promise<void> => {
    await fullscreenApi.toggle(null, {
        callback: async (fullscreen: boolean) => {
            isFullScreen.value = fullscreen
            if (deviceStore.isQtApp()) {
                await deviceStore.sleep(50)
                // @ts-ignore
                const ipc = window.ipc
                isFullScreen.value = await ipc.getIsFullScreen()
            }
        },
    })
}

// Use computed to respond to language changes
const themeModeOptions = computed(() => [
    { value: ThemeMode.SYSTEM, label: t('layout.settings.themeMode.system') },
    { value: ThemeMode.DARK, label: t('layout.settings.themeMode.dark') },
    { value: ThemeMode.LIGHT, label: t('layout.settings.themeMode.light') },
    {
        value: ThemeMode.HIGH_CONTRAST_DARK,
        label: t('layout.settings.themeMode.highContrastDark'),
    },
    {
        value: ThemeMode.HIGH_CONTRAST_LIGHT,
        label: t('layout.settings.themeMode.highContrastLight'),
    },
    { value: ThemeMode.CUSTOM, label: t('layout.settings.themeMode.custom') },
])
const changeThemeMode = async (event: ListboxChangeEvent) => {
    if (event.value === null) {
        return // do not update on unselect
    }

    // Save the original theme mode
    const previousThemeMode = settingsStore.themeMode

    // Update the theme mode in settings store
    settingsStore.themeMode = event.value

    // Dynamically apply theme changes without page reload
    settingsStore.applyThemeMode()

    // Display success notification
    toast.add({
        severity: 'success',
        summary: t('common.success'),
        detail: t('layout.settings.themeChangeSuccess'),
        life: 2000,
    })

    // Trigger theme change event to notify other components
    window.dispatchEvent(
        new CustomEvent('theme-changed', {
            detail: {
                previousTheme: previousThemeMode,
                currentTheme: event.value,
            },
        }),
    )
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
const applyGenericDaemonChange = _.debounce(
    () =>
        confirm.require({
            message: t('layout.settings.applySettingAndRestart'),
            header: t('layout.settings.restartHeader'),
            icon: 'pi pi-exclamation-triangle',
            defaultFocus: 'accept',
            acceptLabel: t('common.yes'),
            rejectLabel: t('common.no'),
            accept: async () => {
                settingsStore.ccSettings.poll_rate = pollRate.value
                // give the system a moment to make sure the pollRate has been saved ^
                await deviceStore.sleep(50)
                toast.add({
                    severity: 'success',
                    summary: t('layout.settings.success'),
                    detail: t('layout.settings.successDetail'),
                    life: 6000,
                })
                await deviceStore.daemonClient.shutdownDaemon()
                await deviceStore.waitAndReload()
            },
        }),
    2000,
)

const applyQuickUIRefresh = _.debounce(() => deviceStore.reloadUI(), 1000)

const applyEntitiesBelowSensorsChange = (value: boolean | string | number): void => {
    toast.add({
        severity: 'success',
        summary: t('common.success'),
        detail: Boolean(value)
            ? t('layout.settings.entitiesBelowSensorsEnabledMessage')
            : t('layout.settings.entitiesBelowSensorsDisabledMessage'),
        life: 3000,
    })
    applyQuickUIRefresh()
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
const pollRate: Ref<number> = ref(settingsStore.ccSettings.poll_rate)
watch(pollRate, () => {
    applyGenericDaemonChange()
})

const portScrolled = (event: WheelEvent): void => {
    if (daemonPort.value == null) return
    if (event.deltaY < 0) {
        if (daemonPort.value < 65525) daemonPort.value += 10
    } else {
        if (daemonPort.value > 90) daemonPort.value -= 10
    }
}
const addScrollEventListeners = (): void => {
    // @ts-ignore
    document?.querySelector('#port-input')?.addEventListener('wheel', portScrolled)
}

onMounted(() => {
    addScrollEventListeners()

    // Listen for language change events
    window.addEventListener('language-changed', () => {
        // When language changes, the computed property will automatically update, no need to manually assign value
        // themeModeOptions.value = [...] - this line would cause an error

        // Trigger theme options recalculation
        window.dispatchEvent(new CustomEvent('theme-options-updated'))
    })
})

// Remove event listeners when component is unmounted
onUnmounted(() => {
    window.removeEventListener('language-changed', () => {
        // Cleanup code
    })
})
</script>

<template>
    <div class="flex h-[3.5rem] border-b-4 border-border-one items-center justify-between">
        <div class="pl-4 py-2 text-2xl font-bold">{{ t('layout.settings.title') }}</div>
    </div>
    <ScrollAreaRoot style="--scrollbar-size: 10px">
        <ScrollAreaViewport class="pb-16 h-screen w-full">
            <Tabs value="0">
                <TabList>
                    <Tab value="0" as="div" class="flex w-1/5 justify-center items-center gap-2">
                        <svg-icon
                            type="mdi"
                            :path="mdiViewQuiltOutline"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                        {{ t('layout.settings.general') }}
                    </Tab>
                    <Tab
                        value="4"
                        as="div"
                        class="flex w-1/5 justify-center items-center gap-2"
                        :disabled="false"
                    >
                        <svg-icon
                            type="mdi"
                            :path="mdiFormatListBulleted"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                        {{ t('layout.settings.device') }}
                    </Tab>
                    <Tab value="1" as="div" class="flex w-1/5 justify-center items-center gap-2">
                        <svg-icon
                            type="mdi"
                            :path="mdiDnsOutline"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                        {{ t('views.daemon.title', 'Daemon') }}
                    </Tab>
                    <Tab
                        value="2"
                        as="div"
                        class="flex w-1/5 justify-center items-center gap-2"
                        :disabled="!deviceStore.isQtApp()"
                    >
                        <svg-icon
                            type="mdi"
                            :path="mdiMonitor"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                        {{ t('layout.settings.desktop', 'Desktop') }}
                    </Tab>
                    <Tab
                        value="3"
                        as="div"
                        class="flex w-1/5 justify-center items-center gap-2"
                        :disabled="!deviceStore.isThinkPad"
                    >
                        <svg-icon
                            type="mdi"
                            :path="mdiLaptop"
                            :size="deviceStore.getREMSize(1.5)"
                        />
                        {{ t('layout.settings.thinkpad', 'ThinkPad') }}
                    </Tab>
                </TabList>
                <TabPanels class="mt-2">
                    <TabPanel value="0" class="flex flex-col lg:flex-row">
                        <!--UI Settings-->
                        <table class="bg-bg-two rounded-lg">
                            <tbody>
                                <tr v-tooltip.right="t('layout.settings.tooltips.introduction')">
                                    <td
                                        class="py-5 px-4 w-60 leading-none content-center items-center border-border-one border-r-2 border-b-2"
                                    >
                                        <div class="float-right">
                                            {{ t('layout.settings.introduction') }}
                                        </div>
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <Button
                                            :label="t('layout.settings.startTour')"
                                            class="bg-accent/80 hover:!bg-accent w-full h-[2.375rem]"
                                            @click="emitter.emit('start-tour')"
                                        />
                                    </td>
                                </tr>
                                <tr v-tooltip.right="t('layout.settings.tooltips.timeFormat')">
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        {{ t('layout.settings.timeFormat') }}
                                    </td>
                                    <td
                                        class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <el-switch
                                            v-model="settingsStore.time24"
                                            size="large"
                                            :active-text="t('layout.settings.time24h')"
                                            :inactive-text="t('layout.settings.time12h')"
                                            style="--el-switch-off-color: rgb(var(--colors-accent))"
                                        />
                                    </td>
                                </tr>
                                <tr
                                    v-tooltip.right="
                                        t('layout.settings.tooltips.frequencyPrecision')
                                    "
                                >
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        {{ t('layout.settings.frequencyPrecision') }}
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
                                <tr v-tooltip.right="t('layout.settings.tooltips.sidebarCollapse')">
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        {{ t('layout.settings.sidebarToCollapse') }}
                                    </td>
                                    <td
                                        class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <el-switch
                                            v-model="settingsStore.hideMenuCollapseIcon"
                                            size="large"
                                        />
                                    </td>
                                </tr>
                                <tr
                                    v-tooltip.right="
                                        t('layout.settings.tooltips.entitiesBelowSensors')
                                    "
                                >
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        <div
                                            class="float-left py-1"
                                            v-tooltip.top="
                                                t('layout.settings.tooltips.triggersUIRestart')
                                            "
                                        >
                                            <svg-icon
                                                type="mdi"
                                                :path="mdiRestart"
                                                :size="deviceStore.getREMSize(1.0)"
                                            />
                                        </div>
                                        <div class="text-right float-right">
                                            {{ t('layout.settings.entitiesBelowSensors') }}
                                        </div>
                                    </td>
                                    <td
                                        class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <el-switch
                                            v-model="settingsStore.menuEntitiesAtBottom"
                                            size="large"
                                            @change="applyEntitiesBelowSensorsChange"
                                        />
                                    </td>
                                </tr>
                                <tr v-tooltip.right="t('layout.settings.tooltips.fullScreen')">
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        {{ t('layout.settings.fullScreen') }}
                                    </td>
                                    <td
                                        class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <el-switch
                                            v-model="isFullScreen"
                                            :disabled="!fullscreenApi.isEnabled"
                                            size="large"
                                            @change="toggleFullScreen"
                                        />
                                    </td>
                                </tr>
                                <tr v-tooltip.right="t('layout.settings.tooltips.lineThickness')">
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        {{ t('layout.settings.dashboardLineSize') }}
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
                                <tr v-tooltip.right="t('layout.settings.appearance')">
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        <div
                                            class="float-left py-1"
                                            v-tooltip.top="
                                                t('layout.settings.tooltips.triggersUIRestart')
                                            "
                                        >
                                            <svg-icon
                                                type="mdi"
                                                :path="mdiRestart"
                                                :size="deviceStore.getREMSize(1.0)"
                                            />
                                        </div>
                                        <div class="text-right float-right">
                                            {{ t('layout.settings.themeStyle') }}
                                        </div>
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
                                <tr v-tooltip.right="t('layout.settings.language')">
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        <div class="text-right float-right">
                                            {{ t('layout.settings.language') }}
                                        </div>
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <LanguageSwitcher />
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
                                        {{ t('layout.settings.customTheme.accent') }}
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
                                                    :confirm-text="
                                                        t('layout.settings.colorPickerConfirm')
                                                    "
                                                    :cancel-text="
                                                        t('layout.settings.colorPickerCancel')
                                                    "
                                                />
                                            </div>
                                        </div>
                                    </td>
                                </tr>
                                <tr>
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2"
                                    >
                                        {{ t('layout.settings.customTheme.bgOne') }}
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
                                                    :confirm-text="
                                                        t('layout.settings.colorPickerConfirm')
                                                    "
                                                    :cancel-text="
                                                        t('layout.settings.colorPickerCancel')
                                                    "
                                                />
                                            </div>
                                        </div>
                                    </td>
                                </tr>
                                <tr>
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        {{ t('layout.settings.customTheme.bgTwo') }}
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
                                                    :confirm-text="
                                                        t('layout.settings.colorPickerConfirm')
                                                    "
                                                    :cancel-text="
                                                        t('layout.settings.colorPickerCancel')
                                                    "
                                                />
                                            </div>
                                        </div>
                                    </td>
                                </tr>
                                <tr>
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        {{ t('layout.settings.customTheme.border') }}
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
                                                    :confirm-text="
                                                        t('layout.settings.colorPickerConfirm')
                                                    "
                                                    :cancel-text="
                                                        t('layout.settings.colorPickerCancel')
                                                    "
                                                />
                                            </div>
                                        </div>
                                    </td>
                                </tr>
                                <tr>
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        {{ t('layout.settings.customTheme.text') }}
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
                                                    :confirm-text="
                                                        t('layout.settings.colorPickerConfirm')
                                                    "
                                                    :cancel-text="
                                                        t('layout.settings.colorPickerCancel')
                                                    "
                                                />
                                            </div>
                                        </div>
                                    </td>
                                </tr>
                                <tr>
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        {{ t('layout.settings.customTheme.textSecondary') }}
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
                                                    :confirm-text="
                                                        t('layout.settings.colorPickerConfirm')
                                                    "
                                                    :cancel-text="
                                                        t('layout.settings.colorPickerCancel')
                                                    "
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
                                <tr v-tooltip.right="t('layout.settings.tooltips.applyOnBoot')">
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        {{ t('layout.settings.applySettingsOnStartup') }}
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
                                <tr v-tooltip.right="t('layout.settings.tooltips.startupDelay')">
                                    <td
                                        class="py-4 px-4 w-60 leading-none text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        {{ t('layout.settings.deviceDelayAtStartup') }}
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <InputNumber
                                            v-model="settingsStore.ccSettings.startup_delay"
                                            show-buttons
                                            :min="1"
                                            :max="30"
                                            :suffix="` ${t('common.secondAbbr')}`"
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
                                <tr v-tooltip.right="t('layout.settings.tooltips.pollRate')">
                                    <td
                                        class="py-4 px-4 w-60 leading-none items-center border-border-one border-r-2 border-b-2"
                                    >
                                        <div
                                            class="float-left"
                                            v-tooltip.top="
                                                t('layout.settings.tooltips.triggersDaemonRestart')
                                            "
                                        >
                                            <svg-icon
                                                type="mdi"
                                                :path="mdiRestart"
                                                :size="deviceStore.getREMSize(1.1)"
                                            />
                                        </div>
                                        <div class="text-right float-right">
                                            {{ t('layout.settings.pollingRate') }}
                                        </div>
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <InputNumber
                                            v-model="pollRate"
                                            show-buttons
                                            :min="0.5"
                                            :max="5.0"
                                            :suffix="` ${t('common.secondAbbr')}`"
                                            :step="0.5"
                                            :min-fraction-digits="1"
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
                                <tr v-tooltip.right="t('layout.settings.tooltips.compress')">
                                    <td
                                        class="py-4 px-4 w-60 leading-none items-center border-border-one border-r-2 border-b-2"
                                    >
                                        <div
                                            class="float-left"
                                            v-tooltip.top="
                                                t('layout.settings.tooltips.triggersDaemonRestart')
                                            "
                                        >
                                            <svg-icon
                                                type="mdi"
                                                :path="mdiRestart"
                                                :size="deviceStore.getREMSize(1.0)"
                                            />
                                        </div>
                                        <div class="text-right float-right">
                                            {{ t('layout.settings.compressApiPayload') }}
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
                                <tr v-tooltip.right="t('layout.settings.tooltips.liquidctl')">
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        <div
                                            class="float-left py-1"
                                            v-tooltip.top="
                                                t('layout.settings.tooltips.triggersDaemonRestart')
                                            "
                                        >
                                            <svg-icon
                                                type="mdi"
                                                :path="mdiRestart"
                                                :size="deviceStore.getREMSize(1.0)"
                                            />
                                        </div>
                                        <div class="text-right float-right">
                                            {{ t('layout.settings.liquidctlIntegration') }}
                                        </div>
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <el-switch
                                            v-model="settingsStore.ccSettings.liquidctl_integration"
                                            size="large"
                                            @change="applyGenericDaemonChange"
                                        />
                                    </td>
                                </tr>
                                <tr v-tooltip.right="t('layout.settings.tooltips.liquidctlNoInit')">
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        {{ t('layout.settings.liquidctlDeviceInit') }}
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
                                <tr v-tooltip.right="t('layout.settings.tooltips.hideDuplicate')">
                                    <td
                                        class="py-4 px-4 w-60 leading-none items-center border-border-one border-r-2 border-t-2"
                                    >
                                        <div
                                            class="float-left"
                                            v-tooltip.top="
                                                t('layout.settings.tooltips.triggersDaemonRestart')
                                            "
                                        >
                                            <svg-icon
                                                type="mdi"
                                                :path="mdiRestart"
                                                :size="deviceStore.getREMSize(1.0)"
                                            />
                                        </div>
                                        <div class="text-right float-right">
                                            {{ t('layout.settings.hideDuplicateDevices') }}
                                        </div>
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-t-2"
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
                                <tr
                                    v-tooltip.right="
                                        'SSDs and HDDs in particular can spin down and enter a low power state.' +
                                        '\nThis option, when enabled and the drive supports it, will report drive temperatures' +
                                        '\nas 0°C when spun down so that fan Profiles can be adjusted accordingly.'
                                    "
                                >
                                    <td
                                        class="py-4 px-4 w-60 leading-none items-center border-border-one border-r-2 border-t-2"
                                    >
                                        <div
                                            class="float-left"
                                            v-tooltip.top="'Triggers an automatic daemon restart'"
                                        >
                                            <svg-icon
                                                type="mdi"
                                                :path="mdiRestart"
                                                :size="deviceStore.getREMSize(1.0)"
                                            />
                                        </div>
                                        <div class="text-right float-right">Drive Power State</div>
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <el-switch
                                            v-model="settingsStore.ccSettings.drivetemp_suspend"
                                            size="large"
                                            @change="applyGenericDaemonChange"
                                        />
                                    </td>
                                </tr>
                            </tbody>
                        </table>
                        <table class="lg:ml-4 h-full bg-bg-two rounded-lg">
                            <tbody>
                                <tr v-tooltip.right="t('layout.settings.tooltips.daemonAddress')">
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        {{ t('common.address') }}
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <InputText
                                            v-model="daemonAddress"
                                            class="min-w-48 w-full text-center"
                                            placeholder="localhost"
                                        />
                                    </td>
                                </tr>
                                <tr v-tooltip.right="t('layout.settings.tooltips.daemonPort')">
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        {{ t('common.port') }}
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <InputNumber
                                            id="port-input"
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
                                <tr v-tooltip.right="t('layout.settings.tooltips.daemonSsl')">
                                    <td
                                        class="py-4 px-4 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        {{ t('common.sslTls') }}
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                                    >
                                        <el-switch v-model="daemonSslEnabled" size="large" />
                                    </td>
                                </tr>
                                <tr>
                                    <td
                                        class="py-4 px-4 w-60 leading-none items-center border-border-one border-r-2 border-t-2"
                                    >
                                        <div
                                            class="float-left py-2"
                                            v-tooltip.top="
                                                t('layout.settings.tooltips.triggersUIRestart')
                                            "
                                        >
                                            <svg-icon
                                                type="mdi"
                                                :path="mdiRestart"
                                                :size="deviceStore.getREMSize(1.0)"
                                            />
                                        </div>
                                        <div class="float-right">
                                            <Button
                                                :label="t('common.defaults')"
                                                class="h-[2.375rem]"
                                                @click="resetDaemonSettings"
                                                v-tooltip.top="
                                                    t('layout.settings.tooltips.resetToDefaults')
                                                "
                                            />
                                        </div>
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <Button
                                            :label="t('common.apply')"
                                            class="bg-accent/80 hover:!bg-accent w-full h-[2.375rem]"
                                            @click="saveDaemonSettings"
                                            v-tooltip.top="
                                                t('layout.settings.tooltips.saveAndReload')
                                            "
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
                                <tr v-tooltip.right="t('layout.settings.tooltips.startInTray')">
                                    <td
                                        class="py-4 px-2 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        {{ t('layout.settings.startInTray') }}
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
                                <tr v-tooltip.right="t('layout.settings.tooltips.closeToTray')">
                                    <td
                                        class="py-4 px-2 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        {{ t('layout.settings.closeToTray') }}
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
                                <tr v-tooltip.right="t('layout.settings.tooltips.zoom')">
                                    <td
                                        class="py-4 px-2 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        {{ t('layout.settings.zoom') }}
                                    </td>
                                    <td
                                        class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <InputNumber
                                            v-model="settingsStore.uiScale"
                                            show-buttons
                                            :min="50"
                                            :max="400"
                                            :suffix="` ${t('common.percentUnit')}`"
                                            :step="10"
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
                                        t('layout.settings.tooltips.desktopStartupDelay')
                                    "
                                >
                                    <td
                                        class="py-4 px-4 w-60 leading-none text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        {{ t('layout.settings.desktopStartupDelay') }}
                                    </td>
                                    <td
                                        class="py-4 px-4 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                                    >
                                        <InputNumber
                                            v-model="settingsStore.desktopStartupDelay"
                                            show-buttons
                                            :min="0"
                                            :max="10"
                                            :suffix="` ${t('common.secondAbbr')}`"
                                            :step="1"
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
                            </tbody>
                        </table>
                    </TabPanel>
                    <TabPanel value="3">
                        <!--ThinkPad Settings-->
                        <table class="bg-bg-two rounded-lg">
                            <tbody>
                                <tr
                                    v-tooltip.right="
                                        t('layout.settings.tooltips.thinkPadFanControl')
                                    "
                                >
                                    <td
                                        class="py-4 px-2 w-60 text-right items-center border-border-one border-r-2 border-b-2"
                                    >
                                        {{ t('layout.settings.fanControl') }}
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
                                        t('layout.settings.tooltips.thinkPadFullSpeed')
                                    "
                                >
                                    <td
                                        class="py-4 px-2 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        {{ t('layout.settings.fullSpeed') }}
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
                    <TabPanel value="4" class="flex flex-col lg:flex-row">
                        <AppSettingsDevices />
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
