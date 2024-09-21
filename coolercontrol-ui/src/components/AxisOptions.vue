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
import { mdiAxisArrow, mdiRenameOutline } from '@mdi/js'
import { PopoverClose, PopoverContent, PopoverRoot, PopoverTrigger } from 'radix-vue'
import Button from 'primevue/button'
import InputText from 'primevue/inputtext'
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import FloatLabel from 'primevue/floatlabel'
import { useDeviceStore } from '@/stores/DeviceStore.ts'

const deviceStore = useDeviceStore()
</script>

<template>
    <div v-tooltip.bottom="'Axis Options'">
        <popover-root>
            <popover-trigger
                class="rounded-lg border-2 border-border-one !py-1.5 !px-4 text-text-color outline-0 text-center justify-center items-center flex !m-0 hover:bg-surface-hover"
            >
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiAxisArrow"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </popover-trigger>
            <popover-content side="bottom" class="z-10">
                <div
                    class="w-80 bg-bg-two border-2 border-border-one p-4 rounded-lg text-text-color"
                >
<!--                    todo:-->
                    TESTING
                </div>
            </popover-content>
        </popover-root>
    </div>
</template>

<style scoped lang="scss"></style>
