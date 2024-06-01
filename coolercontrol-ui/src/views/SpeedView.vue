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
import Dropdown from 'primevue/dropdown'
import { computed, nextTick, onMounted, ref, type Ref, watch } from 'vue'
import { Profile, ProfileType } from '@/models/Profile'
import { useSettingsStore } from '@/stores/SettingsStore'
import Button from 'primevue/button'
import SpeedDefaultChart from '@/components/SpeedDefaultChart.vue'
import SpeedFixedChart from '@/components/SpeedFixedChart.vue'
import SpeedGraphChart from '@/components/SpeedGraphChart.vue'
import SpeedMixChart from '@/components/SpeedMixChart.vue'
import { type UID } from '@/models/Device'
import { useDeviceStore } from '@/stores/DeviceStore'
import MiniGauge from '@/components/MiniGauge.vue'
import { storeToRefs } from 'pinia'
import {
    DeviceSettingReadDTO,
    DeviceSettingWriteManualDTO,
    DeviceSettingWriteProfileDTO,
} from '@/models/DaemonSettings'
import SelectButton from 'primevue/selectbutton'
import InputNumber from 'primevue/inputnumber'
import Slider from 'primevue/slider'
import { useDialog } from 'primevue/usedialog'
import ProfileEditor from '@/components/ProfileEditor.vue'
import FunctionEditor from '@/components/FunctionEditor.vue'

interface Props {
    deviceId: UID
    name: string
}

const props = defineProps<Props>()

const settingsStore = useSettingsStore()
const deviceStore = useDeviceStore()
const { currentDeviceStatus } = storeToRefs(deviceStore)
const dialog = useDialog()
const componentKey: Ref<number> = ref(0)
let startingManualControlEnabled = false
let startingProfile = settingsStore.profiles.find((profile) => profile.uid === '0')! // default profile as default
const startingDeviceSetting: DeviceSettingReadDTO | undefined =
    settingsStore.allDaemonDeviceSettings.get(props.deviceId)?.settings.get(props.name)
if (startingDeviceSetting?.speed_fixed != null) {
    startingManualControlEnabled = true
} else if (startingDeviceSetting?.profile_uid != null) {
    startingProfile = settingsStore.profiles.find(
        (profile) => profile.uid === startingDeviceSetting!.profile_uid,
    )!
}
const selectedProfile: Ref<Profile> = ref(startingProfile)
const memberProfiles: Ref<Array<Profile>> = computed(() => {
    if (selectedProfile.value.p_type === ProfileType.Mix) {
        return settingsStore.profiles.filter((profile) =>
            selectedProfile.value.member_profile_uids.includes(profile.uid),
        )
    }
    return []
})
const manualControlEnabled: Ref<boolean> = ref(startingManualControlEnabled)
const editProfileEnabled = () => {
    return !manualControlEnabled.value && selectedProfile.value.uid !== '0'
}
const editFunctionEnabled = () => {
    return (
        !manualControlEnabled.value &&
        selectedProfile.value.uid !== '0' &&
        selectedProfile.value.function_uid !== '0'
    )
}

const goToProfile = (): void => {
    dialog.open(ProfileEditor, {
        props: {
            header: 'Edit Profile',
            position: 'center',
            modal: true,
            dismissableMask: false,
        },
        data: {
            profileUID: selectedProfile.value.uid,
        },
        onClose: (options: any) => {
            componentKey.value += 1 // refreshes the graph after profile changes, so they are correctly displayed
            const data = options.data
            if (data && data.functionUID != null) {
                dialog.open(FunctionEditor, {
                    props: {
                        header: 'Edit Function',
                        position: 'center',
                        modal: true,
                        dismissableMask: false,
                    },
                    data: {
                        functionUID: data.functionUID,
                    },
                })
            }
        },
    })
}
const goToFunction = (): void => {
    dialog.open(FunctionEditor, {
        props: {
            header: 'Edit Function',
            position: 'center',
            modal: true,
            dismissableMask: false,
        },
        data: {
            functionUID: selectedProfile.value.function_uid,
        },
    })
}
const getCurrentDuty = (): number | undefined => {
    const duty = currentDeviceStatus.value.get(props.deviceId)?.get(props.name)?.duty
    return duty != null ? Number(duty) : undefined
}

const manualDuty: Ref<number> = ref(getCurrentDuty() || 0)
const dutyMin = 0
const dutyMax = 100

const channelIsControllable = (): boolean => {
    for (const device of deviceStore.allDevices()) {
        if (device.uid === props.deviceId && device.info != null) {
            const channelInfo = device.info.channels.get(props.name)
            if (channelInfo != null && channelInfo.speed_options != null) {
                return true
            }
        }
    }
    return false
}

const getProfileOptions = (): any[] => {
    if (channelIsControllable()) {
        return settingsStore.profiles
    } else {
        return [settingsStore.profiles.find((profile) => profile.uid === '0')]
    }
}

const manualProfileOptions = [
    { value: true, label: 'Manual' },
    { value: false, label: 'Profiles' },
]
// todo: PWM Mode Toggle with own save function

const saveSetting = async () => {
    if (manualControlEnabled.value) {
        if (manualDuty.value == null) {
            return
        }
        const setting = new DeviceSettingWriteManualDTO(manualDuty.value)
        await settingsStore.saveDaemonDeviceSettingManual(props.deviceId, props.name, setting)
    } else {
        const setting = new DeviceSettingWriteProfileDTO(selectedProfile.value.uid)
        await settingsStore.saveDaemonDeviceSettingProfile(props.deviceId, props.name, setting)
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

const applyButton = ref()
nextTick(async () => {
    const delay = () => new Promise((resolve) => setTimeout(resolve, 100))
    await delay()
    applyButton.value.$el.focus()
})

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
})
</script>

