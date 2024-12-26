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
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { PopoverContent, PopoverRoot, PopoverTrigger } from 'radix-vue'
import { mdiInformationSlabCircleOutline } from '@mdi/js'

interface Props {}

defineProps<Props>()

const deviceStore = useDeviceStore()
</script>

<template>
    <div v-tooltip.top="{ value: 'Overview' }">
        <popover-root>
            <popover-trigger
                class="rounded-lg w-8 h-8 border-none p-0 text-text-color-secondary outline-0 text-center justify-center items-center flex hover:text-text-color hover:bg-surface-hover"
            >
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiInformationSlabCircleOutline"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </popover-trigger>
            <popover-content side="right" class="z-10">
                <div
                    class="w-full max-w-prose bg-bg-two border border-border-one p-4 rounded-lg text-text-color"
                >
                    <span class="font-bold text-lg">Functions Overview</span>
                    <div class="mt-1 h-2 border-border-one border-t-2" />
                    Functions are configurable algorithms that are applied to a Profile's output.
                    They enable you to manage when fan speed changes occur, adjust hysteresis
                    settings, and utilize moving averages for dynamic temperatures. The Identity
                    Function is the simplest option, as it does not modify the calculated Profile's
                    output; it merely allows you to set the minimum and maximum speed duty changes.
                    This is especially beneficial for users who want to minimize constant
                    fluctuations in fan speeds.
                </div>
            </popover-content>
        </popover-root>
    </div>
</template>

<style scoped lang="scss"></style>
