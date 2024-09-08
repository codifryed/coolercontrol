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
import { mdiSyncOff } from '@mdi/js'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import Button from 'primevue/button'
import type { UID } from '@/models/Device.ts'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useConfirm } from 'primevue/useconfirm'
import { CoolerControlDeviceSettingsDTO } from '@/models/CCSettings.ts'
import { useToast } from 'primevue/usetoast'

interface Props {
    deviceUID: UID
}

const props = defineProps<Props>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const confirm = useConfirm()
const toast = useToast()

const disableDevice = (): void => {
    if (!settingsStore.ccDeviceSettings.has(props.deviceUID)) {
        console.error(`CCDeviceSetting not found for this device: ${props.deviceUID}`)
        return
    }
    const ccSetting: CoolerControlDeviceSettingsDTO = settingsStore.ccDeviceSettings.get(
        props.deviceUID,
    )!
    confirm.require({
        message:
            'Disabled Devices can be re-enable later in the settings menu. ' +
            'Are you sure you want to restart and proceed?',
        header: 'Disable Device Sync',
        icon: 'pi pi-exclamation-triangle',
        accept: async () => {
            ccSetting.disable = true
            const successful = await deviceStore.daemonClient.saveCCDeviceSettings(
                ccSetting.uid,
                ccSetting,
            )
            if (successful) {
                toast.add({
                    severity: 'success',
                    summary: 'Success',
                    detail: 'Device Sync Disabled. Restarting now',
                    life: 3000,
                })
                await deviceStore.daemonClient.shutdownDaemon()
                await deviceStore.waitAndReload()
            } else {
                toast.add({
                    severity: 'error',
                    summary: 'Error',
                    detail: 'Unknown error trying to disable a device sync. See logs for details.',
                    life: 4000,
                })
            }
        },
    })
}
</script>

<template>
    <div v-tooltip.top="{ value: 'Disable Device Sync' }">
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0.5 text-text-color-secondary hover:text-text-color"
            @click="disableDevice"
        >
            <svg-icon type="mdi" :path="mdiSyncOff" :size="deviceStore.getREMSize(1.75)" />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
