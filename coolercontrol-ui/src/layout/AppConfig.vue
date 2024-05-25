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
import Button from 'primevue/button'
import Sidebar from 'primevue/sidebar'
import SelectButton, { type SelectButtonChangeEvent } from 'primevue/selectbutton'
import Divider from 'primevue/divider'
import InputNumber from 'primevue/inputnumber'
import InputText from 'primevue/inputtext'
import Checkbox from 'primevue/checkbox'
import DataTable from 'primevue/datatable'
import Column from 'primevue/column'
import Accordion from 'primevue/accordion'
import AccordionTab from 'primevue/accordiontab'
import Dropdown from 'primevue/dropdown'

import { type Ref, ref } from 'vue'
import { useLayout } from '@/layout/composables/layout'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useConfirm } from 'primevue/useconfirm'
import { useToast } from 'primevue/usetoast'
import { CoolerControlDeviceSettingsDTO } from '@/models/CCSettings'
import { ThemeMode } from '@/models/UISettings.ts'

defineProps({
    simple: {
        type: Boolean,
        default: true,
    },
})

const scales = ref([50, 75, 100, 125, 150])
const { setScale, layoutConfig, isConfigSidebarActive } = useLayout()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const confirm = useConfirm()
const toast = useToast()
const appVersion = import.meta.env.PACKAGE_VERSION

const decrementScale = () => {
    setScale(layoutConfig.scale.value - 25)
    settingsStore.uiScale = layoutConfig.scale.value
}
const incrementScale = () => {
    setScale(layoutConfig.scale.value + 25)
    settingsStore.uiScale = layoutConfig.scale.value
}

const applyThinkPadFanControl = (event: SelectButtonChangeEvent) => {
    settingsStore.applyThinkPadFanControl(event.value)
}

const enabledOptions = [
    { value: true, label: 'Enabled' },
    { value: false, label: 'Disabled' },
]
const showOptions = [
    { value: true, label: 'Show' },
    { value: false, label: 'Hide' },
]
const menuLayoutOptions = ['static', 'overlay']
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
const noInitOptions = [
    { value: false, label: 'Enabled' },
    { value: true, label: 'Disabled' },
]
const timeOptions = [
    { value: false, label: '12-hr' },
    { value: true, label: '24-hr' },
]

