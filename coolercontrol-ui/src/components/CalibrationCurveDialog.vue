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
import * as echarts from 'echarts/core'
import { GridComponent, LegendComponent, TooltipComponent } from 'echarts/components'
import { LineChart } from 'echarts/charts'
import { CanvasRenderer } from 'echarts/renderers'
import VChart from 'vue-echarts'
import { inject, onMounted, ref, watch, type Ref } from 'vue'
import type { DynamicDialogInstance } from 'primevue/dynamicdialogoptions'
import Button from 'primevue/button'
import Select from 'primevue/select'
import InputNumber from 'primevue/inputnumber'
import ToggleSwitch from 'primevue/toggleswitch'
import { useToast } from 'primevue/usetoast'
import { useI18n } from 'vue-i18n'
import _ from 'lodash'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore'
import { useCalibrationStore } from '@/stores/CalibrationStore'
import { ErrorResponse } from '@/models/ErrorResponse'
import type { Calibration } from '@/models/Calibration'
import type { UID } from '@/models/Device'

echarts.use([CanvasRenderer, LineChart, GridComponent, LegendComponent, TooltipComponent])

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const { t } = useI18n()
const deviceStore = useDeviceStore()
const colors = useThemeColorsStore()
const calibrationStore = useCalibrationStore()
const toast = useToast()

const deviceUID: UID = dialogRef.value.data.deviceUID
const channelName: string = dialogRef.value.data.channelName
const initialCalibration: Calibration | undefined = dialogRef.value.data.calibration

const calibration: Ref<Calibration | undefined> = ref(initialCalibration)
const loading: Ref<boolean> = ref(initialCalibration == null)
const loadError: Ref<string | undefined> = ref()

type BoostMode = 'auto' | 'on' | 'off'
const boostMode: Ref<BoostMode> = ref('auto')
const durationOverride: Ref<number | null> = ref(null)
// Local state for the walk-down toggle. The daemon defaults the
// override to None (walk enabled), so the toggle starts checked.
const walkAfterKick: Ref<boolean> = ref(true)
const saving: Ref<boolean> = ref(false)
// Suppress watcher-driven saves while we sync the refs from a fresh
// calibration (initial load, post-save echo). Without this the watcher
// would fire on every assignment and re-PATCH the same values.
let syncing = false

const boostOptions: Array<{ label: string; value: BoostMode }> = [
    { label: t('components.calibrationCurve.kickBoostAuto'), value: 'auto' },
    { label: t('components.calibrationCurve.kickBoostOn'), value: 'on' },
    { label: t('components.calibrationCurve.kickBoostOff'), value: 'off' },
]

const syncOverridesFrom = (cal: Calibration): void => {
    syncing = true
    boostMode.value =
        cal.kick_boost_override === true ? 'on' : cal.kick_boost_override === false ? 'off' : 'auto'
    durationOverride.value = cal.kick_duration_override_ms ?? null
    // `null` (no override) and `true` (explicit on) both mean the walk
    // runs. Only `false` toggles it off.
    walkAfterKick.value = cal.walk_after_kick_override !== false
    // Release the suppression on the next microtask so the watchers
    // observe the assignment as "settled".
    void Promise.resolve().then(() => {
        syncing = false
    })
}

onMounted(async () => {
    if (calibration.value != null) {
        syncOverridesFrom(calibration.value)
        return
    }
    try {
        const stored = await calibrationStore.getStored(deviceUID, channelName)
        if (stored == null) {
            loadError.value = t('components.calibrationCurve.notFound')
        } else {
            calibration.value = stored
            syncOverridesFrom(stored)
        }
    } catch {
        loadError.value = t('components.calibrationCurve.loadError')
    } finally {
        loading.value = false
    }
})

const overridesFromMode = (mode: BoostMode): boolean | null => {
    return mode === 'on' ? true : mode === 'off' ? false : null
}

const saveOverrides = async (): Promise<void> => {
    if (calibration.value == null) return
    saving.value = true
    const result = await calibrationStore.updateOverrides(deviceUID, channelName, {
        kick_boost_override: overridesFromMode(boostMode.value),
        kick_duration_override_ms: durationOverride.value,
        // Persist `false` only when the user opts out, leaving the
        // daemon-side `None` default for the on case to keep stored
        // payloads minimal.
        walk_after_kick_override: walkAfterKick.value ? null : false,
    })
    saving.value = false
    if (result instanceof ErrorResponse || result == null) {
        toast.add({
            severity: 'error',
            summary: t('components.calibrationCurve.overridesSaveFailed'),
            detail: result instanceof ErrorResponse ? result.error : undefined,
            life: 5000,
        })
        // Revert local refs to the still-current calibration values so
        // the form reflects what's actually persisted.
        if (calibration.value != null) syncOverridesFrom(calibration.value)
        return
    }
    calibration.value = result
}

