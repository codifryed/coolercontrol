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
import { mdiArrowLeft, mdiContentSaveOutline } from '@mdi/js'
import InputNumber from 'primevue/inputnumber'
import Slider from 'primevue/slider'
import { ref, Ref } from 'vue'
import { storeToRefs } from 'pinia'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { UID } from '@/models/Device.ts'
import Button from 'primevue/button'
import { DeviceSettingWriteManualDTO } from '@/models/DaemonSettings.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { v4 as uuidV4 } from 'uuid'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
const deviceStore = useDeviceStore()
const { currentDeviceStatus } = storeToRefs(deviceStore)
const settingsStore = useSettingsStore()
const router = useRouter()

interface Props {
    deviceUID: UID
    channelName: string
}

const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'close'): void
}>()
const props = defineProps<Props>()
const getCurrentDuty = (): number | undefined => {
    const duty = currentDeviceStatus.value.get(props.deviceUID)?.get(props.channelName)?.duty
    return duty != null ? Number(duty) : undefined
}
const manualDuty: Ref<number> = ref(getCurrentDuty() || 0)
let dutyMin = 0
let dutyMax = 100
for (const device of deviceStore.allDevices()) {
    if (device.uid === props.deviceUID && device.info != null) {
        const channelInfo = device.info.channels.get(props.channelName)
        if (channelInfo != null && channelInfo.speed_options != null) {
            dutyMin = channelInfo.speed_options.min_duty
            dutyMax = channelInfo.speed_options.max_duty
        }
    }
}

const saveSetting = async () => {
    if (manualDuty.value == null) {
        return
    }
    const setting = new DeviceSettingWriteManualDTO(manualDuty.value)
    await settingsStore.saveDaemonDeviceSettingManual(props.deviceUID, props.channelName, setting)
    emit('close')
    await router.push({
        name: 'device-speed',
        params: { deviceUID: props.deviceUID, channelName: props.channelName },
        query: { key: uuidV4() },
    })
}
</script>

<template>
    <div class="flex flex-col gap-y-4 w-96">
        <div class="w-full">{{ t('components.wizards.fanControl.selectSpeed') }}:</div>
        <InputNumber
            :placeholder="t('common.duty')"
            v-model="manualDuty"
            mode="decimal"
            class="duty-input w-full bg-bg-two"
            :suffix="` ${t('common.percentUnit')}`"
            showButtons
            :min="dutyMin"
            :max="dutyMax"
            :use-grouping="false"
            :step="1"
            button-layout="horizontal"
            :input-style="{ width: '8rem', background: 'var(--bg-bg-two)' }"
        >
            <template #incrementicon>
                <span class="pi pi-plus" />
            </template>
            <template #decrementicon>
                <span class="pi pi-minus" />
            </template>
        </InputNumber>
        <Slider
            v-model="manualDuty"
            class="!w-[23.25rem] ml-1.5"
            :step="1"
            :min="dutyMin"
            :max="dutyMax"
        />
        <div class="flex flex-row justify-between mt-4">
            <Button class="w-24 h-[2.375rem]" label="Back" @click="emit('nextStep', 1)">
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiArrowLeft"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
            <Button
                class="bg-accent/80 hover:!bg-accent w-32 h-[2.375rem]"
                :label="t('common.apply')"
                v-tooltip.bottom="t('views.speed.applySetting')"
                @click="saveSetting"
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
