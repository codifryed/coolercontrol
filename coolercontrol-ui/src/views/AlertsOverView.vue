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
import SvgIcon from '@jamescoyle/vue-icon'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'radix-vue'
import DataTable from 'primevue/datatable'
import Column from 'primevue/column'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { AlertState, getAlertStateDisplayName } from '@/models/Alert.ts'
import { mdiBellOutline, mdiBellRingOutline } from '@mdi/js'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useI18n } from 'vue-i18n'

const settingsStore = useSettingsStore()
const { getREMSize } = useDeviceStore()
const { t } = useI18n()
</script>

<template>
    <div class="flex h-[3.5rem] border-b-4 border-border-one items-center justify-between">
          <div class="pl-4 py-2 text-2xl font-bold">{{ t('views.alerts.alertsOverview') }}</div>
    </div>
    <ScrollAreaRoot style="--scrollbar-size: 10px">
        <ScrollAreaViewport class="p-4 pb-16 h-screen w-full">
            <div class="mt-8 flex flex-col">
                  <span class="pb-1 ml-1 font-semibold text-xl text-text-color">{{ t('views.alerts.createAlert') }}</span>
                <div class="flex flex-row">
                    <DataTable class="w-[31rem]" :value="settingsStore.alerts">
                        <Column header="">
                            <template #body="slotProps">
                                <svg-icon
                                    type="mdi"
                                    :class="{
                                        'text-error': slotProps.data.state === AlertState.Active,
                                    }"
                                    :path="
                                        slotProps.data.state === AlertState.Active
                                            ? mdiBellRingOutline
                                            : mdiBellOutline
                                    "
                                    :size="getREMSize(1.5)"
                                />
                            </template>
                        </Column>
                        <Column field="state" :header="t('common.state')">
                            <template #body="slotProps">
                                <span :class="{
                                    'text-error': slotProps.data.state === AlertState.Active,
                                    'text-success': slotProps.data.state === AlertState.Inactive,
                                }">
                                    {{ getAlertStateDisplayName(slotProps.data.state) }}
                                </span>
                            </template>
                        </Column>
                        <Column field="name" :header="t('common.name')" body-class="w-full text-ellipsis" />
                    </DataTable>
                    <div class="w-full" />
                </div>
            </div>
            <div class="mt-8 flex flex-col">
                  <span class="pb-1 ml-1 font-semibold text-xl text-text-color">{{ t('views.alerts.alertLogs') }}</span>
                <DataTable
                    class="w-full"
                    :value="settingsStore.alertLogs"
                    sort-field="timestamp"
                    :sort-order="-1"
                >
                      <Column field="timestamp" :header="t('common.timestamp')" :sortable="true">
                        <template #body="slotProps">
                            {{ new Date(slotProps.data.timestamp).toLocaleString() }}
                        </template>
                    </Column>
                    <Column field="state" :header="t('common.state')">
                        <template #body="slotProps">
                            <span
                                :class="{
                                    'text-error': slotProps.data.state === AlertState.Active,
                                    'text-success': slotProps.data.state === AlertState.Inactive,
                                }"
                            >
                                {{ getAlertStateDisplayName(slotProps.data.state) }}
                            </span>
                        </template>
                    </Column>
                      <Column field="name" :header="t('common.name')" />
                      <Column field="message" :header="t('common.message')" body-class="w-full text-ellipsis" />
                </DataTable>
            </div>
        </ScrollAreaViewport>
        <ScrollAreaScrollbar
            class="flex select-none touch-none p-0.5 bg-transparent transition-colors duration-[120ms] ease-out data-[orientation=vertical]:w-2.5"
            orientation="vertical"
        >
            <ScrollAreaThumb
                class="flex-1 bg-border-one opacity-80 rounded-lg relative before:content-[''] before:absolute before:top-1/2 before:left-1/2 before:-translate-x-1/2 before:-translate-y-1/2 before:w-full before:h-full before:min-w-[44px] before:min-h-[44px]"
            />
        </ScrollAreaScrollbar>
    </ScrollAreaRoot>
</template>

<style scoped lang="scss">
.table-data {
    padding: 0.5rem;
    border: 1px solid rgb(var(--colors-border-one));
}
</style>
