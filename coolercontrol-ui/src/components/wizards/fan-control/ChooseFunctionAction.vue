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
import Button from 'primevue/button'
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { mdiArrowLeft } from '@mdi/js'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { UID } from '@/models/Device.ts'

interface Props {
    name: string
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'functionUID', funUID: UID): void
    (e: 'nextStep', step: number): void
}>()
const { t } = useI18n()
const settingsStore = useSettingsStore()
const deviceStore = useDeviceStore()

const functionsLength: number = settingsStore.functions.length

const defaultFunctionAction = () => {
    emit('functionUID', '0')
    emit('nextStep', 13)
}
</script>

<template>
    <div class="flex flex-col gap-y-4 w-96">
        <p>
            {{ t('components.wizards.fanControl.functionFor') }}:
            <span class="font-bold">{{ props.name }}</span>
        </p>
        <p>
            <span v-html="t('components.wizards.fanControl.functionDescription')" />
        </p>
        <Button
            class="!p-2 h-[2.375rem]"
            :label="t('components.wizards.fanControl.createNewFunction')"
            @click="emit('nextStep', 11)"
        />
        <Button
            v-if="functionsLength > 1"
            class="!p-2 h-[2.375rem]"
            :label="t('components.wizards.fanControl.existingFunction')"
            @click="emit('nextStep', 12)"
        />
        <Button
            class="!p-2 h-[2.375rem]"
            :label="t('components.wizards.fanControl.defaultFunction')"
            @click="defaultFunctionAction"
        />
        <div class="flex flex-row justify-between mt-4">
            <Button class="w-24 h-[2.375rem]" label="Back" @click="emit('nextStep', 9)">
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiArrowLeft"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
