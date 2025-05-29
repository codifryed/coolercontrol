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
import { mdiArrowLeft } from '@mdi/js'
import { ref, Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import Button from 'primevue/button'
import InputNumber from 'primevue/inputnumber'
import Slider from 'primevue/slider'

interface Props {
    name: string
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'speedFixed', speed: number): void
}>()

const { t } = useI18n()
const deviceStore = useDeviceStore()

const manualDuty: Ref<number> = ref(0)
const dutyMin = 0
const dutyMax = 100

const nextStep = async (): Promise<void> => {
    emit('speedFixed', manualDuty.value)
    emit('nextStep', 13)
}
</script>

<template>
    <div class="flex flex-col justify-between min-w-96 w-[40vw] min-h-max h-[40vh]">
        <div class="flex flex-col gap-y-4">
            <div class="w-full text-lg">
                {{ t('components.wizards.fanControl.newFixedProfile') }}:
                <span class="font-bold">{{ props.name }}</span
                ><br />
                {{ t('components.wizards.fanControl.withSettings') }}:
            </div>
            <InputNumber
                :placeholder="t('common.duty')"
                v-model="manualDuty"
                mode="decimal"
                class="duty-input h-11 w-full"
                :suffix="` ${t('common.percentUnit')}`"
                showButtons
                :min="dutyMin"
                :max="dutyMax"
                :use-grouping="false"
                :step="1"
                button-layout="horizontal"
                :input-style="{ width: '8rem', background: 'rgb(var(--colors-bg-one))' }"
            >
                <template #incrementicon>
                    <span class="pi pi-plus" />
                </template>
                <template #decrementicon>
                    <span class="pi pi-minus" />
                </template>
            </InputNumber>
            <div class="mx-1.5 mt-0">
                <Slider
                    v-model="manualDuty"
                    class="!w-full"
                    :step="1"
                    :min="dutyMin"
                    :max="dutyMax"
                />
            </div>
        </div>
        <div class="flex flex-row justify-between mt-4">
            <Button class="w-24 bg-bg-one" label="Back" @click="emit('nextStep', 3)">
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiArrowLeft"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
            <Button class="w-24 bg-bg-one" :label="t('common.next')" @click="nextStep" />
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
