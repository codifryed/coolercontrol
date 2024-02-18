<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2023  Guy Boldon
  - |
  - This program is free software: you can redistribute it and/or modify
  - it under the terms of the GNU General Public License as published by
  - the Free Software Foundation, either version 3 of the License, or
  - (at your option) any later version.
  - |
  - This program is distributed in the hope that it will be useful,
  - but WITHOUT ANY WARRANTY; without even the implied warranty of
  - MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  - GNU General Public License for more details.
  - |
  - You should have received a copy of the GNU General Public License
  - along with this program.  If not, see <https://www.gnu.org/licenses/>.
  -->

<script setup lang="ts">
import 'reflect-metadata'
import { RouterView } from 'vue-router'
import ProgressSpinner from 'primevue/progressspinner'
import { onMounted, ref } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import Button from 'primevue/button'
import Toast from 'primevue/toast'
import ConfirmDialog from 'primevue/confirmdialog'
import Dialog from 'primevue/dialog'
import DynamicDialog from 'primevue/dynamicdialog'

const loading = ref(true)
const initSuccessful = ref(true)
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const reloadPage = () => window.location.reload()

/**
 * Startup procedure for the application.
 */
onMounted(async () => {
    const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
    await deviceStore.createDaemonClientWithSettings()
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
            Please make sure that the systemd service is running and available on port 11987.
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
                sudo systemctl start coolercontrold<br />
                sudo systemctl status coolercontrold<br />
            </code>
        </p>
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
