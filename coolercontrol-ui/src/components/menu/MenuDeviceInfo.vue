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
import { mdiInformationSlabCircleOutline } from '@mdi/js'
import { DeviceType, getDeviceTypeDisplayName, UID } from '@/models/Device.ts'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { PopoverContent, PopoverRoot, PopoverTrigger } from 'radix-vue'
import { ref, Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { DriverType, getDriverTypeDisplayName } from '@/models/DeviceInfo'

interface Props {
    deviceUID: UID
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'open', value: boolean): void
}>()

const deviceStore = useDeviceStore()
const { t } = useI18n()
const systemDeviceName: Ref<string> = ref('')
const deviceType: Ref<DeviceType> = ref('' as unknown as DeviceType)
const firmwareVersion: Ref<string> = ref('')
const model: Ref<string> = ref('')
const driverType: Ref<DriverType> = ref('' as unknown as DriverType)
const driverName: Ref<string> = ref('')
const driverVersion: Ref<string> = ref('')
const driverLocations: Ref<Array<string>> = ref([])
for (const device of deviceStore.allDevices()) {
    if (device.uid === props.deviceUID) {
        systemDeviceName.value = device.name
        deviceType.value = device.type
        firmwareVersion.value = device.lc_info?.firmware_version ?? ''
        if (device.info != null) {
            model.value = device.info.model ?? ''
            driverType.value = device.info.driver_info.drv_type
            driverName.value = device.info.driver_info.name ?? ''
            driverVersion.value = device.info.driver_info.version ?? ''
            driverLocations.value = device.info.driver_info.locations
        }
        break
    }
}
</script>

<template>
    <div v-tooltip.top="{ value: t('components.deviceInfo.details') }">
        <popover-root @update:open="(value) => emit('open', value)">
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
                    class="w-full bg-bg-two border border-border-one p-4 rounded-lg text-text-color"
                >
                    <table>
                        <tbody>
                            <tr>
                                <td class="table-data font-bold text-lg text-end">
                                    {{ t('components.deviceInfo.systemName') }}
                                </td>
                                <td class="table-data">{{ systemDeviceName }}</td>
                            </tr>
                            <tr>
                                <td class="table-data font-bold text-lg text-end">
                                    {{ t('components.deviceInfo.deviceType') }}
                                </td>
                                <td class="table-data">
                                    {{ getDeviceTypeDisplayName(deviceType) }}
                                </td>
                            </tr>
                            <tr>
                                <td class="table-data font-bold text-lg text-end">
                                    {{ t('components.deviceInfo.deviceUID') }}
                                </td>
                                <td class="table-data">{{ props.deviceUID }}</td>
                            </tr>
                            <tr v-if="firmwareVersion">
                                <td class="table-data font-bold text-lg text-end">
                                    {{ t('components.deviceInfo.firmwareVersion') }}
                                </td>
                                <td class="table-data">{{ firmwareVersion }}</td>
                            </tr>
                            <tr v-if="model">
                                <td class="table-data font-bold text-lg text-end">
                                    {{ t('components.deviceInfo.model') }}
                                </td>
                                <td class="table-data">{{ model }}</td>
                            </tr>
                            <tr v-if="driverName">
                                <td class="table-data font-bold text-lg text-end">
                                    {{ t('components.deviceInfo.driverName') }}
                                </td>
                                <td class="table-data">{{ driverName }}</td>
                            </tr>
                            <tr>
                                <td class="table-data font-bold text-lg text-end">
                                    {{ t('components.deviceInfo.driverType') }}
                                </td>
                                <td class="table-data">
                                    {{ getDriverTypeDisplayName(driverType) }}
                                </td>
                            </tr>
                            <tr v-if="driverVersion">
                                <td class="table-data font-bold text-lg text-end">
                                    {{ t('components.deviceInfo.driverVersion') }}
                                </td>
                                <td class="table-data">{{ driverVersion }}</td>
                            </tr>
                            <tr v-if="driverLocations">
                                <td class="table-data font-bold text-lg text-end align-text-top">
                                    {{ t('components.deviceInfo.locations') }}
                                </td>
                                <td class="table-data">
                                    <p
                                        v-for="driverLocation in driverLocations"
                                        class="leading-loose"
                                    >
                                        {{ driverLocation }}
                                    </p>
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
.table-data {
    padding: 0.5rem;
    border: 1px solid rgb(var(--colors-border-one));
}
</style>