const saveBoost = () => {
    if (syncing) return
    void saveOverrides()
}
const saveDuration = _.debounce(() => {
    if (syncing) return
    void saveOverrides()
}, 500)
const saveWalk = () => {
    if (syncing) return
    void saveOverrides()
}

watch(boostMode, saveBoost)
watch(durationOverride, saveDuration)
watch(walkAfterKick, saveWalk)

const formatTimestamp = (iso: string): string => {
    const date = new Date(iso)
    if (Number.isNaN(date.getTime())) return iso
    return date.toLocaleString()
}

const curveKindLabel = (kind: 'Smooth' | 'Stepped'): string => {
    return kind === 'Smooth'
        ? t('components.calibrationCurve.curveKindSmooth')
        : t('components.calibrationCurve.curveKindStepped')
}

const chartOption = (cal: Calibration) => {
    const upData = cal.up_curve.map((s) => ({ value: [s.duty, s.rpm] }))
    const downData = cal.down_curve.map((s) => ({ value: [s.duty, s.rpm] }))
    const allRpms = [
        cal.rpm_max,
        ...cal.up_curve.map((s) => s.rpm),
        ...cal.down_curve.map((s) => s.rpm),
        1,
    ]
    const maxRpm = Math.max(...allRpms)
    const upLabel = t('components.calibrationCurve.legendUp')
    const downLabel = t('components.calibrationCurve.legendDown')
    const upColor = colors.themeColors.accent
    const downColor = colors.themeColors.text_color_secondary
    return {
        backgroundColor: colors.themeColors.bg_one,
        grid: {
            show: false,
            top: deviceStore.getREMSize(2.5),
            left: deviceStore.getREMSize(2.0),
            right: deviceStore.getREMSize(1.5),
            bottom: deviceStore.getREMSize(2.5),
            outerBoundsMode: 'same',
        },
        tooltip: {
            trigger: 'axis',
            backgroundColor: colors.themeColors.bg_two,
            borderColor: colors.themeColors.border,
            textStyle: { color: colors.themeColors.text_color },
            valueFormatter: (value: any): string => {
                if (Array.isArray(value)) return `${Math.round(Number(value[1]))} RPM`
                return `${Math.round(Number(value))} RPM`
            },
        },
        legend: {
            data: [upLabel, downLabel],
            selected: { [upLabel]: true, [downLabel]: true },
            textStyle: { color: colors.themeColors.text_color },
            top: 0,
            icon: 'roundRect',
        },
        xAxis: {
            min: 0,
            max: 100,
            type: 'value',
            splitNumber: 10,
            name: t('components.calibrationCurve.axisDuty'),
            nameLocation: 'middle',
            nameGap: deviceStore.getREMSize(2.0),
            nameTextStyle: {
                color: colors.themeColors.text_color_secondary,
                fontSize: deviceStore.getREMSize(0.9),
            },
            axisLabel: {
                fontSize: deviceStore.getREMSize(0.85),
                color: colors.themeColors.text_color_secondary,
                formatter: (value: any): string => `${value}${t('common.percentUnit')}`,
            },
            axisLine: { lineStyle: { color: colors.themeColors.text_color, width: 1 } },
            splitLine: {
                lineStyle: { color: colors.themeColors.border, width: 0.5, type: 'dotted' },
            },
        },
        yAxis: {
            min: 0,
            max: Math.ceil(maxRpm / 100) * 100,
            type: 'value',
            name: t('components.calibrationCurve.axisRpm'),
            nameLocation: 'middle',
            nameGap: deviceStore.getREMSize(3.2),
            nameTextStyle: {
                color: colors.themeColors.text_color_secondary,
                fontSize: deviceStore.getREMSize(0.9),
            },
            axisLabel: {
                fontSize: deviceStore.getREMSize(0.85),
                color: colors.themeColors.text_color_secondary,
            },
            axisLine: { lineStyle: { color: colors.themeColors.text_color, width: 1 } },
            splitLine: {
                lineStyle: { color: colors.themeColors.border, width: 0.5, type: 'dotted' },
            },
        },
        // @ts-ignore
        series: [
            {
                id: 'upCurve',
                name: upLabel,
                type: 'line',
                smooth: 0.0,
                showSymbol: true,
                symbol: 'circle',
                symbolSize: 8,
                lineStyle: {
                    color: upColor,
                    width: 3,
                    type: 'solid',
                },
                itemStyle: {
                    color: upColor,
                    borderColor: upColor,
                    borderWidth: 2,
                },
                emphasis: { disabled: true },
                connectNulls: true,
                data: upData,
                z: 10,
                // Dashed vertical marker at min_stable_duty when it
                // sits above min_sustain_duty: a firmware-kick fan has
                // an oscillation band above min_sustain_duty and the
                // dispatcher clamps post-kick sustain to this floor.
                // Healthy fans collapse to the same value and the
                // line is suppressed.
                ...(cal.min_stable_duty > cal.min_sustain_duty
                    ? {
                          markLine: {
                              silent: true,
                              symbol: 'none',
                              label: {
                                  color: colors.themeColors.yellow,
                                  fontSize: deviceStore.getREMSize(0.8),
                                  formatter: t('components.calibrationCurve.markerStable'),
                                  position: 'insideEndTop',
                              },
                              lineStyle: {
                                  color: colors.themeColors.yellow,
                                  width: 1.5,
                                  type: 'dashed',
                                  opacity: 0.8,
                              },
                              data: [{ xAxis: cal.min_stable_duty }],
                          },
                      }
                    : {}),
            },
            {
                id: 'downCurve',
                name: downLabel,
                type: 'line',
                smooth: 0.0,
                showSymbol: true,
                symbol: 'circle',
                symbolSize: 6,
                lineStyle: {
                    color: downColor,
                    width: 2,
                    type: 'dashed',
                },
                itemStyle: {
                    color: downColor,
                    borderColor: downColor,
                    borderWidth: 2,
                },
                emphasis: { disabled: true },
                connectNulls: true,
                data: downData,
                z: 5,
            },
        ],
        animationDuration: 200,
    }
}
</script>

