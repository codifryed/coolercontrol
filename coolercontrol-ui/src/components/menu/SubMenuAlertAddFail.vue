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
import { inject } from 'vue'
import { Emitter, EventType } from 'mitt'
import { UID } from '@/models/Device.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useRouter } from 'vue-router'
import { ChannelMetric, ChannelSource } from '@/models/ChannelSource.ts'
import { Alert } from '@/models/Alert.ts'

const props = defineProps<{
    deviceUID: UID
    channelName: string
}>()
const emit = defineEmits<{
    (e: 'close'): void
}>()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const router = useRouter()
const { t } = useI18n()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!

const createFailAlert = async (): Promise<void> => {
    const deviceSettings = settingsStore.allUIDeviceSettings.get(props.deviceUID)!
    const channelName =
        deviceSettings.sensorsAndChannels.get(props.channelName)?.name ?? props.channelName
    const newAlertName = `${t('views.alerts.createFailAlert')} - ${channelName}`
    const channelSource = new ChannelSource(props.deviceUID, props.channelName, ChannelMetric.RPM)
    // standard max rpm speed & if < 100 rpm fire it w/ no warmup:
    const alert = new Alert(newAlertName, channelSource, 100, 10_000, 0.0)
    const successful = await settingsStore.createAlert(alert)
    if (successful) {
        await settingsStore.loadAlertsAndLogs()
        emitter.emit('alert-add-menu', { alertUID: alert.uid })
        await router.push({ name: 'alerts', params: { alertUID: alert.uid } })
    }
    emit('close')
}
</script>

<template>
    <Button
        class="w-full !justify-start !rounded-lg border-none text-text-color-secondary h-12 !p-4 !px-7 hover:text-text-color hover:bg-surface-hover outline-none"
        @click.stop.prevent="createFailAlert"
    >
        <svg-icon type="mdi" :path="mdiBellPlusOutline" :size="deviceStore.getREMSize(1.5)" />
        <span class="ml-1.5">
            {{ t('views.alerts.createFailAlert') }}
        </span>
    </Button>
</template>

<style scoped lang="scss"></style>
