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
import { mdiDeleteOutline } from '@mdi/js'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { UID } from '@/models/Device.ts'
import { useConfirm } from 'primevue/useconfirm'
import { useI18n } from 'vue-i18n'

interface Props {
    alertUID: UID
}

const emit = defineEmits<{
    (e: 'deleted', alertUID: UID): void
    (e: 'close'): void
}>()

const props = defineProps<Props>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const confirm = useConfirm()
const { t } = useI18n()

const deleteAlert = (): void => {
    const alertUIDToDelete: UID = props.alertUID
    const alertIndex: number = settingsStore.alerts.findIndex(
        (alert) => alert.uid === alertUIDToDelete,
    )
    if (alertIndex === -1) {
        console.error('Alert not found for removal: ' + alertUIDToDelete)
        emit('close')
        return
    }
    const alertName = settingsStore.alerts[alertIndex].name
    confirm.require({
        message: t('views.alerts.deleteAlertConfirm', { name: alertName }),
        header: t('views.alerts.deleteAlert'),
        icon: 'pi pi-exclamation-triangle',
        accept: async () => {
            const successful = await settingsStore.deleteAlert(alertUIDToDelete)
            if (successful) emit('deleted', alertUIDToDelete)
            emit('close')
        },
        reject: () => {
            emit('close')
        },
    })
}
</script>

<template>
    <Button
        class="w-full !justify-start !rounded-lg border-none text-text-color-secondary h-12 !p-4 !px-7 hover:text-text-color hover:bg-surface-hover outline-none"
        @click.stop.prevent="deleteAlert"
    >
        <svg-icon type="mdi" :path="mdiDeleteOutline" :size="deviceStore.getREMSize(1.5)" />
        <span class="ml-1.5">
            {{ t('views.alerts.deleteAlert') }}
        </span>
    </Button>
</template>

<style scoped lang="scss"></style>
