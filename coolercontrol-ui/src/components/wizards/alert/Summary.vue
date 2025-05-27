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
import SvgIcon from '@jamescoyle/vue-icon'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { mdiArrowLeft, mdiContentSaveOutline } from '@mdi/js'
import Button from 'primevue/button'
import { Alert } from '@/models/Alert.ts'
import { Emitter, EventType } from 'mitt'
import { inject } from 'vue'
import { useRouter } from 'vue-router'

interface Props {
    alert: Alert
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'close'): void
}>()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!

const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const router = useRouter()

const saveAlert = async (): Promise<void> => {
    const successful = await settingsStore.createAlert(props.alert)
    if (successful) {
        await settingsStore.loadAlertsAndLogs()
        emitter.emit('alert-add-menu', { alertUID: props.alert.uid })
        emit('close')
        await router.push({ name: 'alerts', params: { alertUID: props.alert.uid } })
    }
}
</script>

<template>
    <div class="flex flex-col justify-between min-w-96 w-[40vw] min-h-max h-[40vh]">
        <div class="flex flex-col gap-y-4">
            <span class="text-xl text-center underline">{{
                t('components.wizards.fanControl.summary')
            }}</span>
            <div class="w-full text-lg">
                <p>
                    {{ t('views.alerts.newAlert') }}:
                    <span class="font-bold">{{ props.alert.name }}</span>
                    <br />
                    {{ t('components.wizards.profile.willCreated') }}
                </p>
            </div>
        </div>
        <div class="flex flex-row justify-between mt-4">
            <Button class="w-24 bg-bg-one" label="Back" @click="emit('nextStep', 1)">
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiArrowLeft"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
            <Button
                class="bg-accent/80 hover:!bg-accent w-32"
                :label="t('common.apply')"
                v-tooltip.bottom="t('views.speed.applySetting')"
                @click="saveAlert"
            >
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiContentSaveOutline"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
