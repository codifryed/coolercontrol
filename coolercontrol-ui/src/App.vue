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
import { ElLoading } from 'element-plus'
import 'element-plus/es/components/loading/style/css'

const loaded: Ref<boolean> = ref(false)
const initSuccessful = ref(true)
const showSetupInstructions = ref(false)
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
const loading = ElLoading.service({
    lock: true,
    text: 'Connecting...',
    background: 'rgb(var(--colors-bg-one))',
})

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
    await sleep(300) // give the engine a moment to catch up for a smoother start
    loaded.value = true
    loading.close()
    await deviceStore.login()

    const loopTickMS = 1000
    let timeStarted = Date.now()
    while (true) {
        // this will be automatically paused by the browser when going inactive/sleep
        const waitTime = Math.max(0, loopTickMS - (Date.now() - timeStarted))
        await sleep(waitTime)
        timeStarted = Date.now()
        await deviceStore.updateStatus()
    }
})
</script>

<template>
    <RouterView v-if="loaded" />
    <Toast />
    <DynamicDialog />
    <ConfirmDialog
        :pt="{
            mask: {
                style: 'backdrop-filter: blur(2px); -webkit-backdrop-filter: blur(2px);',
            },
        }"
    >
        <template #message="slotProps">
            <div class="flex flex-col items-center">
                <i
                    v-if="slotProps.message.icon"
                    class="text-text-color-secondary text-4xl mb-2"
                    :class="slotProps.message.icon"
                />
                <p class="w-96">{{ slotProps.message.message }}</p>
            </div>
        </template>
    </ConfirmDialog>
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
    <Dialog
        :visible="showSetupInstructions"
        header="Welcome to CoolerControl!"
        :style="{ width: '75vw' }"
    >
        <h5>Important Information</h5>
        <p>
            CoolerControl depends on open source drivers to communicate with your hardware.<br /><br />

            If CoolerControl does not list or cannot control your fans, then likely there is an
            issue with your currently installed kernel drivers.<br /><br />

            Before opening an issue, please confirm that all drivers have been properly loaded by
            checking
            <a
                href="https://gitlab.com/coolercontrol/coolercontrol/-/wikis/HWMon-Support"
                style="color: var(--cc-context-color)"
            >
                HWMon Support
            </a>
            and
            <a
                href="https://gitlab.com/coolercontrol/coolercontrol/-/wikis/adding-device-support"
                style="color: var(--cc-context-color)"
            >
                Adding Device Support</a
            >.<br /><br />

            Note that this popup is simply a reminder and does not signify any problems with your
            system.
        </p>

        <template #footer>
            <Button label="Remind me later" @click="() => (showSetupInstructions = false)" />
            <Button
                label="Do not show again (I know what I'm doing)"
                @click="
                    () => {
                        showSetupInstructions = false
                        settingsStore.showSetupInstructions = false
                    }
                "
            />
        </template>
    </Dialog>
</template>

<style>
:root {
    --el-color-primary: #568af2;
}
</style>
