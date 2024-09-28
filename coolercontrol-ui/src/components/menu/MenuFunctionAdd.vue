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
import { mdiFlaskPlusOutline } from '@mdi/js'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { inject } from 'vue'
import { Emitter, EventType } from 'mitt'
import { UID } from '@/models/Device.ts'
import { Function } from '@/models/Profile.ts'
import { useToast } from 'primevue/usetoast'

interface Props {}

defineProps<Props>()
const emit = defineEmits<{
    (e: 'added', functionUID: UID): void
}>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const toast = useToast()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!

const addFunction = async (): Promise<void> => {
    const newFunction = new Function('New Function')
    settingsStore.functions.push(newFunction)
    await settingsStore.saveFunction(newFunction.uid)
    toast.add({
        severity: 'success',
        summary: 'Success',
        detail: 'Function Created',
        life: 3000,
    })
    emit('added', newFunction.uid)
}
// be able to add a function from the side menu add button:
emitter.on('function-add', addFunction)
</script>

<template>
    <div v-tooltip.top="{ value: 'Add Function' }">
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color"
            @click="addFunction"
        >
            <svg-icon type="mdi" :path="mdiFlaskPlusOutline" :size="deviceStore.getREMSize(1.5)" />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