<template>
    <div class="card pt-2">
        <div class="flex">
            <div class="flex-inline control-column">
                <div v-if="channelIsControllable()" class="mt-2">
                    <SelectButton
                        v-model="manualControlEnabled"
                        :options="manualProfileOptions"
                        option-label="label"
                        option-value="value"
                        :allow-empty="false"
                        class="w-full"
                        :pt="{ label: { style: 'width: 3.80rem' } }"
                        v-tooltip.top="{
                            value: 'Select whether to control manually, or apply a profile',
                            showDelay: 700,
                        }"
                    />
                </div>
                <div v-if="manualControlEnabled" class="manual-input p-float-label mt-5">
                    <InputNumber
                        placeholder="Duty"
                        v-model="manualDuty"
                        inputId="dd-brightness"
                        mode="decimal"
                        class="w-full"
                        suffix="%"
                        :step="1"
                        :input-style="{ width: '60px' }"
                        :min="dutyMin"
                        :max="dutyMax"
                    />
                    <Slider
                        v-model="manualDuty"
                        :step="1"
                        :min="dutyMin"
                        :max="dutyMax"
                        class="w-full mt-0"
                    />
                    <label for="dd-duty">Duty</label>
                </div>
                <div v-else class="p-float-label mt-5">
                    <Dropdown
                        v-model="selectedProfile"
                        inputId="dd-profile"
                        :options="getProfileOptions()"
                        option-label="name"
                        placeholder="Profile"
                        class="w-full"
                        scroll-height="400px"
                        :disabled="manualControlEnabled"
                    />
                    <label for="dd-profile">Profile</label>
                </div>
                <Button
                    label="Edit Profile"
                    class="mt-6 w-full"
                    outlined
                    :disabled="!editProfileEnabled()"
                    @click="goToProfile"
                >
                    <span class="p-button-label">Edit Profile</span>
                </Button>
                <Button
                    label="Edit Function"
                    class="mt-5 w-full"
                    outlined
                    :disabled="!editFunctionEnabled()"
                    @click="goToFunction"
                >
                    <span class="p-button-label">Edit Function</span>
                </Button>
                <Button ref="applyButton" label="Apply" class="mt-5 w-full" @click="saveSetting">
                    <span class="p-button-label">Apply</span>
                </Button>
                <div v-if="!manualControlEnabled">
                    <div v-if="selectedProfile.p_type === ProfileType.Graph" class="mt-6">
                        <MiniGauge
                            :device-u-i-d="selectedProfile.temp_source!.device_uid"
                            :sensor-name="selectedProfile.temp_source!.temp_name"
                            :key="'temp' + props.deviceId + props.name + selectedProfile.uid"
                            temp
                        />
                        <MiniGauge
                            v-if="getCurrentDuty() != null"
                            :device-u-i-d="props.deviceId"
                            :sensor-name="props.name"
                            :key="'duty' + props.deviceId + props.name + selectedProfile.uid"
                            duty
                        />
                    </div>
                    <div v-if="selectedProfile.p_type === ProfileType.Mix" class="mt-6">
                        <MiniGauge
                            v-for="memberProfile in memberProfiles"
                            :device-u-i-d="memberProfile.temp_source!.device_uid"
                            :sensor-name="memberProfile.temp_source!.temp_name"
                            :key="'temp' + props.deviceId + props.name + memberProfile.uid"
                            temp
                        />
                        <MiniGauge
                            v-if="getCurrentDuty() != null"
                            :device-u-i-d="props.deviceId"
                            :sensor-name="props.name"
                            :key="'duty' + props.deviceId + props.name + selectedProfile.uid"
                            duty
                        />
                    </div>
                </div>
            </div>
            <div class="flex-1 pb-0">
                <div v-if="manualControlEnabled">
                    <SpeedFixedChart
                        :duty="manualDuty"
                        :current-device-u-i-d="props.deviceId"
                        :current-sensor-name="props.name"
                        :key="'manual' + props.deviceId + props.name + selectedProfile.uid"
                    />
                </div>
                <div v-else>
                    <SpeedDefaultChart
                        v-if="selectedProfile.p_type === ProfileType.Default"
                        :profile="selectedProfile"
                        :current-device-u-i-d="props.deviceId"
                        :current-sensor-name="props.name"
                        :key="'default' + props.deviceId + props.name + selectedProfile.uid"
                    />
                    <SpeedFixedChart
                        v-else-if="selectedProfile.p_type === ProfileType.Fixed"
                        :duty="selectedProfile.speed_fixed"
                        :current-device-u-i-d="props.deviceId"
                        :current-sensor-name="props.name"
                        :key="'fixed' + props.deviceId + props.name + selectedProfile.uid"
                    />
                    <SpeedGraphChart
                        v-else-if="selectedProfile.p_type === ProfileType.Graph"
                        :profile="selectedProfile"
                        :current-device-u-i-d="props.deviceId"
                        :current-sensor-name="props.name"
                        :key="
                            'graph' +
                            componentKey +
                            props.deviceId +
                            props.name +
                            selectedProfile.uid
                        "
                    />
                    <SpeedMixChart
                        v-else-if="selectedProfile.p_type === ProfileType.Mix"
                        :profile="selectedProfile"
                        :current-device-u-i-d="props.deviceId"
                        :current-sensor-name="props.name"
                        :key="
                            'mix' + componentKey + props.deviceId + props.name + selectedProfile.uid
                        "
                    />
                </div>
            </div>
        </div>
    </div>
</template>

<style scoped lang="scss">
.control-column {
    width: 14rem;
    padding-right: 1rem;
    margin-right: 1rem;
}
</style>