const blacklistedDevices: Ref<Array<CoolerControlDeviceSettingsDTO>> = ref([])
for (const deviceSettings of settingsStore.ccBlacklistedDevices.values()) {
    blacklistedDevices.value.push(deviceSettings)
}
const selectedBlacklistedDevices: Ref<Array<CoolerControlDeviceSettingsDTO>> = ref([])
const reEnableSelected = () => {
    if (selectedBlacklistedDevices.value.length === 0) {
        return
    }
    confirm.require({
        message:
            'Re-enabling these devices requires a daemon and UI restart. Are you you want to do this now?',
        header: 'Re-enable Devices',
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
                    detail: 'Devices re-enabled. Restarting now',
                    life: 3000,
                })
                await deviceStore.daemonClient.shutdownDaemon()
                await deviceStore.waitAndReload()
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

const restartDaemon = () => {
    confirm.require({
        message: 'Are you sure you want to restart the daemon and the UI?',
        header: 'Daemon Restart',
        icon: 'pi pi-exclamation-triangle',
        accept: async () => {
            const successful = await deviceStore.daemonClient.shutdownDaemon()
            if (successful) {
                toast.add({
                    severity: 'success',
                    summary: 'Success',
                    detail: 'Daemon shutdown signal accepted',
                    life: 3000,
                })
                await deviceStore.waitAndReload()
            } else {
                toast.add({
                    severity: 'error',
                    summary: 'Error',
                    detail: 'Unknown error sending shutdown signal. See logs for details.',
                    life: 4000,
                })
            }
        },
    })
}
</script>

<template>
    <Sidebar
        v-model:visible="isConfigSidebarActive"
        position="right"
        :transitionOptions="'.3s cubic-bezier(0, 0, 0.2, 1)'"
        class="layout-config-sidebar"
    >
        <h3 style="font-family: rounded">
            CoolerControl
            <span style="font-size: 60%">v{{ appVersion }}</span>
            <Divider class="m-0 w-9" />
        </h3>
        <p style="font-size: small; font-style: italic">
            This program comes with absolutely no warranty.
        </p>

        <Accordion :active-index="0">
            <AccordionTab header="UI">
                <h6>
                    Scale
                    <Divider class="mt-1 mb-0" />
                </h6>
                <div class="flex align-items-center">
                    <Button
                        icon="pi pi-minus"
                        type="button"
                        @click="decrementScale()"
                        class="p-button-text p-button-rounded w-2rem h-2rem mr-2"
                        :disabled="layoutConfig.scale.value === scales[0]"
                    ></Button>
                    <div class="flex gap-2 align-items-center">
                        <i
                            class="pi pi-circle-fill text-300"
                            v-for="s in scales"
                            :key="s"
                            :class="{ 'text-primary-500': s === layoutConfig.scale.value }"
                        ></i>
                    </div>
                    <Button
                        icon="pi pi-plus"
                        type="button"
                        pButton
                        @click="incrementScale()"
                        class="p-button-text p-button-rounded w-2rem h-2rem ml-2"
                        :disabled="layoutConfig.scale.value === scales[scales.length - 1]"
                    ></Button>
                </div>

                <h6>
                    Menu Type
                    <Divider class="mt-1 mb-0" />
                </h6>
                <div class="flex">
                    <SelectButton
                        v-model="layoutConfig.menuMode.value"
                        :options="menuLayoutOptions"
                        @change="(event) => (settingsStore.menuMode = event.value)"
                        :option-label="(value: string) => deviceStore.toTitleCase(value)"
                        :allow-empty="false"
                        v-tooltip.left="
                            'Whether the main menu remains static or acts as a movable overlay.'
                        "
                    />
                </div>

                <h6>
                    Theme Style
                    <Divider class="mt-1 mb-0" />
                </h6>
                <div class="flex">
                    <Dropdown
                        v-model="settingsStore.themeMode"
                        :options="themeModeOptions"
                        option-label="label"
                        option-value="value"
                        checkmark="true"
                        @change="async () => await deviceStore.waitAndReload(0.2)"
                    />
                </div>

                <h6>
                    Time Format
                    <Divider class="mt-1 mb-0" />
                </h6>
                <div class="flex">
                    <SelectButton
                        v-model="settingsStore.time24"
                        :options="timeOptions"
                        option-label="label"
                        option-value="value"
                        :allow-empty="false"
                        @change="async () => await deviceStore.waitAndReload(0.2)"
                        v-tooltip.left="
                            'Whether to display time in 12-hour or 24-hour format for time charts.'
                        "
                    />
                </div>

                <h6>
                    Show Hidden Menu Items
                    <Divider class="mt-1 mb-0" />
                </h6>
                <div class="flex">
                    <SelectButton
                        v-model="settingsStore.displayHiddenItems"
                        :options="showOptions"
                        option-label="label"
                        option-value="value"
                        :allow-empty="false"
                        v-tooltip.left="
                            'Whether to show hidden items in the main menu, or to remove them.'
                        "
                    />
                </div>
            </AccordionTab>

            <AccordionTab header="Daemon">
                <h6>
                    Apply Settings on System Boot
                    <Divider class="mt-1 mb-0" />
                </h6>
                <div class="flex">
                    <SelectButton
                        v-model="settingsStore.ccSettings.apply_on_boot"
                        :options="enabledOptions"
                        option-label="label"
                        option-value="value"
                        :allow-empty="false"
                        v-tooltip.left="
                            'Whether to apply your settings automatically when the daemon starts'
                        "
                    />
                </div>

                <h6>
                    Liquidctl Device Initialization
                    <Divider class="mt-1 mb-0" />
                </h6>
                <div class="flex">
                    <SelectButton
                        v-model="settingsStore.ccSettings.no_init"
                        :options="noInitOptions"
                        option-label="label"
                        option-value="value"
                        :allow-empty="false"
                        v-tooltip.left="
                            'Disabling this can help avoid conflicts with other programs that also control ' +
                            'your liquidctl devices. Most devices require this step for proper communication and should ' +
                            'only be disabled with care.'
                        "
                    />
                </div>

                <h6>
                    Boot-Up Delay
                    <Divider class="mt-1 mb-0" />
                </h6>
                <div class="flex">
                    <InputNumber
                        v-model="settingsStore.ccSettings.startup_delay"
                        showButtons
                        :min="1"
                        :max="10"
                        suffix=" seconds"
                        class=""
                        :input-style="{ width: '10rem' }"
                        v-tooltip.left="
                            'The number of seconds the daemon waits before attempting to communicate ' +
                            'with devices. This can be helpful when dealing with devices that aren\'t consistently detected' +
                            ' or need extra time to fully initialize.'
                        "
                    />
                </div>

                <h6>
                    Blacklisted Devices
                    <Divider class="mt-1 mb-0" />
                </h6>
                <div v-if="blacklistedDevices.length > 0" class="flex mb-2">
                    <Button
                        label="Re-Enable selected"
                        v-tooltip.left="
                            'This will re-enable the selected blacklisted devices. ' +
                            'This requires a restart of the daemon and UI.'
                        "
                        @click="reEnableSelected"
                        :disabled="selectedBlacklistedDevices.length === 0"
                    />
                </div>
                <div v-if="blacklistedDevices.length > 0" class="flex">
                    <DataTable
                        v-model:selection="selectedBlacklistedDevices"
                        :value="blacklistedDevices"
                        show-gridlines
                        data-key="uid"
                    >
                        <Column selection-mode="multiple" header-style="width: 3rem" />
                        <Column field="name" header="Device Name" />
                    </DataTable>
                </div>
                <span v-else style="font-style: italic">None</span>

                <h6>
                    Daemon Connection Address
                    <Divider class="mt-1 mb-0" />
                </h6>
                <div class="w-12">
                    <InputText
                        v-model="daemonAddress"
                        class="mb-2 w-full"
                        v-tooltip.left="
                            'The IP address to use to communicate with the daemon. ' +
                            'This can be an IPv4 or IPv6 address.'
                        "
                    />
                    <InputNumber
                        v-model="daemonPort"
                        showButtons
                        :min="80"
                        :max="65535"
                        :useGrouping="false"
                        class="mb-2"
                        :input-style="{ width: '10rem' }"
                        v-tooltip.left="'The port to use to communicate with the daemon'"
                    />
                    <div class="mb-3">
                        <Checkbox
                            v-model="daemonSslEnabled"
                            inputId="ssl-enable"
                            :binary="true"
                            v-tooltip.left="'Whether to connect to the daemon using SSL/TLS'"
                        />
                        <label for="ssl-enable" class="ml-2"> SSL/TLS </label>
                    </div>
                    <div>
                        <Button
                            label="Save and Refresh"
                            class="mb-2"
                            v-tooltip.left="'Saves the daemon settings and reloads the UI.'"
                            @click="saveDaemonSettings"
                        />
                    </div>
                    <Button
                        label="Reset"
                        v-tooltip.left="
                            'Resets the daemon settings to their defaults and reloads the UI.'
                        "
                        @click="resetDaemonSettings"
                    />
                </div>

                <h6>
                    Restart systemd Daemon
                    <Divider class="mt-1 mb-0" />
                </h6>
                <div class="flex">
                    <Button
                        @click="restartDaemon"
                        label="Restart Daemon"
                        v-tooltip.left="
                            'This will send a shutdown signal to the daemon and systemd will automatically restart it. Note that ' +
                            'this will re-detect all your devices and clear all sensor data. This will also restart the UI to re-establish ' +
                            'the connection.'
                        "
                    />
                </div>
            </AccordionTab>

            <AccordionTab header="Desktop Application" :disabled="!deviceStore.isTauriApp()">
                <h6>
                    Start in Tray
                    <Divider class="mt-1 mb-0" />
                </h6>
                <div class="flex">
                    <SelectButton
                        v-model="settingsStore.startInSystemTray"
                        :options="enabledOptions"
                        :disabled="!deviceStore.isTauriApp()"
                        option-label="label"
                        option-value="value"
                        :allow-empty="false"
                        v-tooltip.left="
                            'Upon startup, the main UI window will be hidden and only ' +
                            'the system tray icon will be visible.'
                        "
                    />
                </div>
                <h6>
                    Close to Tray
                    <Divider class="mt-1 mb-0" />
                </h6>
                <div class="flex">
                    <SelectButton
                        v-model="settingsStore.closeToSystemTray"
                        :options="enabledOptions"
                        :disabled="!deviceStore.isTauriApp()"
                        option-label="label"
                        option-value="value"
                        :allow-empty="false"
                        v-tooltip.left="
                            'Closing the application window will leave the app running in the system tray'
                        "
                    />
                </div>
                <h6>
                    Start-Up Delay
                    <Divider class="mt-1 mb-0" />
                </h6>
                <div class="flex">
                    <InputNumber
                        v-model="settingsStore.desktopStartupDelay"
                        showButtons
                        :min="0"
                        :max="10"
                        suffix=" seconds"
                        :input-style="{ width: '10rem' }"
                        v-tooltip.left="
                            'The number of seconds the UI will wait at startup before attempting to ' +
                            'create a UI window. This can be helpful to address UI issues ' +
                            'when CoolerControl is auto-started on Desktop Login'
                        "
                    />
                </div>
            </AccordionTab>

            <AccordionTab header="ThinkPad" :disabled="!deviceStore.isThinkPad">
                <h6>
                    ThinkPad Full Speed
                    <Divider class="mt-1 mb-0" />
                </h6>
                <div class="flex">
                    <SelectButton
                        v-model="settingsStore.ccSettings.thinkpad_full_speed"
                        :options="enabledOptions"
                        option-label="label"
                        option-value="value"
                        :allow-empty="false"
                        :disabled="!deviceStore.isThinkPad"
                        v-tooltip.left="
                            'For Thinkpad Laptops this enables Full-Speed mode. This allows the fans to ' +
                            'spin up to their absolute maximum when set to 100%, but will run the fans out of ' +
                            'specification and cause increased wear. Use with caution.'
                        "
                    />
                </div>

                <h6>
                    ThinkPad Fan Control
                    <Divider class="mt-1 mb-0" />
                </h6>
                <div class="flex">
                    <SelectButton
                        v-model="settingsStore.thinkPadFanControlEnabled"
                        :options="enabledOptions"
                        @change="applyThinkPadFanControl"
                        option-label="label"
                        option-value="value"
                        :allow-empty="false"
                        :disabled="!deviceStore.isThinkPad"
                        v-tooltip.left="
                            'This is a helper to enable ThinkPad ACPI Fan Control. Fan control operations are disabled by ' +
                            'default for safety reasons. CoolerControl can try to enable this for you, but you should be aware of the risks ' +
                            'to your hardware. Proceed at your own risk.'
                        "
                    />
                </div>
            </AccordionTab>
        </Accordion>
    </Sidebar>
</template>

<style lang="scss" scoped></style>
