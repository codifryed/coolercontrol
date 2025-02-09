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
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiBookmarkPlusOutline } from '@mdi/js'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { inject } from 'vue'
import { Emitter, EventType } from 'mitt'
import { UID } from '@/models/Device.ts'
import { useRouter } from 'vue-router'

interface Props {}

defineProps<Props>()
const emit = defineEmits<{
    (e: 'added', modeUID: UID): void
}>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!
const router = useRouter()

const addMode = async (): Promise<void> => {
    const newModeUID = await settingsStore.createMode('New Mode')
    if (newModeUID == null) return
    emit('added', newModeUID)
    await router.push({ name: 'modes', params: { modeUID: newModeUID } })
}
// be able to add a mode from the side menu add button:
emitter.on('mode-add', addMode)
</script>

<template>
    <div v-tooltip.top="{ value: 'Create Mode from Current Settings' }">
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color"
            @click="addMode"
        >
            <svg-icon
                type="mdi"
                :path="mdiBookmarkPlusOutline"
                :size="deviceStore.getREMSize(1.5)"
            />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
