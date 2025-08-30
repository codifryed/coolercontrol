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
import { mdiBellPlusOutline } from '@mdi/js'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useI18n } from 'vue-i18n'
import { defineAsyncComponent } from 'vue'
import { UID } from '@/models/Device.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { ChannelMetric, ChannelSource } from '@/models/ChannelSource.ts'
import { Alert } from '@/models/Alert.ts'
import { useDialog } from 'primevue/usedialog'

const props = defineProps<{
    deviceUID: UID
    channelName: string
}>()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { t } = useI18n()
const emit = defineEmits<{
    (e: 'close'): void
}>()
const dialog = useDialog()
const alertWizard = defineAsyncComponent(() => import('../wizards/alert/Wizard.vue'))

const createTempAlert = async (): Promise<void> => {
    const deviceSettings = settingsStore.allUIDeviceSettings.get(props.deviceUID)!
    const channelName =
        deviceSettings.sensorsAndChannels.get(props.channelName)?.name ?? props.channelName
    const newAlertName = `${channelName}`
    const channelSource = new ChannelSource(props.deviceUID, props.channelName, ChannelMetric.Temp)
    const newAlert = new Alert(
        newAlertName,
        channelSource,
        0,
        200,
        settingsStore.ccSettings.poll_rate,
    )
    dialog.open(alertWizard, {
        props: {
            header: t('views.alerts.newAlert'),
            position: 'center',
            modal: true,
            dismissableMask: true,
        },
        data: {
            alert: newAlert,
        },
    })
    emit('close')
}
</script>

<template>
    <Button
        class="w-full !justify-start !rounded-lg border-none text-text-color-secondary h-12 !p-4 !px-7 hover:text-text-color hover:bg-surface-hover outline-none"
        @click.stop.prevent="createTempAlert"
    >
        <svg-icon type="mdi" :path="mdiBellPlusOutline" :size="deviceStore.getREMSize(1.5)" />
        <span class="ml-1.5">
            {{ t('views.alerts.createAlert') }}
        </span>
    </Button>
</template>

<style scoped lang="scss"></style>
