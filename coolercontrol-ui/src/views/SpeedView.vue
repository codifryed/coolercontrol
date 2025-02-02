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
import { mdiAlertOutline, mdiContentSaveOutline } from '@mdi/js'
import Select from 'primevue/select'
import { nextTick, onMounted, ref, type Ref, watch } from 'vue'
import { Profile, ProfileType } from '@/models/Profile'
import { useSettingsStore } from '@/stores/SettingsStore'
import Button from 'primevue/button'
import SpeedFixedChart from '@/components/SpeedFixedChart.vue'
import SpeedGraphChart from '@/components/SpeedGraphChart.vue'
import SpeedMixChart from '@/components/SpeedMixChart.vue'
import { type UID } from '@/models/Device'
import { useDeviceStore } from '@/stores/DeviceStore'
import { storeToRefs } from 'pinia'
import {
    DeviceSettingReadDTO,
    DeviceSettingWriteManualDTO,
    DeviceSettingWriteProfileDTO,
} from '@/models/DaemonSettings'
import InputNumber from 'primevue/inputnumber'
import Slider from 'primevue/slider'
import { $enum } from 'ts-enum-util'
import { ChannelViewType, SensorAndChannelSettings } from '@/models/UISettings.ts'
import { ChartType, Dashboard, DashboardDeviceChannel } from '@/models/Dashboard.ts'
import TimeChart from '@/components/TimeChart.vue'
import SensorTable from '@/components/SensorTable.vue'
import AxisOptions from '@/components/AxisOptions.vue'
import { v4 as uuidV4 } from 'uuid'
import _ from 'lodash'
import { onBeforeRouteLeave, onBeforeRouteUpdate } from 'vue-router'
import { useConfirm } from 'primevue/useconfirm'

interface Props {
    deviceUID: UID
    channelName: string
}

const props = defineProps<Props>()

const settingsStore = useSettingsStore()
const deviceStore = useDeviceStore()
const { currentDeviceStatus } = storeToRefs(deviceStore)
const componentKey: Ref<number> = ref(0)
const confirm = useConfirm()

let contextIsDirty: boolean = false

const deviceLabel = settingsStore.allUIDeviceSettings.get(props.deviceUID)!.name
let startingManualControlEnabled = false
let startingProfile = settingsStore.profiles.find((profile) => profile.uid === '0')! // default profile as default
const startingDeviceSetting: DeviceSettingReadDTO | undefined =
    settingsStore.allDaemonDeviceSettings.get(props.deviceUID)?.settings.get(props.channelName)
const uiChannelSetting: SensorAndChannelSettings = settingsStore.allUIDeviceSettings
    .get(props.deviceUID)!
    .sensorsAndChannels.get(props.channelName)!

const channelIsControllable = (): boolean => {
    for (const device of deviceStore.allDevices()) {
        if (device.uid === props.deviceUID && device.info != null) {
            const channelInfo = device.info.channels.get(props.channelName)
            if (channelInfo != null && channelInfo.speed_options != null) {
                return channelInfo.speed_options.fixed_enabled
            }
        }
    }
    return false
}

if (channelIsControllable()) {
    if (startingDeviceSetting?.speed_fixed != null) {
        startingManualControlEnabled = true
    } else if (startingDeviceSetting?.profile_uid != null) {
        startingProfile = settingsStore.profiles.find(
            (profile) => profile.uid === startingDeviceSetting!.profile_uid,
        )!
    }
}
const selectedProfile: Ref<Profile> = ref(startingProfile)
const manualControlEnabled: Ref<boolean> = ref(startingManualControlEnabled)
const chosenViewType: Ref<ChannelViewType> = ref(
    channelIsControllable() ? uiChannelSetting.viewType : ChannelViewType.Dashboard,
)
const viewTypeOptions = channelIsControllable()
    ? [...$enum(ChannelViewType).keys()]
    : [ChannelViewType.Dashboard]

const channelLabel =
    settingsStore.allUIDeviceSettings
        .get(props.deviceUID)
        ?.sensorsAndChannels.get(props.channelName)?.name ?? props.channelName

