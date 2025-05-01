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
import { ElSwitch } from 'element-plus'
import 'element-plus/es/components/switch/style/css'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { Dashboard } from '@/models/Dashboard.ts'
import InputNumber from 'primevue/inputnumber'
import { mdiAxisArrow, mdiAxisXArrow, mdiAxisYArrow } from '@mdi/js'
import { PopoverContent, PopoverRoot, PopoverTrigger } from 'radix-vue'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { ref, Ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

interface Props {
    dashboard: Dashboard
}

const props = defineProps<Props>()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { t } = useI18n()

const precision = settingsStore.frequencyPrecision
const freqIsMhz = precision === 1
const freqStepSize = 100.0 / precision
const freqMaxLimit = 100_000 / precision
const freqScaledMin: Ref<number> = ref(props.dashboard.frequencyMin / precision)
const freqScaledMax: Ref<number> = ref(props.dashboard.frequencyMax / precision)

watch(freqScaledMin, () => {
    props.dashboard.frequencyMin = freqScaledMin.value * precision
})
watch(freqScaledMax, () => {
    props.dashboard.frequencyMax = freqScaledMax.value * precision
})
</script>

<template>
    <div v-tooltip.bottom="t('components.axisOptions.title')">
        <popover-root>
            <popover-trigger
                class="h-[2.375rem] rounded-lg border-2 border-border-one !py-1.5 !px-4 text-text-color outline-0 text-center justify-center items-center flex !m-0 hover:bg-surface-hover"
            >
                <svg-icon
                    class="outline-0 mt-[-2px]"
                    type="mdi"
                    :path="mdiAxisArrow"
                    :size="deviceStore.getREMSize(1.25)"
                />
            </popover-trigger>
            <popover-content side="bottom" class="z-10">
                <div
                    class="w-full bg-bg-two border border-border-one p-1 rounded-lg text-text-color"
                >
                    <table>
                        <thead>
                            <tr>
                                <th colspan="6" class="pb-2">
                                    {{ t('components.axisOptions.title') }}
                                </th>
                            </tr>
                            <tr>
                                <th
                                    colspan="2"
                                    class="w-48 p-2 border-b border-r border-border-one"
                                >
                                    <span class="flex flex-row justify-center">
                                        <svg-icon
                                            class="outline-0 mr-2"
                                            type="mdi"
                                            :path="mdiAxisXArrow"
                                            :size="deviceStore.getREMSize(1.25)"
                                        />
                                        {{ t('components.axisOptions.dutyTemperature') }}
                                    </span>
                                </th>
                                <th colspan="2" class="w-48 p-2 border-b border-border-one">
                                    <span class="flex flex-row justify-center">
                                        {{
                                            freqIsMhz
                                                ? t('components.axisOptions.rpmMhz')
                                                : t('components.axisOptions.krpmGhz')
                                        }}
                                        <svg-icon
                                            class="outline-0 ml-2"
                                            type="mdi"
                                            :path="mdiAxisYArrow"
                                            :size="deviceStore.getREMSize(1.25)"
                                        />
                                    </span>
                                </th>
                                <th
                                    colspan="2"
                                    class="w-48 p-2 border-l border-b border-border-one"
                                >
                                    <span class="flex flex-row justify-center">
                                        {{ t('components.axisOptions.watts') }}
                                        <svg-icon
                                            class="outline-0 ml-2"
                                            type="mdi"
                                            :path="mdiAxisYArrow"
                                            :size="deviceStore.getREMSize(1.25)"
                                        />
                                    </span>
                                </th>
                            </tr>
                        </thead>
                        <tbody>
                            <tr>
                                <td class="w-24 text-end px-2 border-r border-border-one">
                                    {{ t('components.axisOptions.autoScale') }}
                                </td>
                                <td class="w-24 px-2 border-r border-border-one text-center">
                                    <el-switch v-model="dashboard.autoScaleDegree" size="large" />
                                </td>
                                <td class="w-24 text-end px-2 border-r border-border-one">
                                    {{ t('components.axisOptions.autoScale') }}
                                </td>
                                <td class="w-24 px-2 text-center">
                                    <el-switch
                                        v-model="dashboard.autoScaleFrequency"
                                        size="large"
                                    />
                                </td>
                                <td class="w-24 text-end px-2 border-x border-border-one">
                                    {{ t('components.axisOptions.autoScale') }}
                                </td>
                                <td class="w-24 px-2 text-center">
                                    <el-switch v-model="dashboard.autoScaleWatts" size="large" />
                                </td>
                            </tr>
                            <tr>
                                <td class="w-24 text-end px-2 border-r border-border-one">
                                    {{ t('components.axisOptions.max') }}
                                </td>
                                <td class="w-24 px-2 border-r border-border-one">
                                    <InputNumber
                                        :placeholder="t('components.axisOptions.max')"
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
                                        <template #incrementicon>
                                            <span class="pi pi-plus" />
                                        </template>
                                        <template #decrementicon>
                                            <span class="pi pi-minus" />
                                        </template>
                                    </InputNumber>
                                </td>
                                <td class="w-24 text-end px-2 border-r border-border-one">
                                    {{ t('components.axisOptions.max') }}
                                </td>
                                <td class="w-24 px-2 text-center">
                                    <InputNumber
                                        :placeholder="t('components.axisOptions.max')"
                                        v-model="freqScaledMax"
                                        class="my-1"
                                        show-buttons
                                        :use-grouping="true"
                                        :step="freqStepSize"
                                        :min="freqScaledMin + freqStepSize"
                                        :max="freqMaxLimit"
                                        :min-fraction-digits="freqIsMhz ? 0 : 1"
                                        button-layout="horizontal"
                                        :allow-empty="false"
                                        :input-style="{ width: '5rem' }"
                                        :disabled="dashboard.autoScaleFrequency"
                                    >
                                        <template #incrementicon>
                                            <span class="pi pi-plus" />
                                        </template>
                                        <template #decrementicon>
                                            <span class="pi pi-minus" />
                                        </template>
                                    </InputNumber>
                                </td>
                                <td class="w-24 text-end px-2 border-x border-border-one">
                                    {{ t('components.axisOptions.max') }}
                                </td>
                                <td class="w-24 px-2 text-center">
                                    <InputNumber
                                        :placeholder="t('components.axisOptions.max')"
                                        v-model="dashboard.wattsMax"
                                        class="my-1"
                                        show-buttons
                                        :use-grouping="true"
                                        :step="dashboard.wattsMax >= 10 ? 10 : 1"
                                        :min="dashboard.wattsMin + 1"
                                        :max="800"
                                        :min-fraction-digits="0"
                                        button-layout="horizontal"
                                        :allow-empty="false"
                                        :input-style="{ width: '3rem' }"
                                        :disabled="dashboard.autoScaleWatts"
                                    >
                                        <template #incrementicon>
                                            <span class="pi pi-plus" />
                                        </template>
                                        <template #decrementicon>
                                            <span class="pi pi-minus" />
                                        </template>
                                    </InputNumber>
                                </td>
                            </tr>
                            <tr>
                                <td class="w-24 text-end px-2 border-r border-border-one">
                                    {{ t('components.axisOptions.min') }}
                                </td>
                                <td class="w-24 px-2 border-r border-border-one">
                                    <InputNumber
                                        :placeholder="t('components.axisOptions.min')"
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
                                        <template #incrementicon>
                                            <span class="pi pi-plus" />
                                        </template>
                                        <template #decrementicon>
                                            <span class="pi pi-minus" />
                                        </template>
                                    </InputNumber>
                                </td>
                                <td class="w-24 text-end px-2 border-r border-border-one">
                                    {{ t('components.axisOptions.min') }}
                                </td>
                                <td class="w-24 px-2 text-center">
                                    <InputNumber
                                        :placeholder="t('components.axisOptions.min')"
                                        v-model="freqScaledMin"
                                        class="my-1"
                                        show-buttons
                                        :use-grouping="true"
                                        :step="freqStepSize"
                                        :min="0"
                                        :max="freqScaledMax - freqStepSize"
                                        :min-fraction-digits="freqIsMhz ? 0 : 1"
                                        button-layout="horizontal"
                                        :allow-empty="false"
                                        :input-style="{ width: '5rem' }"
                                        :disabled="dashboard.autoScaleFrequency"
                                    >
                                        <template #incrementicon>
                                            <span class="pi pi-plus" />
                                        </template>
                                        <template #decrementicon>
                                            <span class="pi pi-minus" />
                                        </template>
                                    </InputNumber>
                                </td>
                                <td class="w-24 text-end px-2 border-x border-border-one">
                                    {{ t('components.axisOptions.min') }}
                                </td>
                                <td class="w-24 px-2 text-center">
                                    <InputNumber
                                        :placeholder="t('components.axisOptions.min')"
                                        v-model="dashboard.wattsMin"
                                        class="my-1"
                                        show-buttons
                                        :use-grouping="true"
                                        :step="dashboard.wattsMax >= 10 ? 10 : 1"
                                        :min="0"
                                        :max="dashboard.wattsMax - 1"
                                        :min-fraction-digits="0"
                                        button-layout="horizontal"
                                        :allow-empty="false"
                                        :input-style="{ width: '3rem' }"
                                        :disabled="dashboard.autoScaleWatts"
                                    >
                                        <template #incrementicon>
                                            <span class="pi pi-plus" />
                                        </template>
                                        <template #decrementicon>
                                            <span class="pi pi-minus" />
                                        </template>
                                    </InputNumber>
                                </td>
                            </tr>
                        </tbody>
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
