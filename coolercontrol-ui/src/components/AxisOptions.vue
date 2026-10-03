<!--
  SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { Dashboard } from '@/models/Dashboard.ts'
import UiNumberInput from '@/shell/ui/UiNumberInput.vue'
import { mdiAxisArrow, mdiAxisXArrow, mdiAxisYArrow } from '@mdi/js'
import { PopoverContent, PopoverPortal, PopoverRoot, PopoverTrigger } from 'reka-ui'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import UiSwitch from '@/shell/ui/UiSwitch.vue'
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
// A fan input can carry another unit: the Leakshield's pressure reaches 6 553 500.
const freqMaxLimit = 10_000_000 / precision
const freqScaledMin: Ref<number> = ref(props.dashboard.frequencyMin / precision)
const freqScaledMax: Ref<number> = ref(props.dashboard.frequencyMax / precision)

watch(freqScaledMin, () => {
    props.dashboard.frequencyMin = freqScaledMin.value * precision
})
watch(freqScaledMax, () => {
    props.dashboard.frequencyMax = freqScaledMax.value * precision
})
const isPopupOpen = ref(false)
</script>

<template>
    <div v-tooltip.bottom="{ value: t('components.axisOptions.title'), disabled: isPopupOpen }">
        <popover-root @update:open="(open) => (isPopupOpen = open)">
            <popover-trigger
                class="h-10 rounded-lg border border-border-one bg-control !py-1.5 !px-4 text-text-color outline-0 text-center justify-center items-center flex !m-0 hover:bg-surface-hover"
            >
                <svg-icon
                    class="outline-0 mt-[-2px]"
                    type="mdi"
                    :path="mdiAxisArrow"
                    :size="deviceStore.getREMSize(1.25)"
                />
            </popover-trigger>
            <popover-portal>
                <popover-content side="bottom" class="z-[1300]">
                    <div
                        class="w-full bg-bg-two border border-border-one p-1 rounded-lg text-text-color shadow-overlay-lg"
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
                                        <UiSwitch v-model="dashboard.autoScaleDegree" />
                                    </td>
                                    <td class="w-24 text-end px-2 border-r border-border-one">
                                        {{ t('components.axisOptions.autoScale') }}
                                    </td>
                                    <td class="w-24 px-2 text-center">
                                        <UiSwitch v-model="dashboard.autoScaleFrequency" />
                                    </td>
                                    <td class="w-24 text-end px-2 border-x border-border-one">
                                        {{ t('components.axisOptions.autoScale') }}
                                    </td>
                                    <td class="w-24 px-2 text-center">
                                        <UiSwitch v-model="dashboard.autoScaleWatts" />
                                    </td>
                                </tr>
                                <tr>
                                    <td class="w-24 text-end px-2 border-r border-border-one">
                                        {{ t('components.axisOptions.max') }}
                                    </td>
                                    <td class="w-24 px-2 border-r border-border-one">
                                        <UiNumberInput
                                            v-model="dashboard.degreeMax"
                                            :min="dashboard.degreeMin + 10"
                                            :max="200"
                                            :step="10"
                                            :disabled="dashboard.autoScaleDegree"
                                        />
                                    </td>
                                    <td class="w-24 text-end px-2 border-r border-border-one">
                                        {{ t('components.axisOptions.max') }}
                                    </td>
                                    <td class="w-24 px-2 text-center">
                                        <UiNumberInput
                                            v-model="freqScaledMax"
                                            :min="freqScaledMin + freqStepSize"
                                            :max="freqMaxLimit"
                                            :step="freqStepSize"
                                            :disabled="dashboard.autoScaleFrequency"
                                        />
                                    </td>
                                    <td class="w-24 text-end px-2 border-x border-border-one">
                                        {{ t('components.axisOptions.max') }}
                                    </td>
                                    <td class="w-24 px-2 text-center">
                                        <UiNumberInput
                                            v-model="dashboard.wattsMax"
                                            :min="dashboard.wattsMin + 1"
                                            :max="800"
                                            :step="dashboard.wattsMax >= 10 ? 10 : 1"
                                            :disabled="dashboard.autoScaleWatts"
                                        />
                                    </td>
                                </tr>
                                <tr>
                                    <td class="w-24 text-end px-2 border-r border-border-one">
                                        {{ t('components.axisOptions.min') }}
                                    </td>
                                    <td class="w-24 px-2 border-r border-border-one">
                                        <UiNumberInput
                                            v-model="dashboard.degreeMin"
                                            :min="0"
                                            :max="dashboard.degreeMax - 10"
                                            :step="10"
                                            :disabled="dashboard.autoScaleDegree"
                                        />
                                    </td>
                                    <td class="w-24 text-end px-2 border-r border-border-one">
                                        {{ t('components.axisOptions.min') }}
                                    </td>
                                    <td class="w-24 px-2 text-center">
                                        <UiNumberInput
                                            v-model="freqScaledMin"
                                            :min="0"
                                            :max="freqScaledMax - freqStepSize"
                                            :step="freqStepSize"
                                            :disabled="dashboard.autoScaleFrequency"
                                        />
                                    </td>
                                    <td class="w-24 text-end px-2 border-x border-border-one">
                                        {{ t('components.axisOptions.min') }}
                                    </td>
                                    <td class="w-24 px-2 text-center">
                                        <UiNumberInput
                                            v-model="dashboard.wattsMin"
                                            :min="0"
                                            :max="dashboard.wattsMax - 1"
                                            :step="dashboard.wattsMax >= 10 ? 10 : 1"
                                            :disabled="dashboard.autoScaleWatts"
                                        />
                                    </td>
                                </tr>
                            </tbody>
                        </table>
                    </div>
                </popover-content>
            </popover-portal>
        </popover-root>
    </div>
</template>

<style scoped lang="scss"></style>
