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
import { Reactive, reactive, Ref, ref, onMounted, watch, computed, ComputedRef } from 'vue'
import { ElDropdown, ElTree } from 'element-plus'
import 'element-plus/es/components/tree/style/css'
import InputText from 'primevue/inputtext'
import IconField from 'primevue/iconfield'
import InputIcon from 'primevue/inputicon'
import { ChannelValues, useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import {
    mdiChartBoxMultipleOutline,
    mdiChartBoxOutline,
    mdiChartLine,
    mdiChartMultiple,
    mdiCircleMultipleOutline,
    mdiFan,
    mdiFlask,
    mdiFlaskOutline,
    mdiLayersOutline,
    mdiLayersTripleOutline,
    mdiLedOn,
    mdiLightningBolt,
    mdiMagnify,
    mdiMemory,
    mdiPinOutline,
    mdiSineWave,
    mdiSpeedometer,
    mdiTelevisionShimmer,
    mdiThermometer,
} from '@mdi/js'
import { Color, DeviceType, UID } from '@/models/Device.ts'
import MenuRename from '@/components/MenuRename.vue'
import MenuHide from '@/components/MenuHide.vue'
import MenuColor from '@/components/MenuColor.vue'
import MenuHideAll from '@/components/MenuHideAll.vue'
import MenuDisable from '@/components/MenuDisable.vue'

// interface Tree {
//     label: string
//     children?: Tree[]
// }
interface Tree {
    // necessary for test filter
    [key: string]: any
}

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
// const dialog = useDialog()
// const toast = useToast()

const deviceChannelValues = (deviceUID: UID, channelName: string): ChannelValues | undefined =>
    deviceStore.currentDeviceStatus.get(deviceUID)?.get(channelName)
const deviceChannelHidden = (deviceUID: UID, channelName: string): ComputedRef<boolean> =>
    computed(
        () =>
            settingsStore.allUIDeviceSettings.get(deviceUID)?.sensorsAndChannels.get(channelName)
                ?.hide ?? false,
    )
const deviceChannelColor = (deviceUID: UID, channelName: string): Ref<Color> => {
    let color = ref('')
    if (deviceUID == null) {
    } else if (
        deviceUID.startsWith('Dashboards') ||
        deviceUID.startsWith('Modes') ||
        deviceUID.startsWith('Profiles') ||
        deviceUID.startsWith('Functions')
    ) {
        color.value = '#568af2' // accent color todo: get css color dynamically
    } else {
        color.value =
            settingsStore.allUIDeviceSettings.get(deviceUID)?.sensorsAndChannels.get(channelName)
                ?.color ?? ''
    }
    return color
}

const filterText: Ref<string> = ref('')
const treeRef = ref<InstanceType<typeof ElTree>>()
const nodeProps = {
    children: 'children',
    label: 'label',
}
// todo: will have to see if there are advantages to a deep reactive menu: (probably not - as things are not directly connected)
const data: Reactive<Tree[]> = reactive([])
const createTreeMenu = (): void => {
    data.length = 0
    data.push(pinnedTree())
    data.push(dashboardsTree())
    data.push(modesTree())
    data.push(profilesTree())
    data.push(functionsTree())
    data.push(customSensorsTree())
    data.push(...devicesTreeArray())
}
const pinnedTree = (): any => {
    // todo: only add pinned node if there are pins
    return {
        id: 'pinned',
        label: 'Pinned',
        icon: mdiPinOutline,
        name: null, // devices should not have names
        options: [],
    }
}
const dashboardsTree = (): any => {
    return {
        id: 'dashboards',
        label: 'Dashboards',
        icon: mdiChartBoxMultipleOutline,
        name: null, // devices should not have names
        options: [],
        children: [
            {
                id: 'dashboards_system-id',
                label: 'System',
                icon: mdiChartBoxOutline,
                deviceUID: 'Dashboards',
                name: 'system-id',
                to: { name: 'system-overview' },
                options: [],
            },
        ],
    }
}
const modesTree = (): any => {
    const modeChildren = []
    for (const mode of settingsStore.modes) {
        modeChildren.push({
            id: `modes_${mode.uid}`,
            label: mode.name,
            icon: mdiLayersOutline,
            deviceUID: 'Modes',
            name: mode.uid,
            isActive: settingsStore.modeActive === mode.uid,
            options: [],
        })
    }
    return {
        id: 'modes',
        label: 'Modes',
        icon: mdiLayersTripleOutline,
        name: null, // devices should not have names
        options: [],
        children: modeChildren,
    }
}

const profilesTree = (): any => {
    const profileChildren = []
    for (const profile of settingsStore.profiles) {
        if (profile.uid === '0') continue // do not display the default profile
        profileChildren.push({
            id: `profiles_${profile.uid}`,
            label: profile.name,
            icon: mdiChartLine,
            deviceUID: 'Profiles',
            name: profile.uid,
            options: [],
        })
    }
    return {
        id: 'profiles',
        label: 'Profiles',
        name: null, // devices should not have names
        icon: mdiChartMultiple,
        options: [],
        children: profileChildren,
    }
}

const functionsTree = (): any => {
    const functionChildren = []
    for (const fun of settingsStore.functions) {
        if (fun.uid === '0') continue // do not display the default function
        functionChildren.push({
            id: `functions_${fun.uid}`,
            label: fun.name,
            icon: mdiFlask,
            deviceUID: 'Functions',
            name: fun.uid,
            options: [{ functionEdit: true }, { functionDelete: true }],
        })
    }
    return {
        id: 'functions',
        label: 'Functions',
        icon: mdiFlaskOutline,
        name: null, // devices should not have names
        options: [{ functionAdd: true }],
        children: functionChildren,
    }
}
const customSensorsTree = (): any => {
    const sensorsChildren = []
    let deviceUID = ''
    for (const device of deviceStore.allDevices()) {
        if (device.type !== DeviceType.CUSTOM_SENSORS) {
            continue
        }
        deviceUID = device.uid
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
        for (const temp of device.status.temps) {
            sensorsChildren.push({
                id: `customer-sensors_${temp.name}`,
                label: deviceSettings.sensorsAndChannels.get(temp.name)!.name,
                name: temp.name,
                hasColor: true,
                color: deviceChannelColor(device.uid, temp.name),
                icon: mdiThermometer,
                to: { name: 'device-temp', params: { deviceId: device.uid, name: temp.name } },
                deviceUID: device.uid,
                temp: temp.temp.toFixed(1),
                options: [
                    { hide: true },
                    { color: true },
                    { sensorEdit: true },
                    { sensorDelete: true },
                ],
            })
        }
        return {
            id: 'custom-sensors',
            label: 'Custom Sensors',
            icon: mdiCircleMultipleOutline,
            name: null, // devices should not have names
            deviceUID: deviceUID,
            options: [{ hideAll: true }, { sensorAdd: true }],
            children: sensorsChildren,
        }
    }
}

const devicesTreeArray = (): any[] => {
    const allDevices = []
    for (const device of deviceStore.allDevices()) {
        if (device.type === DeviceType.CUSTOM_SENSORS) {
            continue // has its own dedicated menu above
        }
        const deviceSettings = settingsStore.allUIDeviceSettings.get(device.uid)!
        const deviceItem = {
            id: device.uid,
            label: deviceSettings.name,
            name: null, // devices should not have names
            icon: mdiMemory,
            deviceUID: device.uid,
            children: [],
            options: [{ hideAll: true }, { rename: true }, { disable: true }],
        }
        for (const temp of device.status.temps) {
            // @ts-ignore
            deviceItem.children.push({
                id: `${device.uid}_${temp.name}`,
                label: deviceSettings.sensorsAndChannels.get(temp.name)!.name,
                name: temp.name,
                hasColor: true,
                color: deviceChannelColor(device.uid, temp.name),
                icon: mdiThermometer,
                to: { name: 'device-temp', params: { deviceId: device.uid, name: temp.name } },
                deviceUID: device.uid,
                temp: temp.temp.toFixed(1),
                options: [{ hide: true }, { rename: true }, { color: true }],
            })
        }
        for (const channel of device.status.channels) {
            if (channel.name.toLowerCase().includes('freq')) {
                // @ts-ignore
                deviceItem.children.push({
                    id: `${device.uid}_${channel.name}`,
                    label: deviceSettings.sensorsAndChannels.get(channel.name)!.name,
                    name: channel.name,
                    hasColor: true,
                    color: deviceChannelColor(device.uid, channel.name),
                    icon: mdiSineWave,
                    to: {
                        name: 'device-freq',
                        params: { deviceId: device.uid, name: channel.name },
                    },
                    deviceUID: device.uid,
                    freq: channel.freq,
                    options: [{ hide: true }, { rename: true }, { color: true }],
                })
            }
        }
        for (const channel of device.status.channels) {
            if (channel.name.toLowerCase().includes('load')) {
                // @ts-ignore
                deviceItem.children.push({
                    id: `${device.uid}_${channel.name}`,
                    label: deviceSettings.sensorsAndChannels.get(channel.name)!.name,
                    name: channel.name,
                    hasColor: true,
                    color: deviceChannelColor(device.uid, channel.name),
                    icon: mdiSpeedometer,
                    to: {
                        name: 'device-load',
                        params: { deviceId: device.uid, name: channel.name },
                    },
                    deviceUID: device.uid,
                    duty: channel.duty,
                    rpm: channel.rpm,
                    options: [{ hide: true }, { rename: true }, { color: true }],
                })
            }
        }
        if (device.info != null) {
            for (const [channelName, channelInfo] of device.info.channels.entries()) {
                if (channelInfo.speed_options === null) {
                    continue
                }
                // need to get the status data to properly setup the menu item
                let duty: number | undefined = undefined
                let rpm: number | undefined = undefined
                for (const channel of device.status.channels) {
                    if (channel.name === channelName) {
                        duty = channel.duty
                        rpm = channel.rpm
                        break
                    }
                }
                // @ts-ignore
                deviceItem.children.push({
                    id: `${device.uid}_${channelName}`,
                    label: deviceSettings.sensorsAndChannels.get(channelName)!.name,
                    name: channelName,
                    hasColor: true,
                    color: deviceChannelColor(device.uid, channelName),
                    icon: mdiFan,
                    to: {
                        name: 'device-speed',
                        params: { deviceId: device.uid, name: channelName },
                    },
                    deviceUID: device.uid,
                    duty: duty,
                    rpm: rpm,
                    options: [{ hide: true }, { rename: true }, { color: true }],
                })
            }
            for (const [channelName, channelInfo] of device.info.channels.entries()) {
                if (channelInfo.lighting_modes.length === 0) {
                    continue
                }
                // @ts-ignore
                deviceItem.children.push({
                    id: `${device.uid}_${channelName}`,
                    label: deviceSettings.sensorsAndChannels.get(channelName)!.name,
                    name: channelName,
                    icon: mdiLedOn,
                    color: deviceChannelColor(device.uid, channelName),
                    to: {
                        name: 'device-lighting',
                        params: { deviceId: device.uid, name: channelName },
                    },
                    deviceUID: device.uid,
                    options: [{ hide: true }, { rename: true }],
                })
            }
            for (const [channelName, channelInfo] of device.info.channels.entries()) {
                if (channelInfo.lcd_modes.length === 0) {
                    continue
                }
                // @ts-ignore
                deviceItem.children.push({
                    id: `${device.uid}_${channelName}`,
                    label: deviceSettings.sensorsAndChannels.get(channelName)!.name,
                    name: channelName,
                    icon: mdiTelevisionShimmer,
                    color: deviceChannelColor(device.uid, channelName),
                    to: { name: 'device-lcd', params: { deviceId: device.uid, name: channelName } },
                    deviceUID: device.uid,
                    options: [{ hide: true }, { rename: true }],
                })
            }
        }
        allDevices.push(deviceItem)
    }
    return allDevices
}

createTreeMenu()

const applyFilter = (val: string) => {
    treeRef.value!.filter(val.toLowerCase())
}
const filterNode = (value: string, data: Tree): boolean => {
    if (!settingsStore.displayHiddenItems && deviceChannelHidden(data.deviceUID, data.name).value)
        return false
    // todo: also when value is empty - set the expanded nodes to their saved/original values
    if (!value) return true
    // todo: add clause to never filter the static root node names i.e. "Overviews", "Profiles", etc
    return data.label.toLowerCase().includes(value)
}

const expandedNodeIds = (): Array<string> => {
    // todo: save and load initial expanded IDs. (new setting - probably gut the original setting)
    // If no saved IDs (first start)
    return data
        .filter(
            (node: any) =>
                !node.id.startsWith('modes') &&
                !node.id.startsWith('profiles') &&
                !node.id.startsWith('functions'),
        )
        .map((node: any) => node.id)
}

onMounted(async () => {
    applyFilter('') // at startup this filters hidden items out.
})
watch(settingsStore.allUIDeviceSettings, () => {
    applyFilter('') // update filter if hidden sensors change
})
</script>

<template>
    <div class="">
        <IconField>
            <InputIcon>
                <svg-icon
                    class="mt-[-0.75rem] ml-[-0.25rem] text-text-color-secondary"
                    type="mdi"
                    :path="mdiMagnify"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </InputIcon>
            <InputText
                v-model="filterText"
                class="mb-4 bg-bg-one"
                size="small"
                fluid
                placeholder="search"
                @update:modelValue="applyFilter"
            />
        </IconField>
        <el-tree
            ref="treeRef"
            class="w-full"
            :data="data"
            :props="nodeProps"
            :filter-node-method="filterNode"
            highlight-current
            node-key="id"
            empty-text="No Matches"
            :indent="deviceStore.getREMSize(0.5)"
            :default-expanded-keys="expandedNodeIds()"
            @node-collapse=""
            @node-expand=""
        >
            <template #default="{ node, data }">
                <el-dropdown
                    class="ml-0.5 w-full outline-0"
                    :show-after="0"
                    transition="none"
                    hide-after="0"
                    :disabled="data.options == null || data.options.length == 0"
                    placement="top-start"
                    popper-class="ml-[1.7rem] mb-[-1.25rem]"
                >
                    <router-link
                        class="flex h-10 items-center justify-between outline-0"
                        tabindex="0"
                        exact
                        :exact-active-class="'active-link'"
                        :to="
                            !data.to || deviceChannelHidden(data.deviceUID, data.name).value
                                ? ''
                                : data.to
                        "
                    >
                        <div class="flex flex-row items-center min-w-0 w-11/12">
                            <svg-icon
                                v-if="data.icon"
                                class="mr-1.5 min-w-6"
                                :class="{
                                    'disabled-text': deviceChannelHidden(data.deviceUID, data.name)
                                        .value,
                                }"
                                type="mdi"
                                :path="data.icon ?? ''"
                                :style="{
                                    color: deviceChannelColor(data.deviceUID, data.name).value,
                                }"
                                :size="deviceStore.getREMSize(1.5)"
                            />
                            <div
                                class="tree-text"
                                :class="{
                                    'disabled-text': deviceChannelHidden(data.deviceUID, data.name)
                                        .value,
                                }"
                            >
                                {{ node.label }}
                            </div>
                        </div>
                        <div class="">
                            <div v-if="data.isActive">
                                <svg-icon
                                    class="ml-2 mr-2 text-accent"
                                    type="mdi"
                                    :path="mdiLightningBolt"
                                    :size="deviceStore.getREMSize(1.5)"
                                />
                            </div>
                            <div
                                v-else-if="data.temp != null"
                                class="ml-2 items-end tree-data"
                                :class="{
                                    'disabled-text': deviceChannelHidden(data.deviceUID, data.name)
                                        .value,
                                }"
                            >
                                {{ deviceChannelValues(data.deviceUID, data.name)!.temp }}
                                <span>°&nbsp;&nbsp;&nbsp;</span>
                            </div>
                            <div
                                v-else-if="data.freq != null"
                                class="ml-2 items-end tree-data"
                                :class="{
                                    'disabled-text': deviceChannelHidden(data.deviceUID, data.name)
                                        .value,
                                }"
                            >
                                {{ deviceChannelValues(data.deviceUID, data.name)!.freq }}
                                <span style="font-size: 0.62rem">mhz</span>
                            </div>
                            <div
                                v-else-if="data.duty != null && data.rpm == null"
                                class="ml-2 content-end tree-data"
                                :class="{
                                    'disabled-text': deviceChannelHidden(data.deviceUID, data.name)
                                        .value,
                                }"
                            >
                                {{ deviceChannelValues(data.deviceUID, data.name)!.duty }}
                                <span style="font-size: 0.7rem">%&nbsp;&nbsp;&nbsp;</span>
                            </div>
                            <div
                                v-else-if="data.rpm != null && data.duty == null"
                                class="ml-2 items-end flex tree-data"
                                :class="{
                                    'disabled-text': deviceChannelHidden(data.deviceUID, data.name)
                                        .value,
                                }"
                            >
                                {{ deviceChannelValues(data.deviceUID, data.name)!.rpm }}
                                <span style="font-size: 0.7rem">rpm</span>
                            </div>
                            <div
                                v-else-if="data.duty != null && data.rpm != null"
                                class="ml-2 items-end flex flex-col leading-none tree-data"
                                :class="{
                                    'disabled-text': deviceChannelHidden(data.deviceUID, data.name)
                                        .value,
                                }"
                            >
                                <span>
                                    {{ deviceChannelValues(data.deviceUID, data.name)!.duty }}
                                    <span style="font-size: 0.7rem">%&nbsp;&nbsp;&nbsp;</span>
                                </span>
                                <span>
                                    {{ deviceChannelValues(data.deviceUID, data.name)!.rpm }}
                                    <span style="font-size: 0.7rem">rpm</span>
                                </span>
                            </div>
                        </div>
                    </router-link>
                    <template #dropdown>
                        <div
                            class="border-2 border-border-one bg-bg-two rounded-lg flex content-center items-center justify-center p-[1px]"
                        >
                            <div v-for="option in data.options">
                                <menu-hide
                                    v-if="option.hide"
                                    :device-u-i-d="data.deviceUID"
                                    :channel-name="data.name"
                                />
                                <menu-hide-all
                                    v-else-if="option.hideAll"
                                    :device-u-i-d="data.deviceUID"
                                />
                                <menu-rename
                                    v-else-if="option.rename"
                                    :device-u-i-d="data.deviceUID"
                                    :channel-name="data.name"
                                    @name-change="(value: string) => (data.label = value)"
                                />
                                <menu-color
                                    v-else-if="option.color"
                                    :device-u-i-d="data.deviceUID"
                                    :channel-name="data.name"
                                    :color="data.color"
                                    @color-reset="(newColor: Color) => (data.color = newColor)"
                                />
                                <menu-disable
                                    v-else-if="option.disable"
                                    :device-u-i-d="data.deviceUID"
                                />
                            </div>
                        </div>
                    </template>
                </el-dropdown>
            </template>
        </el-tree>
    </div>
</template>

<style scoped lang="scss">
.el-tree {
    --el-fill-color-blank: rgb(var(--colors-bg-one));
    --el-font-size-base: 1rem;
    --el-tree-text-color: rgb(var(--colors-text-color));
    --el-tree-node-content-height: 2.5rem;
    --el-tree-node-hover-bg-color: rgb(var(--colors-bg-two));
    --el-text-color-placeholder: rgb(var(--colors-text-color));
    --el-color-primary-light-9: rgb(var(--colors-bg-two));
}

.tree-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.tree-data {
    white-space: nowrap;
    align-items: end;
    align-content: end;
    //display: inline-block;
    //min-width: 5rem;
}

//.custom-tree-node {
//    flex: 1;
//    display: flex;
//    align-items: center;
//    justify-content: space-between;
//}

.disabled-text {
    opacity: 0.2;
}
</style>
<style lang="scss">
/******************************************************************************************
* Unscoped Style needed to deeply affect the element components
*/
.el-tree-node__content {
    border-radius: 0.5rem;
}
</style>
