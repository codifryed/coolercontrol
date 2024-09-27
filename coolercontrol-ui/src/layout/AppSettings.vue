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
import { type Ref, ref } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useConfirm } from 'primevue/useconfirm'
import { useToast } from 'primevue/usetoast'
import { ThemeMode } from '@/models/UISettings.ts'
import { CoolerControlDeviceSettingsDTO } from '@/models/CCSettings.ts'
import { mdiDnsOutline, mdiLaptop, mdiMonitor, mdiViewQuiltOutline } from '@mdi/js'
import Tabs from 'primevue/tabs'
import Tab from 'primevue/tab'
import TabList from 'primevue/tablist'
import TabPanels from 'primevue/tabpanels'
import TabPanel from 'primevue/tabpanel'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'radix-vue'
import { ElSwitch } from 'element-plus'
import 'element-plus/es/components/switch/style/css'
import Listbox, { ListboxChangeEvent } from 'primevue/listbox'
import Select from 'primevue/select'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const confirm = useConfirm()
const toast = useToast()

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
]
const changeThemeMode = async (event: ListboxChangeEvent) => {
    if (event.value === null) {
        return // do not update on unselect
    }
    settingsStore.themeMode = event.value
    await deviceStore.waitAndReload(1.1)
}
const lineThicknessOptions = ref([
    { optionSize: 1, value: 0.5 },
    { optionSize: 2, value: 1.0 },
    { optionSize: 3, value: 1.5 },
    { optionSize: 4, value: 2.0 },
    { optionSize: 6, value: 3.0 },
])

</script>

<template>
    <div class="flex h-[3.5rem] border-b-4 border-border-one items-center justify-between">
        <div class="pl-4 py-2 text-xl">Application Settings</div>
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
                    <TabPanel value="0">
                        <!--UI Settings-->
                        <table class="bg-bg-two rounded-lg">
                            <tbody>
                                <tr v-tooltip.right="'Time format: 12-hour (AM/PM) or 24-hour'">
                                    <td
                                        class="py-4 px-2 w-60 text-right items-center border-border-one border-r-2 border-b-2"
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
                                        class="py-4 px-2 w-60 text-right items-center border-border-one border-r-2 border-t-2"
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
                                        class="py-4 px-2 w-60 text-right items-center border-border-one border-r-2 border-t-2"
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
                                        'Control the visibility of menu items. You can choose to show them ' +
                                        'greyed out, or hide them altogether.'
                                    "
                                >
                                    <td
                                        class="py-4 px-2 w-60 text-right items-center border-border-one border-r-2 border-t-2"
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
                                <tr
                                    v-tooltip.right="
                                        'Change the overall UI theme. Choose between light, dark, or ' +
                                        'automatically adjust based on system settings.'
                                    "
                                >
                                    <td
                                        class="py-4 px-2 w-60 text-right items-center border-border-one border-r-2 border-t-2"
                                    >
                                        Theme Style
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
                            </tbody>
                        </table>
                    </TabPanel>
                    <TabPanel value="1">
                        <!--Daemon Settings-->
                        <p>This stuff</p>
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
</style>
