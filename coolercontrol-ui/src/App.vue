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
import 'reflect-metadata'
import { RouterView } from 'vue-router'
import ProgressSpinner from 'primevue/progressspinner'
import { Ref, onMounted, ref } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import Button from 'primevue/button'
import Toast from 'primevue/toast'
import ConfirmDialog from 'primevue/confirmdialog'
import Dialog from 'primevue/dialog'
import DynamicDialog from 'primevue/dynamicdialog'
import InputNumber from 'primevue/inputnumber'
import InputText from 'primevue/inputtext'
import Checkbox from 'primevue/checkbox'

const loading = ref(true)
const initSuccessful = ref(true)
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const reloadPage = () => window.location.reload()

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

/**
 * Startup procedure for the application.
 */
onMounted(async () => {
    const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
    initSuccessful.value = await deviceStore.initializeDevices()
    if (!initSuccessful.value) {
        return
    }
    await settingsStore.initializeSettings(deviceStore.allDevices())
    await sleep(200) // give the engine a moment to catch up for a smoother start
    loading.value = false
    await deviceStore.login()

    const delay = () => new Promise((resolve) => setTimeout(resolve, 200))
    let timeStarted = Date.now()
    while (true) {
        // this will be automatically paused by the browser when going inactive/sleep
        if (Date.now() - timeStarted >= 1000) {
            timeStarted = Date.now()
            await deviceStore.updateStatus()
        }
        await delay()
    }
})
</script>

<template>
    <div v-if="loading">
        <div
            class="flex align-items-center align-items-stretch flex-wrap"
            style="min-height: 100vh"
        >
            <ProgressSpinner />
        </div>
    </div>
    <RouterView v-else />
    <Toast />
    <DynamicDialog />
    <ConfirmDialog
        :pt="{
            mask: {
                style: 'backdrop-filter: blur(2px); -webkit-backdrop-filter: blur(2px);',
            },
        }"
    />
    <ConfirmDialog
        group="AseTek690"
        :pt="{
            mask: {
                style: 'backdrop-filter: blur(2px); -webkit-backdrop-filter: blur(2px);',
            },
        }"
    >
        <template #message="slotProps">
            <div
                class="flex flex-column align-items-left w-30rem gap-3 border-bottom-1 surface-border"
            >
                <p>
                    The legacy NZXT Krakens and the EVGA CLC happen to have the same device ID and
                    CoolerControl can not determine which device is connected. This is required for
                    proper device communication.
                </p>
                <p>
                    A restart of the CoolerControl systemd services may be required and will be
                    handled automatically if needed.
                </p>
                <p>
                    Is Liquidctl Device <strong>#{{ slotProps.message.message }}</strong> one of the
                    following models?<br />
                    NZXT Kraken X40, X60, X31, X41, X51 or X61
                </p>
            </div>
        </template>
    </ConfirmDialog>
    <Dialog
        :visible="!initSuccessful"
        header="CoolerControl Connection Error"
        :style="{ width: '50vw' }"
    >
        <p>
            A connection to the CoolerControl Daemon could not be established. <br />
            Please make sure that the systemd service is running and available.
        </p>
        <p>
            Check the
            <a
                href="https://gitlab.com/coolercontrol/coolercontrol/"
                style="color: var(--cc-context-color)"
            >
                project page</a
            >
            for installation instructions.
        </p>
        <p>Some helpful commands:</p>
        <p>
            <code>
                sudo systemctl enable --now coolercontrold<br />
                sudo systemctl status coolercontrold<br />
            </code>
        </p>
        <hr />
        <p>
            If you have configured a non-standard address to connect to the daemon, you can set it
            here:
        </p>
        <h6 v-if="deviceStore.isTauriApp()">Daemon Address - Desktop App</h6>
        <h6 v-else>Daemon Address - Web UI</h6>
        <div>
            <div>
                <InputText
                    v-model="daemonAddress"
                    class="mb-2 w-6"
                    :input-style="{ width: '10rem' }"
                    v-tooltip.right="
                        'The IP address to use to communicate with the daemon. ' +
                        'This can be an IPv4 or IPv6 address.'
                    "
                />
            </div>
            <InputNumber
                v-model="daemonPort"
                showButtons
                :min="80"
                :max="65535"
                :useGrouping="false"
                class="mb-2"
                :input-style="{ width: '10rem' }"
                v-tooltip.right="'The port to use to communicate with the daemon'"
            />
            <div class="mb-3">
                <Checkbox
                    v-model="daemonSslEnabled"
                    inputId="ssl-enable"
                    :binary="true"
                    v-tooltip.right="'Whether to connect to the daemon using SSL/TLS'"
                />
                <label for="ssl-enable" class="ml-2"> SSL/TLS </label>
            </div>
            <div>
                <Button
                    label="Save and Refresh"
                    class="mb-2"
                    v-tooltip.right="'Saves the daemon settings and reloads the UI.'"
                    @click="saveDaemonSettings"
                />
            </div>
            <Button
                label="Reset"
                v-tooltip.right="'Resets the daemon settings to their defaults and reloads the UI.'"
                @click="resetDaemonSettings"
            />
        </div>
        <template #footer>
            <Button label="Retry" icon="pi pi-refresh" @click="reloadPage" />
        </template>
    </Dialog>
</template>

<style>
@font-face {
    font-family: 'rounded';
    font-style: normal;
    font-weight: normal;
    src:
        local('Rounded Elegance Regular'),
        url('/Rounded_Elegance.woff') format('woff');
}

#app {
    /* Foreground, Background */
    scrollbar-color: var(--cc-context-pressed) var(--cc-bg-two);
}

::-webkit-scrollbar {
    width: 8px;
}

/* Track */
::-webkit-scrollbar-track {
    /* Background */
    -webkit-box-shadow: inset 0 0 4px rgba(0, 0, 0, 0.3);
    border-radius: 6px;
    background: var(--cc-bg-two);
}

/* Handle */
::-webkit-scrollbar-thumb {
    /* Foreground */
    border-radius: 6px;
    -webkit-box-shadow: inset 0 0 4px rgba(0, 0, 0, 0.3);
    background: var(--cc-context-pressed);
}

/* Handle on hover */
::-webkit-scrollbar-thumb:hover {
    /* Foreground Hover */
    background: var(--cc-context-color);
}
</style>
