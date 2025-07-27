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
import { mdiPinOffOutline, mdiPinOutline } from '@mdi/js'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useI18n } from 'vue-i18n'

const deviceStore = useDeviceStore()
const { t } = useI18n()

const props = defineProps<{
    isPinned: boolean
}>()
const emit = defineEmits<{
    (e: 'pin'): void
    (e: 'unpin'): void
    (e: 'close'): void
}>()

const pinItem = () => {
    if (props.isPinned) {
        emit('unpin')
    } else {
        emit('pin')
    }
    emit('close')
}
</script>

<template>
    <Button
        class="w-full !justify-start !rounded-lg border-none text-text-color-secondary h-12 !p-4 !px-7 hover:text-text-color hover:bg-surface-hover outline-none"
        @click="pinItem"
    >
        <svg-icon
            class="outline-0 !cursor-pointer"
            type="mdi"
            :path="props.isPinned ? mdiPinOffOutline : mdiPinOutline"
            :size="deviceStore.getREMSize(1.5)"
        />
        <span class="ml-1.5">
            {{ props.isPinned ? t('layout.menu.tooltips.unpin') : t('layout.menu.tooltips.pin') }}
        </span>
    </Button>
</template>

<style scoped lang="scss"></style>
