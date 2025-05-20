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
import SvgIcon from '@jamescoyle/vue-icon'
import { ChartType, Dashboard } from '@/models/Dashboard.ts'
import ControlsOverview from '@/components/ControlsOverview.vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { mdiHelpCircleOutline } from '@mdi/js'
import { useI18n } from 'vue-i18n'
import { onMounted, ref, Ref, watch } from 'vue'
import { v4 as uuidV4 } from 'uuid'
import _ from 'lodash'

const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const dashboard = new Dashboard('System Controls')
dashboard.chartType = ChartType.CONTROLS

const chartKey: Ref<string> = ref(uuidV4())
onMounted(async () => {
    // This forces a debounced chart re-draw for any dashboard-related settings change:
    watch(
        [settingsStore.allUIDeviceSettings],
        _.debounce(() => (chartKey.value = uuidV4()), 400, { leading: true }),
    )
    watch(
        settingsStore.allDaemonDeviceSettings,
        _.debounce(() => (chartKey.value = uuidV4()), 400, { leading: true }),
    )
})
</script>

<template>
    <div id="control-panel" class="flex border-b-4 border-border-one items-center justify-between">
        <div class="flex pl-4 py-2 text-2xl overflow-hidden">
            <span class="font-bold overflow-hidden overflow-ellipsis">{{ dashboard.name }}</span>
        </div>
        <div class="flex flex-wrap gap-x-1 justify-end mr-4">
            <svg-icon
                type="mdi"
                class="mr-2 inline"
                :path="mdiHelpCircleOutline"
                :size="deviceStore.getREMSize(1.3)"
            />
            {{ t('layout.settings.devices.detectionIssues') }}
            <a
                target="_blank"
                href="https://docs.coolercontrol.org/hardware-support.html"
                class="text-accent"
            >
                {{ t('layout.settings.devices.hardwareSupportDoc') }}
            </a>
        </div>
    </div>
    <ControlsOverview :dashboard="dashboard" :key="'system-controls' + chartKey" />
</template>

<style scoped lang="scss"></style>