const createNewDashboard = (): Dashboard => {
    const dash = new Dashboard(channelLabel)
    dash.timeRangeSeconds = 300
    dash.deviceChannelNames.push(new DashboardDeviceChannel(props.deviceUID, props.channelName))
    settingsStore.allUIDeviceSettings
        .get(props.deviceUID)!
        .sensorsAndChannels.get(props.channelName)!.channelDashboard = dash
    return dash
}
const singleDashboard = ref(
    settingsStore.allUIDeviceSettings
        .get(props.deviceUID)!
        .sensorsAndChannels.get(props.channelName)!.channelDashboard ?? createNewDashboard(),
)
const chartTypes = [...$enum(ChartType).values()]
const chartMinutesMin: number = 1
const chartMinutesMax: number = 60
const chartMinutes: Ref<number> = ref(singleDashboard.value.timeRangeSeconds / 60)
const chartMinutesScrolled = (event: WheelEvent): void => {
    if (event.deltaY < 0) {
        if (chartMinutes.value < chartMinutesMax) chartMinutes.value += 1
    } else {
        if (chartMinutes.value > chartMinutesMin) chartMinutes.value -= 1
    }
}

const addScrollEventListener = (): void => {
    // @ts-ignore
    document?.querySelector('.chart-minutes')?.addEventListener('wheel', chartMinutesScrolled)
}
const chartMinutesChanged = (value: number): void => {
    singleDashboard.value.timeRangeSeconds = value * 60
}
const chartKey: Ref<string> = ref(uuidV4())

const getCurrentDuty = (): number | undefined => {
    const duty = currentDeviceStatus.value.get(props.deviceUID)?.get(props.channelName)?.duty
    return duty != null ? Number(duty) : undefined
}

const manualDuty: Ref<number> = ref(getCurrentDuty() || 0)
let dutyMin = 0
let dutyMax = 100
for (const device of deviceStore.allDevices()) {
    if (device.uid === props.deviceUID && device.info != null) {
        const channelInfo = device.info.channels.get(props.channelName)
        if (channelInfo != null && channelInfo.speed_options != null) {
            dutyMin = channelInfo.speed_options.min_duty
            dutyMax = channelInfo.speed_options.max_duty
        }
    }
}

const getProfileOptions = (): any[] => {
    if (channelIsControllable()) {
        return settingsStore.profiles
    } else {
        return [settingsStore.profiles.find((profile) => profile.uid === '0')]
    }
}

const manualProfileOptions = [
    { value: false, label: 'Automatic' },
    { value: true, label: 'Manual' },
]
// todo: PWM Mode Toggle with own save function

const saveSetting = async () => {
    if (manualControlEnabled.value) {
        if (manualDuty.value == null) {
            return
        }
        const setting = new DeviceSettingWriteManualDTO(manualDuty.value)
        await settingsStore.saveDaemonDeviceSettingManual(
            props.deviceUID,
            props.channelName,
            setting,
        )
        contextIsDirty = false
    } else {
        const setting = new DeviceSettingWriteProfileDTO(selectedProfile.value.uid)
        await settingsStore.saveDaemonDeviceSettingProfile(
            props.deviceUID,
            props.channelName,
            setting,
        )
        contextIsDirty = false
    }
}

const manualScrolled = (event: WheelEvent): void => {
    if (manualDuty.value == null) return
    if (event.deltaY < 0) {
        if (manualDuty.value < dutyMax) manualDuty.value += 1
    } else {
        if (manualDuty.value > dutyMin) manualDuty.value -= 1
    }
}

const viewTypeChanged = () => {
    settingsStore.allUIDeviceSettings
        .get(props.deviceUID)!
        .sensorsAndChannels.get(props.channelName)!.viewType = chosenViewType.value
}

const checkForUnsavedChanges = (_to: any, _from: any, next: any): void => {
    if (!contextIsDirty) {
        next()
        return
    }
    confirm.require({
        message: 'There are unsaved changes made to this control channel.',
        header: 'Unsaved Changes',
        icon: 'pi pi-exclamation-triangle',
        defaultFocus: 'accept',
        rejectLabel: 'Stay',
        acceptLabel: 'Discard',
        accept: () => {
            next()
            contextIsDirty = false
        },
        reject: () => next(false),
    })
}
onMounted(() => {
    // @ts-ignore
    document.querySelector('.manual-input')?.addEventListener('wheel', manualScrolled)
    watch(manualControlEnabled, async (newValue: boolean): Promise<void> => {
        // needed if not enabled on UI mount:
        if (newValue) {
            await nextTick(async () => {
                // @ts-ignore
                document.querySelector('.manual-input')?.addEventListener('wheel', manualScrolled)
            })
        }
    })

    addScrollEventListener()
    watch(chartMinutes, (newValue: number): void => {
        chartMinutesChanged(newValue)
    })
    watch(
        settingsStore.allUIDeviceSettings,
        _.debounce(() => (chartKey.value = uuidV4()), 400, { leading: true }),
    )

    watch([manualControlEnabled, manualDuty, selectedProfile], () => {
        contextIsDirty = true
    })
    onBeforeRouteUpdate(checkForUnsavedChanges)
    onBeforeRouteLeave(checkForUnsavedChanges)
})
</script>

