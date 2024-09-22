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
import { mdiAxisArrow, mdiAxisXArrow, mdiAxisYArrow } from '@mdi/js'
import { PopoverContent, PopoverRoot, PopoverTrigger } from 'radix-vue'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { ElSwitch } from 'element-plus'
import 'element-plus/es/components/switch/style/css'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { Dashboard } from '@/models/Dashboard.ts'
import InputNumber from 'primevue/inputnumber'

interface Props {
    dashboard: Dashboard
}

defineProps<Props>()
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
                    class="w-full bg-bg-two border-2 border-border-one p-1 rounded-lg text-text-color"
                >
                    <table>
                        <tr>
                            <th colspan="4" class="pb-2">Axis Options</th>
                        </tr>
                        <tr>
                            <th colspan="2" class="w-48 p-2 border-b border-r border-border-one">
                                <span class="flex flex-row justify-center">
                                    <svg-icon
                                        class="outline-0 mr-2"
                                        type="mdi"
                                        :path="mdiAxisXArrow"
                                        :size="deviceStore.getREMSize(1.25)"
                                    />
                                    Duty / Temperature
                                </span>
                            </th>
                            <th colspan="2" class="w-48 p-2 border-b border-border-one">
                                <span class="flex flex-row justify-center">
                                    RPM / Mhz
                                    <svg-icon
                                        class="outline-0 ml-2"
                                        type="mdi"
                                        :path="mdiAxisYArrow"
                                        :size="deviceStore.getREMSize(1.25)"
                                    />
                                </span>
                            </th>
                        </tr>
                        <tr>
                            <td class="w-24 text-end px-2 border-r border-border-one">AutoScale</td>
                            <td class="w-24 px-2 border-r border-border-one text-center">
                                <el-switch v-model="dashboard.autoScaleDegree" size="large" />
                            </td>
                            <td class="w-24 text-end px-2 border-r border-border-one">AutoScale</td>
                            <td class="w-24 px-2 text-center">
                                <el-switch v-model="dashboard.autoScaleFrequency" size="large" />
                            </td>
                        </tr>
                        <tr>
                            <td class="w-24 text-end px-2 border-r border-border-one">Max</td>
                            <td class="w-24 px-2 border-r border-border-one">
                                <InputNumber
                                    placeholder="Max"
                                    v-model="dashboard.degreeMax"
                                    class="my-1"
                                    show-buttons
                                    :use-grouping="false"
                                    :step="10"
                                    :min="dashboard.degreeMin + 10"
                                    :max="200"
                                    button-layout="horizontal"
                                    :allow-empty="false"
                                    :input-style="{ width: '3rem' }"
                                    :disabled="dashboard.autoScaleDegree"
                                >
                                    <template #incrementbuttonicon>
                                        <span class="pi pi-plus" />
                                    </template>
                                    <template #decrementbuttonicon>
                                        <span class="pi pi-minus" />
                                    </template>
                                </InputNumber>
                            </td>
                            <td class="w-24 text-end px-2 border-r border-border-one">Max</td>
                            <td class="w-24 px-2 text-center">
                                <InputNumber
                                    placeholder="Max"
                                    v-model="dashboard.frequencyMax"
                                    class="my-1"
                                    show-buttons
                                    :use-grouping="true"
                                    :step="100"
                                    :min="dashboard.frequencyMin + 100"
                                    :max="100_000"
                                    button-layout="horizontal"
                                    :allow-empty="false"
                                    :input-style="{ width: '5rem' }"
                                    :disabled="dashboard.autoScaleFrequency"
                                >
                                    <template #incrementbuttonicon>
                                        <span class="pi pi-plus" />
                                    </template>
                                    <template #decrementbuttonicon>
                                        <span class="pi pi-minus" />
                                    </template>
                                </InputNumber>
                            </td>
                        </tr>
                        <tr>
                            <td class="w-24 text-end px-2 border-r border-border-one">Min</td>
                            <td class="w-24 px-2 border-r border-border-one">
                                <InputNumber
                                    placeholder="Min"
                                    v-model="dashboard.degreeMin"
                                    class="my-1"
                                    show-buttons
                                    :use-grouping="false"
                                    :step="10"
                                    :min="0"
                                    :max="dashboard.degreeMax - 10"
                                    button-layout="horizontal"
                                    :allow-empty="false"
                                    :input-style="{ width: '3rem' }"
                                    :disabled="dashboard.autoScaleDegree"
                                >
                                    <template #incrementbuttonicon>
                                        <span class="pi pi-plus" />
                                    </template>
                                    <template #decrementbuttonicon>
                                        <span class="pi pi-minus" />
                                    </template>
                                </InputNumber>
                            </td>
                            <td class="w-24 text-end px-2 border-r border-border-one">Min</td>
                            <td class="w-24 px-2 text-center">
                                <InputNumber
                                    placeholder="Min"
                                    v-model="dashboard.frequencyMin"
                                    class="my-1"
                                    show-buttons
                                    :use-grouping="false"
                                    :step="100"
                                    :min="0"
                                    :max="dashboard.frequencyMax - 100"
                                    button-layout="horizontal"
                                    :allow-empty="false"
                                    :input-style="{ width: '5rem' }"
                                    :disabled="dashboard.autoScaleFrequency"
                                >
                                    <template #incrementbuttonicon>
                                        <span class="pi pi-plus" />
                                    </template>
                                    <template #decrementbuttonicon>
                                        <span class="pi pi-minus" />
                                    </template>
                                </InputNumber>
                            </td>
                        </tr>
                    </table>
                </div>
            </popover-content>
        </popover-root>
    </div>
</template>

<style scoped lang="scss">
.el-switch {
    --el-switch-on-color: rgb(var(--colors-accent));
    --el-switch-off-color: rgb(var(--colors-bg-one));
    --el-color-white: rgb(var(--colors-bg-two));
}
</style>