<template>
    <div class="flex flex-col gap-3 text-text-color">
        <div v-if="loading" class="py-10 text-center text-text-color-secondary">
            {{ t('components.calibrationCurve.loading') }}
        </div>
        <div v-else-if="loadError" class="py-10 text-center text-red">
            {{ loadError }}
        </div>
        <template v-else-if="calibration != null">
            <v-chart
                class="calibration-curve-chart"
                :option="chartOption(calibration)"
                :autoresize="true"
            />
            <div
                class="grid grid-cols-2 md:grid-cols-3 gap-x-6 gap-y-1.5 text-sm border-t border-border-one pt-3"
            >
                <div
                    v-tooltip.top="t('components.calibrationCurve.fieldCurveKindTooltip')"
                    class="flex justify-between md:block cursor-help"
                >
                    <span class="text-text-color-secondary mr-2">
                        {{ t('components.calibrationCurve.fieldCurveKind') }}:
                    </span>
                    <span class="font-medium">{{ curveKindLabel(calibration.curve_kind) }}</span>
                </div>
                <div
                    v-tooltip.top="t('components.calibrationCurve.fieldRpmMaxTooltip')"
                    class="flex justify-between md:block cursor-help"
                >
                    <span class="text-text-color-secondary mr-2">
                        {{ t('components.calibrationCurve.fieldRpmMax') }}:
                    </span>
                    <span class="font-medium">{{ calibration.rpm_max }} RPM</span>
                </div>
                <div
                    v-tooltip.top="t('components.calibrationCurve.fieldKickTooltip')"
                    class="flex justify-between md:block cursor-help"
                >
                    <span class="text-text-color-secondary mr-2">
                        {{ t('components.calibrationCurve.fieldKick') }}:
                    </span>
                    <span class="font-medium">{{ calibration.kick_duration_ms }} ms</span>
                </div>
                <div
                    v-tooltip.top="t('components.calibrationCurve.fieldStartTooltip')"
                    class="flex justify-between md:block cursor-help"
                >
                    <span class="text-text-color-secondary mr-2">
                        {{ t('components.calibrationCurve.fieldStart') }}:
                    </span>
                    <span class="font-medium">
                        {{ calibration.min_start_duty }}{{ t('common.percentUnit') }}
                    </span>
                </div>
                <div
                    v-tooltip.top="t('components.calibrationCurve.fieldSustainTooltip')"
                    class="flex justify-between md:block cursor-help"
                >
                    <span class="text-text-color-secondary mr-2">
                        {{ t('components.calibrationCurve.fieldSustain') }}:
                    </span>
                    <span class="font-medium">
                        {{ calibration.min_sustain_duty }}{{ t('common.percentUnit') }}
                    </span>
                </div>
                <div
                    v-if="calibration.min_stable_duty > calibration.min_sustain_duty"
                    v-tooltip.top="t('components.calibrationCurve.fieldStableTooltip')"
                    class="flex justify-between md:block cursor-help"
                >
                    <span class="text-text-color-secondary mr-2">
                        {{ t('components.calibrationCurve.fieldStable') }}:
                    </span>
                    <span class="font-medium">
                        {{ calibration.min_stable_duty }}{{ t('common.percentUnit') }}
                    </span>
                </div>
                <div
                    v-tooltip.top="t('components.calibrationCurve.fieldSaturateTooltip')"
                    class="flex justify-between md:block cursor-help"
                >
                    <span class="text-text-color-secondary mr-2">
                        {{ t('components.calibrationCurve.fieldSaturate') }}:
                    </span>
                    <span class="font-medium">
                        {{ calibration.max_eff_duty }}{{ t('common.percentUnit') }}
                    </span>
                </div>
            </div>
            <div
                class="grid grid-cols-1 md:grid-cols-2 gap-x-6 gap-y-3 text-sm border-t border-border-one pt-3"
            >
                <div class="md:col-span-2 text-text-color-secondary font-medium">
                    {{ t('components.calibrationCurve.overridesHeading') }}
                </div>
                <div class="flex items-center gap-3">
                    <label
                        v-tooltip.top="
                            t('components.calibrationCurve.fieldKickBoostOverrideTooltip')
                        "
                        class="text-text-color-secondary cursor-help shrink-0"
                    >
                        {{ t('components.calibrationCurve.fieldKickBoostOverride') }}:
                    </label>
                    <Select
                        v-model="boostMode"
                        :options="boostOptions"
                        optionLabel="label"
                        optionValue="value"
                        :disabled="saving"
                        class="w-44"
                    />
                    <span
                        class="text-xs px-2 py-0.5 rounded"
                        :class="
                            calibration.kick_boost_active
                                ? 'text-accent bg-accent/15'
                                : 'text-text-color-secondary bg-bg-two'
                        "
                    >
                        {{
                            calibration.kick_boost_active
                                ? t('components.calibrationCurve.kickBoostCurrentlyOn')
                                : t('components.calibrationCurve.kickBoostCurrentlyOff')
                        }}
                    </span>
                </div>
                <div class="flex items-center gap-3">
                    <label
                        v-tooltip.top="
                            t('components.calibrationCurve.fieldKickDurationOverrideTooltip')
                        "
                        class="text-text-color-secondary cursor-help shrink-0"
                    >
                        {{ t('components.calibrationCurve.fieldKickDurationOverride') }}:
                    </label>
                    <InputNumber
                        v-model="durationOverride"
                        :placeholder="`${t('components.calibrationCurve.kickDurationDefault')}: ${calibration.kick_duration_ms} ms`"
                        :min="100"
                        :max="60000"
                        :step="100"
                        :use-grouping="false"
                        suffix=" ms"
                        :disabled="saving"
                        :input-class="'w-44'"
                    />
                    <Button
                        v-tooltip.top="t('components.calibrationCurve.kickDurationReset')"
                        icon="pi pi-undo"
                        severity="secondary"
                        text
                        rounded
                        :disabled="saving || durationOverride === null"
                        :aria-label="t('components.calibrationCurve.kickDurationReset')"
                        @click="durationOverride = null"
                    />
                </div>
                <div class="flex items-center gap-3">
                    <label
                        v-tooltip.top="t('components.calibrationCurve.fieldWalkAfterKickTooltip')"
                        class="text-text-color-secondary cursor-help shrink-0"
                    >
                        {{ t('components.calibrationCurve.fieldWalkAfterKick') }}:
                    </label>
                    <ToggleSwitch v-model="walkAfterKick" :disabled="saving" />
                </div>
            </div>
            <div class="text-xs text-text-color-secondary text-right">
                {{ t('components.calibrationCurve.fieldTimestamp') }}:
                {{ formatTimestamp(calibration.timestamp) }}
            </div>
        </template>
    </div>
</template>

<style scoped lang="scss">
.calibration-curve-chart {
    width: 100%;
    height: max(calc(80vh - 14rem), 20rem);
}
</style>