<template>
    <div class="flex border-b-4 border-border-one items-center justify-between">
        <div class="flex pl-4 py-2 text-2xl overflow-hidden">
            <span class="overflow-hidden overflow-ellipsis">{{ deviceLabel }}:&nbsp;</span>
            <span class="font-bold">{{ channelLabel }}</span>
        </div>
        <div class="flex flex-wrap gap-x-1 justify-end">
            <div
                v-if="chosenViewType === ChannelViewType.Control && manualControlEnabled"
                class="p-2 pr-0"
            >
                <InputNumber
                    placeholder="Duty"
                    v-model="manualDuty"
                    mode="decimal"
                    class="duty-input w-full"
                    suffix="%"
                    showButtons
                    :min="dutyMin"
                    :max="dutyMax"
                    :use-grouping="false"
                    :step="1"
                    button-layout="horizontal"
                    :input-style="{ width: '8rem' }"
                    v-tooltip.bottom="'Manual Duty'"
                >
                    <template #incrementicon>
                        <span class="pi pi-plus" />
                    </template>
                    <template #decrementicon>
                        <span class="pi pi-minus" />
                    </template>
                </InputNumber>
                <Slider
                    v-model="manualDuty"
                    class="!w-[11.5rem] ml-1.5"
                    :step="1"
                    :min="dutyMin"
                    :max="dutyMax"
                />
            </div>
            <div
                v-else-if="chosenViewType === ChannelViewType.Control && !manualControlEnabled"
                class="flex flex-row"
            >
                <!--                <div class="p-2 pr-1">-->
                <!--                    <Button-->
                <!--                        class="!p-2 w-10 h-[2.375rem]"-->
                <!--                        label="Edit"-->
                <!--                        v-tooltip.bottom="'Edit Profile'"-->
                <!--                        @click="-->
                <!--                            router.push({-->
                <!--                                name: 'profiles',-->
                <!--                                params: { profileUID: selectedProfile.uid },-->
                <!--                            })-->
                <!--                        "-->
                <!--                        :disabled="selectedProfile.uid === '0'"-->
                <!--                    >-->
                <!--                        <svg-icon-->
                <!--                            class="outline-0"-->
                <!--                            type="mdi"-->
                <!--                            :path="mdiPencilOutline"-->
                <!--                            :size="deviceStore.getREMSize(1.5)"-->
                <!--                        />-->
                <!--                    </Button>-->
                <!--                </div>-->
                <div class="p-2 pr-0">
                    <Select
                        v-model="selectedProfile"
                        :options="getProfileOptions()"
                        option-label="name"
                        placeholder="Profile"
                        class="w-full mr-4 h-full"
                        checkmark
                        dropdown-icon="pi pi-chart-line"
                        scroll-height="40rem"
                        v-tooltip.bottom="'Profile to apply'"
                    />
                </div>
            </div>
            <div
                v-else-if="
                    chosenViewType === ChannelViewType.Dashboard &&
                    singleDashboard.chartType == ChartType.TIME_CHART
                "
                class="p-2 pr-0 flex flex-row"
            >
                <InputNumber
                    placeholder="Minutes"
                    input-id="chart-minutes"
                    v-model="chartMinutes"
                    class="h-[2.375rem] chart-minutes"
                    suffix=" min"
                    show-buttons
                    :use-grouping="false"
                    :step="1"
                    :min="chartMinutesMin"
                    :max="chartMinutesMax"
                    button-layout="horizontal"
                    :allow-empty="false"
                    :input-style="{ width: '5rem' }"
                    v-tooltip.bottom="'Time Range'"
                >
                    <template #incrementicon>
                        <span class="pi pi-plus" />
                    </template>
                    <template #decrementicon>
                        <span class="pi pi-minus" />
                    </template>
                </InputNumber>
                <axis-options class="h-[2.375rem] ml-3" :dashboard="singleDashboard" />
            </div>
            <div v-if="chosenViewType === ChannelViewType.Dashboard" class="p-2 pr-0">
                <Select
                    v-model="singleDashboard.chartType"
                    :options="chartTypes"
                    placeholder="Select a Chart Type"
                    class="w-32 h-full"
                    checkmark
                    dropdown-icon="pi pi-chart-bar"
                    scroll-height="400px"
                    v-tooltip.bottom="'Chart Type'"
                />
            </div>
            <div class="p-2 pr-0 flex flex-row">
                <Select
                    v-if="chosenViewType === ChannelViewType.Control"
                    v-model="manualControlEnabled"
                    :options="manualProfileOptions"
                    option-label="label"
                    option-value="value"
                    class="w-32 mr-3"
                    placeholder="Control Type"
                    checkmark
                    dropdown-icon="pi pi-cog"
                    scroll-height="40rem"
                    v-tooltip.bottom="'Automatic or Manual'"
                />
                <div
                    v-if="!channelIsControllable()"
                    class="pr-4 py-2 flex flex-row leading-none items-center"
                    v-tooltip.bottom="
                        'The currently installed driver does not support control of this channel.'
                    "
                >
                    <svg-icon
                        type="mdi"
                        class="text-warning"
                        :path="mdiAlertOutline"
                        :size="deviceStore.getREMSize(1.5)"
                    />
                </div>
                <Select
                    v-model="chosenViewType"
                    :options="viewTypeOptions"
                    class="w-32"
                    placeholder="View Type"
                    checkmark
                    dropdown-icon="pi pi-sliders-h"
                    scroll-height="40rem"
                    @change="viewTypeChanged"
                    v-tooltip.bottom="'Control or View'"
                />
            </div>
            <div class="p-2 flex flex-row">
                <Button
                    class="bg-accent/80 hover:!bg-accent w-32 h-[2.375rem]"
                    label="Apply"
                    v-tooltip.bottom="'Apply Setting'"
                    @click="saveSetting"
                    :disabled="
                        !channelIsControllable() || chosenViewType === ChannelViewType.Dashboard
                    "
                >
                    <svg-icon
                        class="outline-0"
                        type="mdi"
                        :path="mdiContentSaveOutline"
                        :size="deviceStore.getREMSize(1.5)"
                    />
                </Button>
            </div>
        </div>
    </div>
    <div class="flex flex-col">
        <div v-if="chosenViewType === ChannelViewType.Control && manualControlEnabled">
            <SpeedFixedChart
                :duty="manualDuty"
                :current-device-u-i-d="props.deviceUID"
                :current-sensor-name="props.channelName"
                :key="'manual' + props.deviceUID + props.channelName"
            />
        </div>
        <div v-else-if="chosenViewType === ChannelViewType.Control">
            <SpeedFixedChart
                v-if="selectedProfile.p_type === ProfileType.Default"
                :default-profile="true"
                :profile="selectedProfile"
                :current-device-u-i-d="props.deviceUID"
                :current-sensor-name="props.channelName"
                :key="'default' + props.deviceUID + props.channelName + selectedProfile.uid"
            />
            <SpeedFixedChart
                v-else-if="selectedProfile.p_type === ProfileType.Fixed"
                :duty="selectedProfile.speed_fixed"
                :current-device-u-i-d="props.deviceUID"
                :current-sensor-name="props.channelName"
                :key="'fixed' + props.deviceUID + props.channelName + selectedProfile.uid"
            />
            <SpeedGraphChart
                v-else-if="selectedProfile.p_type === ProfileType.Graph"
                :profile="selectedProfile"
                :current-device-u-i-d="props.deviceUID"
                :current-sensor-name="props.channelName"
                :key="
                    'graph' +
                    componentKey +
                    props.deviceUID +
                    props.channelName +
                    selectedProfile.uid
                "
            />
            <SpeedMixChart
                v-else-if="selectedProfile.p_type === ProfileType.Mix"
                :profile="selectedProfile"
                :current-device-u-i-d="props.deviceUID"
                :current-sensor-name="props.channelName"
                :key="
                    'mix' + componentKey + props.deviceUID + props.channelName + selectedProfile.uid
                "
            />
        </div>
        <div v-else-if="chosenViewType === ChannelViewType.Dashboard">
            <TimeChart
                v-if="singleDashboard.chartType == ChartType.TIME_CHART"
                :dashboard="singleDashboard"
                :key="chartKey"
            />
            <SensorTable
                v-else-if="singleDashboard.chartType == ChartType.TABLE"
                :dashboard="singleDashboard"
                :key="'table' + chartKey"
            />
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
