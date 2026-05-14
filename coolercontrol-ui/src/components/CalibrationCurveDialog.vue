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
import { inject, onMounted, ref, type Ref } from 'vue'
import type { DynamicDialogInstance } from 'primevue/dynamicdialogoptions'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore'
import { useCalibrationStore } from '@/stores/CalibrationStore'
import type { Calibration } from '@/models/Calibration'
import type { UID } from '@/models/Device'

echarts.use([CanvasRenderer, LineChart, GridComponent, LegendComponent, TooltipComponent])

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const { t } = useI18n()
const deviceStore = useDeviceStore()
const colors = useThemeColorsStore()
const calibrationStore = useCalibrationStore()

const deviceUID: UID = dialogRef.value.data.deviceUID
const channelName: string = dialogRef.value.data.channelName
const initialCalibration: Calibration | undefined = dialogRef.value.data.calibration

const calibration: Ref<Calibration | undefined> = ref(initialCalibration)
const loading: Ref<boolean> = ref(initialCalibration == null)
const loadError: Ref<string | undefined> = ref()

onMounted(async () => {
    if (calibration.value != null) return
    try {
        const stored = await calibrationStore.getStored(deviceUID, channelName)
        if (stored == null) {
            loadError.value = t('components.calibrationCurve.notFound')
        } else {
            calibration.value = stored
        }
    } catch {
        loadError.value = t('components.calibrationCurve.loadError')
    } finally {
        loading.value = false
    }
})

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
    const downColor = colors.themeColors.yellow
    return {
        grid: {
            show: false,
            top: deviceStore.getREMSize(2.5),
            left: deviceStore.getREMSize(0.5),
            right: deviceStore.getREMSize(1.5),
            bottom: deviceStore.getREMSize(2.5),
            containLabel: true,
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
                <div class="flex justify-between md:block">
                    <span class="text-text-color-secondary mr-2">
                        {{ t('components.calibrationCurve.fieldCurveKind') }}:
                    </span>
                    <span class="font-medium">{{ curveKindLabel(calibration.curve_kind) }}</span>
                </div>
                <div class="flex justify-between md:block">
                    <span class="text-text-color-secondary mr-2">
                        {{ t('components.calibrationCurve.fieldRpmMax') }}:
                    </span>
                    <span class="font-medium">{{ calibration.rpm_max }} RPM</span>
                </div>
                <div class="flex justify-between md:block">
                    <span class="text-text-color-secondary mr-2">
                        {{ t('components.calibrationCurve.fieldKick') }}:
                    </span>
                    <span class="font-medium">{{ calibration.kick_duration_ms }} ms</span>
                </div>
                <div class="flex justify-between md:block">
                    <span class="text-text-color-secondary mr-2">
                        {{ t('components.calibrationCurve.fieldStart') }}:
                    </span>
                    <span class="font-medium">
                        {{ calibration.min_start_duty }}{{ t('common.percentUnit') }}
                    </span>
                </div>
                <div class="flex justify-between md:block">
                    <span class="text-text-color-secondary mr-2">
                        {{ t('components.calibrationCurve.fieldSustain') }}:
                    </span>
                    <span class="font-medium">
                        {{ calibration.min_sustain_duty }}{{ t('common.percentUnit') }}
                    </span>
                </div>
                <div class="flex justify-between md:block">
                    <span class="text-text-color-secondary mr-2">
                        {{ t('components.calibrationCurve.fieldSaturate') }}:
                    </span>
                    <span class="font-medium">
                        {{ calibration.max_eff_duty }}{{ t('common.percentUnit') }}
                    </span>
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
