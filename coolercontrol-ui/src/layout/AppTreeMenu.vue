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
import {
    mdiBookmarkCheckOutline,
    mdiBookmarkMultipleOutline,
    mdiBookmarkOffOutline,
    mdiBookmarkOutline,
    mdiChartBoxMultipleOutline,
    mdiChartBoxOutline,
    mdiChartLine,
    mdiChartMultiple,
    mdiCircleMultipleOutline,
    mdiFan,
    mdiFlask,
    mdiFlaskOutline,
    mdiLedOn,
    mdiMagnify,
    mdiMemory,
    mdiSineWave,
    mdiSpeedometer,
    mdiTelevisionShimmer,
    mdiThermometer,
} from '@mdi/js'
import { computed, ComputedRef, inject, onMounted, reactive, Reactive, ref, Ref, watch } from 'vue'
import { ElDropdown, ElTree } from 'element-plus'
import 'element-plus/es/components/tree/style/css'
import InputText from 'primevue/inputtext'
import IconField from 'primevue/iconfield'
import InputIcon from 'primevue/inputicon'
import { ChannelValues, useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { Emitter, EventType } from 'mitt'
import { Color, DeviceType, UID } from '@/models/Device.ts'
import MenuRename from '@/components/menu/MenuRename.vue'
import MenuHide from '@/components/menu/MenuHide.vue'
import MenuColor from '@/components/menu/MenuColor.vue'
import MenuHideAll from '@/components/menu/MenuHideAll.vue'
import MenuDisable from '@/components/menu/MenuDisable.vue'
import MenuDeviceInfo from '@/components/menu/MenuDeviceInfo.vue'
import MenuDashboardAdd from '@/components/menu/MenuDashboardAdd.vue'
import MenuDashboardRename from '@/components/menu/MenuDashboardRename.vue'
import MenuDashboardDelete from '@/components/menu/MenuDashboardDelete.vue'
import MenuCustomSensorDelete from '@/components/menu/MenuCustomSensorDelete.vue'
import MenuCustomSensorAdd from '@/components/menu/MenuCustomSensorAdd.vue'
import { useRoute, useRouter } from 'vue-router'
import MenuFunctionRename from '@/components/menu/MenuFunctionRename.vue'
import MenuFunctionDelete from '@/components/menu/MenuFunctionDelete.vue'
import MenuFunctionAdd from '@/components/menu/MenuFunctionAdd.vue'
import MenuFunctionDuplicate from '@/components/menu/MenuFunctionDuplicate.vue'
import MenuDashboardDuplicate from '@/components/menu/MenuDashboardDuplicate.vue'
import MenuProfileDelete from '@/components/menu/MenuProfileDelete.vue'
import MenuProfileRename from '@/components/menu/MenuProfileRename.vue'
import MenuProfileDuplicate from '@/components/menu/MenuProfileDuplicate.vue'
import MenuProfileAdd from '@/components/menu/MenuProfileAdd.vue'
import MenuModeAdd from '@/components/menu/MenuModeAdd.vue'
import MenuModeRename from '@/components/menu/MenuModeRename.vue'
import MenuModeDelete from '@/components/menu/MenuModeDelete.vue'
import MenuModeDuplicate from '@/components/menu/MenuModeDuplicate.vue'
import MenuModeActivate from '@/components/menu/MenuModeActivate.vue'
import MenuModeUpdate from '@/components/menu/MenuModeUpdate.vue'
import { TreeNodeData } from 'element-plus/es/components/tree-v2/src/types'
import MenuModeInfo from '@/components/menu/MenuModeInfo.vue'
import MenuProfileInfo from '@/components/menu/MenuProfileInfo.vue'
import MenuDashboardInfo from '@/components/menu/MenuDashboardInfo.vue'
import MenuFunctionInfo from '@/components/menu/MenuFunctionInfo.vue'
import MenuCustomSensorInfo from '@/components/menu/MenuCustomSensorInfo.vue'

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
const router = useRouter()
const route = useRoute()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!

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
    if (
        deviceUID == null ||
        deviceUID.startsWith('Dashboards') ||
        deviceUID.startsWith('Modes') ||
        deviceUID.startsWith('Profiles') ||
        deviceUID.startsWith('Functions')
    ) {
        color.value = ''
    } else {
        color.value =
            settingsStore.allUIDeviceSettings.get(deviceUID)?.sensorsAndChannels.get(channelName)
                ?.color ?? ''
    }
    return color
}

const deviceChannelIconSize = (deviceUID: UID): number => {
    if (deviceUID == null) {
        return 1.5
    } else if (
        deviceUID.startsWith('Dashboards') ||
        deviceUID.startsWith('Modes') ||
        deviceUID.startsWith('Profiles') ||
        deviceUID.startsWith('Functions')
    ) {
        return 1.0
    } else {
        return 1.5
    }
}

const filterText: Ref<string> = ref('')
const treeRef = ref<InstanceType<typeof ElTree>>()
const nodeProps = {
    children: 'children',
    label: 'label',
}
const data: Reactive<Tree[]> = reactive([])
const createTreeMenu = (): void => {
    data.length = 0
    data.push(dashboardsTree())
    data.push(modesTree())
    data.push(profilesTree())
    data.push(functionsTree())
    data.push(customSensorsTree())
    data.push(...devicesTreeArray())
    // data.unshift(pinnedTree(data)) // needs to be done at the end
}
// const pinnedTree = (data: Reactive<Tree[]>): any => {
//     // todo: only add pinned node if there are pins
//     // todo: perhaps the children should be added after the tree is created and "we link to the already created child ID"? (copy-ish)
//
//     // todo: pull saved "pinned" node IDs from settingsStore
//     // todo: copy those nodes from the data array and add them to the pinned tree
//     return {
//         id: 'pinned',
//         label: 'Pinned',
//         icon: mdiPinOutline,
//         name: null, // devices should not have names
//         options: [],
//     }
// }
const dashboardsTree = (): any => {
    return {
        id: 'dashboards',
        label: 'Dashboards',
        icon: mdiChartBoxMultipleOutline,
        name: null, // devices should not have names
        options: [{ dashboardInfo: true }, { dashboardAdd: true }],
        children: settingsStore.dashboards.map((dashboard) => {
            return {
                id: dashboard.uid,
                label: dashboard.name,
                icon: mdiChartBoxOutline,
                deviceUID: 'Dashboards',
                dashboardUID: dashboard.uid,
                name: dashboard.uid,
                to: { name: 'dashboards', params: { dashboardUID: dashboard.uid } },
                options: [
                    { dashboardRename: true },
                    { dashboardDuplicate: true },
                    { dashboardDelete: true },
                ],
            }
        }),
    }
}
const modesTree = (): any => {
    return {
        id: 'modes',
        label: 'Modes',
        icon: mdiBookmarkMultipleOutline,
        name: null, // devices should not have names
        options: [{ modeInfo: true }, { modeAdd: true }],
        children: settingsStore.modes.map((mode) => {
            const isActive: boolean = settingsStore.modesActive.includes(mode.uid)
            const isRecentlyActive: boolean = settingsStore.modesActiveLast.includes(mode.uid)
            return {
                id: `modes_${mode.uid}`,
                label: mode.name,
                icon: isActive
                    ? mdiBookmarkCheckOutline
                    : isRecentlyActive
                      ? mdiBookmarkOffOutline
                      : mdiBookmarkOutline,
                deviceUID: 'Modes',
                uid: mode.uid,
                isActive: isActive,
                isRecentlyActive: isRecentlyActive,
                to: { name: 'modes', params: { modeUID: mode.uid } },
                options: [
                    { modeActivate: true },
                    { modeRename: true },
                    { modeUpdate: true },
                    { modeDuplicate: true },
                    { modeDelete: true },
                ],
            }
        }),
    }
}

const profilesTree = (): any => {
    return {
        id: 'profiles',
        label: 'Profiles',
        name: null, // devices should not have names
        icon: mdiChartMultiple,
        options: [{ profileInfo: true }, { profileAdd: true }],
        children: settingsStore.profiles
            .filter((profile) => profile.uid !== '0') // Default Profile
            .map((profile) => {
                return {
                    id: `profiles_${profile.uid}`,
                    label: profile.name,
                    icon: mdiChartLine,
                    deviceUID: 'Profiles',
                    uid: profile.uid,
                    to: { name: 'profiles', params: { profileUID: profile.uid } },
                    options: [
                        { profileRename: true },
                        { profileDuplicate: true },
                        { profileDelete: true },
                    ],
                }
            }),
    }
}

const functionsTree = (): any => {
    return {
        id: 'functions',
        label: 'Functions',
        icon: mdiFlaskOutline,
        name: null, // devices should not have names
        options: [{ functionInfo: true }, { functionAdd: true }],
        children: settingsStore.functions
            .filter((fun) => fun.uid !== '0') // Default Function
            .map((fun) => {
                return {
                    id: `functions_${fun.uid}`,
                    label: fun.name,
                    icon: mdiFlask,
                    deviceUID: 'Functions',
                    uid: fun.uid,
                    to: { name: 'functions', params: { functionUID: fun.uid } },
                    options: [
                        { functionRename: true },
                        { functionDuplicate: true },
                        { functionDelete: true },
                    ],
                }
            }),
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
                id: `custom-sensors_${temp.name}`,
                label: deviceSettings.sensorsAndChannels.get(temp.name)!.name,
                name: temp.name,
                hasColor: true,
                color: deviceChannelColor(device.uid, temp.name),
                icon: mdiThermometer,
                to: { name: 'custom-sensors', params: { customSensorID: temp.name } },
                deviceUID: device.uid,
                temp: temp.temp.toFixed(1),
                options: [{ rename: true }, { color: true }, { customSensorDelete: true }],
            })
        }
        return {
            id: 'custom-sensors',
            label: 'Custom Sensors',
            icon: mdiCircleMultipleOutline,
            name: null, // devices should not have names
            deviceUID: deviceUID,
            options: [{ customSensorInfo: true }, { customSensorAdd: true }],
            children: sensorsChildren,
        }
    }
}
const aSubMenuIsOpen: Ref<boolean> = ref(false)
const subMenuStatusChange = (isOpen: boolean, data: any): void => {
    aSubMenuIsOpen.value = isOpen
    if (!isOpen) data.dropdownRef.handleClose()
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
            options: [{ deviceInfo: true }, { rename: true }, { hideAll: true }, { disable: true }],
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
                to: {
                    name: 'single-dashboard',
                    params: { deviceUID: device.uid, channelName: temp.name },
                },
                deviceUID: device.uid,
                temp: temp.temp.toFixed(1),
                options: [{ rename: true }, { color: true }, { hide: true }],
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
                        name: 'single-dashboard',
                        params: { deviceUID: device.uid, channelName: channel.name },
                    },
                    deviceUID: device.uid,
                    freq: channel.freq,
                    options: [{ rename: true }, { color: true }, { hide: true }],
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
                        name: 'single-dashboard',
                        params: { deviceUID: device.uid, channelName: channel.name },
                    },
                    deviceUID: device.uid,
                    duty: channel.duty,
                    rpm: channel.rpm,
                    options: [{ rename: true }, { color: true }, { hide: true }],
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
                        params: { deviceUID: device.uid, channelName: channelName },
                    },
                    deviceUID: device.uid,
                    duty: duty,
                    rpm: rpm,
                    options: [{ rename: true }, { color: true }, { hide: true }],
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
                    options: [{ rename: true }, { hide: true }],
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
                    options: [{ rename: true }, { hide: true }],
                })
            }
        }
        allDevices.push(deviceItem)
    }
    return allDevices
}

createTreeMenu()

const formatFrequency = (value: string): string =>
    settingsStore.frequencyPrecision === 1
        ? value
        : (Number(value) / settingsStore.frequencyPrecision).toFixed(2)

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
const addDashbaord = (dashboardUID: UID) => {
    const newDashboard = settingsStore.dashboards.find(
        (dashboard) => dashboard.uid === dashboardUID,
    )!
    treeRef.value!.append(
        {
            id: dashboardUID,
            label: newDashboard.name,
            icon: mdiChartBoxOutline,
            deviceUID: 'Dashboards',
            dashboardUID: dashboardUID,
            name: dashboardUID,
            to: { name: 'dashboards', params: { dashboardUID: dashboardUID } },
            options: [
                { dashboardRename: true },
                { dashboardDuplicate: true },
                { dashboardDelete: true },
            ],
        },
        'dashboards',
    )
}
const deleteDashboard = (dashboardUID: UID): void => {
    if (route.params != null && route.params.dashboardUID === dashboardUID) {
        router.push({ name: 'system-overview' })
    }
    treeRef.value!.remove(treeRef.value!.getNode(dashboardUID))
}

const activeModesChange = (_: UID): void => {
    treeRef
        .value!.getNode('modes')
        .getChildren()
        .forEach((data: TreeNodeData) => {
            const isActive: boolean = settingsStore.modesActive.includes(data.uid)
            const isRecentlyActive: boolean = settingsStore.modesActiveLast.includes(data.uid)
            data.icon = isActive
                ? mdiBookmarkCheckOutline
                : isRecentlyActive
                  ? mdiBookmarkOffOutline
                  : mdiBookmarkOutline
            data.isActive = isActive
            data.isRecentlyActive = isRecentlyActive
        })
    router.push({ name: 'system-overview' })
}
emitter.on('active-modes-change-menu', activeModesChange)
const addMode = (modeUID: UID): void => {
    const newMode = settingsStore.modes.find((mode) => mode.uid === modeUID)!
    const isActive: boolean = settingsStore.modesActive.includes(newMode.uid)
    const isRecentlyActive: boolean = settingsStore.modesActiveLast.includes(newMode.uid)
    treeRef.value!.append(
        {
            id: `modes_${newMode.uid}`,
            label: newMode.name,
            icon: isActive
                ? mdiBookmarkCheckOutline
                : isRecentlyActive
                  ? mdiBookmarkOffOutline
                  : mdiBookmarkOutline,
            deviceUID: 'Modes',
            uid: newMode.uid,
            isActive: isActive,
            isRecentlyActive: isRecentlyActive,
            to: { name: 'modes', params: { modeUID: newMode.uid } },
            options: [
                { modeActivate: true },
                { modeRename: true },
                { modeUpdate: true },
                { modeDuplicate: true },
                { modeDelete: true },
            ],
        },
        'modes',
    )
}
const deleteMode = (modeUID: UID): void => {
    if (route.params != null && route.params.modeUID === modeUID) {
        router.push({ name: 'system-overview' })
    }
    treeRef.value!.remove(treeRef.value!.getNode(`modes_${modeUID}`))
}

const addProfile = (profileUID: UID): void => {
    const newProfile = settingsStore.profiles.find((profile) => profile.uid === profileUID)!
    treeRef.value!.append(
        {
            id: `profiles_${newProfile.uid}`,
            label: newProfile.name,
            icon: mdiChartLine,
            deviceUID: 'Profiles',
            uid: newProfile.uid,
            to: { name: 'profiles', params: { profileUID: newProfile.uid } },
            options: [{ profileRename: true }, { profileDuplicate: true }, { profileDelete: true }],
        },
        'profiles',
    )
}
const deleteProfile = (profileUID: UID): void => {
    if (route.params != null && route.params.profileUID === profileUID) {
        router.push({ name: 'system-overview' })
    }
    treeRef.value!.remove(treeRef.value!.getNode(`profiles_${profileUID}`))
}

const addFunction = (functionUID: UID): void => {
    const newFunction = settingsStore.functions.find((fun) => fun.uid === functionUID)!
    treeRef.value!.append(
        {
            id: `functions_${newFunction.uid}`,
            label: newFunction.name,
            icon: mdiFlask,
            deviceUID: 'Functions',
            uid: newFunction.uid,
            to: { name: 'functions', params: { functionUID: newFunction.uid } },
            options: [
                { functionRename: true },
                { functionDuplicate: true },
                { functionDelete: true },
            ],
        },
        'functions',
    )
}
const deleteFunction = (functionUID: UID): void => {
    if (route.params != null && route.params.functionUID === functionUID) {
        router.push({ name: 'system-overview' })
    }
    treeRef.value!.remove(treeRef.value!.getNode(`functions_${functionUID}`))
}

onMounted(async () => {
    applyFilter('') // at startup this filters hidden items out.
})
watch(settingsStore.allUIDeviceSettings, () => {
    applyFilter('') // update filter if hidden sensors change
})
watch(
    () => settingsStore.displayHiddenItems,
    () => {
        applyFilter('') // update filter if show/hide settings changes
    },
)
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
                class="mb-4 bg-bg-one !border-border-one"
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
            node-key="id"
            empty-text="No Matches"
            :indent="deviceStore.getREMSize(0.5)"
            :default-expanded-keys="expandedNodeIds()"
            :render-after-expand="false"
            @node-collapse=""
            @node-expand=""
        >
            <template #default="{ node, data }">
                <el-dropdown
                    :ref="(el) => (data.dropdownRef = el)"
                    class="ml-0.5 w-full outline-none"
                    :show-timeout="100"
                    :hide-timeout="50"
                    :disabled="data.options == null || data.options.length == 0"
                    placement="top-end"
                    popper-class="mr-[0.2rem] mb-[-1.9rem]"
                    :teleported="true"
                    :hide-on-click="false"
                    :trigger="aSubMenuIsOpen ? 'click' : 'hover'"
                >
                    <!--This options with so many dropdowns causes a strange issue when scrolling-->
                    <!--down a large list of sensors-->
                    <!--:popper-options="{-->
                    <!--modifiers: [{ name: 'computeStyles', options: { gpuAcceleration: true } }],-->
                    <!--}"-->
                    <router-link
                        class="flex h-10 items-center justify-between outline-none"
                        tabindex="0"
                        exact
                        :exact-active-class="'active-link'"
                        :to="
                            !data.to || deviceChannelHidden(data.deviceUID, data.name).value
                                ? ''
                                : data.to
                        "
                    >
                        <div class="flex flex-row items-center min-w-0">
                            <svg-icon
                                v-if="data.icon"
                                class="mr-1.5 min-w-6"
                                :class="{
                                    'disabled-text': deviceChannelHidden(data.deviceUID, data.name)
                                        .value,
                                    'text-accent': data.isActive,
                                }"
                                type="mdi"
                                :path="data.icon ?? ''"
                                :style="{
                                    color: deviceChannelColor(data.deviceUID, data.name).value,
                                }"
                                :size="
                                    deviceStore.getREMSize(deviceChannelIconSize(data.deviceUID))
                                "
                            />
                            <div
                                class="tree-text"
                                :class="{
                                    'disabled-text': deviceChannelHidden(data.deviceUID, data.name)
                                        .value,
                                    'text-accent': data.isActive,
                                }"
                            >
                                {{ node.label }}
                            </div>
                        </div>
                        <div class="ml-2">
                            <div
                                v-if="data.temp != null"
                                class="items-end tree-data"
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
                                class="items-end tree-data"
                                :class="{
                                    'disabled-text': deviceChannelHidden(data.deviceUID, data.name)
                                        .value,
                                }"
                            >
                                {{
                                    formatFrequency(
                                        deviceChannelValues(data.deviceUID, data.name)!.freq!,
                                    )
                                }}
                                <span style="font-size: 0.62rem">
                                    {{ settingsStore.frequencyPrecision === 1 ? 'Mhz' : 'Ghz' }}
                                </span>
                            </div>
                            <div
                                v-else-if="data.duty != null && data.rpm == null"
                                class="content-end tree-data"
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
                                class="items-end flex tree-data"
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
                                class="items-end flex flex-col leading-none tree-data"
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
                            class="border-2 border-border-one/95 bg-bg-two/95 rounded-lg flex content-center items-center justify-center p-[1px]"
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
                                    @open="(isOpen) => subMenuStatusChange(isOpen, data)"
                                />
                                <menu-color
                                    v-else-if="option.color"
                                    :device-u-i-d="data.deviceUID"
                                    :channel-name="data.name"
                                    :color="data.color"
                                    @color-reset="(newColor: Color) => (data.color = newColor)"
                                    @open="(isOpen) => subMenuStatusChange(isOpen, data)"
                                />
                                <menu-disable
                                    v-else-if="option.disable"
                                    :device-u-i-d="data.deviceUID"
                                />
                                <menu-device-info
                                    v-else-if="option.deviceInfo"
                                    :device-u-i-d="data.deviceUID"
                                    @open="(isOpen) => subMenuStatusChange(isOpen, data)"
                                />
                                <menu-dashboard-info v-else-if="option.dashboardInfo" />
                                <menu-dashboard-add
                                    v-else-if="option.dashboardAdd"
                                    @added="addDashbaord"
                                />
                                <menu-dashboard-rename
                                    v-else-if="option.dashboardRename"
                                    :dashboard-u-i-d="data.dashboardUID"
                                    @name-change="(name: string) => (data.label = name)"
                                />
                                <menu-dashboard-duplicate
                                    v-else-if="option.dashboardDuplicate"
                                    :dashboard-u-i-d="data.dashboardUID"
                                    @added="addDashbaord"
                                />
                                <menu-dashboard-delete
                                    v-else-if="option.dashboardDelete"
                                    :dashboard-u-i-d="data.dashboardUID"
                                    @deleted="deleteDashboard"
                                />
                                <menu-mode-activate
                                    v-else-if="option.modeActivate"
                                    :mode-u-i-d="data.uid"
                                    @activated="activeModesChange"
                                />
                                <menu-mode-info v-else-if="option.modeInfo" />
                                <menu-mode-add v-else-if="option.modeAdd" @added="addMode" />
                                <menu-mode-update
                                    v-else-if="option.modeUpdate"
                                    :mode-u-i-d="data.uid"
                                    @updated="activeModesChange"
                                />
                                <menu-mode-duplicate
                                    v-else-if="option.modeDuplicate"
                                    :mode-u-i-d="data.uid"
                                    @added="addMode"
                                />
                                <menu-mode-rename
                                    v-else-if="option.modeRename"
                                    :mode-u-i-d="data.uid"
                                    @name-change="(name: string) => (data.label = name)"
                                    @open="(isOpen) => subMenuStatusChange(isOpen, data)"
                                />
                                <menu-mode-delete
                                    v-else-if="option.modeDelete"
                                    :mode-u-i-d="data.uid"
                                    @deleted="deleteMode"
                                />
                                <menu-profile-info v-else-if="option.profileInfo" />
                                <menu-profile-add
                                    v-else-if="option.profileAdd"
                                    @added="addProfile"
                                />
                                <menu-profile-duplicate
                                    v-else-if="option.profileDuplicate"
                                    :profile-u-i-d="data.uid"
                                    @added="addProfile"
                                />
                                <menu-profile-rename
                                    v-else-if="option.profileRename"
                                    :profile-u-i-d="data.uid"
                                    @name-change="(name: string) => (data.label = name)"
                                    @open="(isOpen) => subMenuStatusChange(isOpen, data)"
                                />
                                <menu-profile-delete
                                    v-else-if="option.profileDelete"
                                    :profile-u-i-d="data.uid"
                                    @deleted="deleteProfile"
                                />
                                <menu-function-info v-else-if="option.functionInfo" />
                                <menu-function-add
                                    v-else-if="option.functionAdd"
                                    @added="addFunction"
                                />
                                <menu-function-duplicate
                                    v-else-if="option.functionDuplicate"
                                    :function-u-i-d="data.uid"
                                    @added="addFunction"
                                />
                                <menu-function-rename
                                    v-else-if="option.functionRename"
                                    :function-u-i-d="data.uid"
                                    @name-change="(name: string) => (data.label = name)"
                                    @open="(isOpen) => subMenuStatusChange(isOpen, data)"
                                />
                                <menu-function-delete
                                    v-else-if="option.functionDelete"
                                    :function-u-i-d="data.uid"
                                    @deleted="deleteFunction"
                                />
                                <menu-custom-sensor-info v-else-if="option.customSensorInfo" />
                                <menu-custom-sensor-add v-else-if="option.customSensorAdd" />
                                <menu-custom-sensor-delete
                                    v-else-if="option.customSensorDelete"
                                    :device-u-i-d="data.deviceUID"
                                    :custom-sensor-i-d="data.name"
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

.el-zoom-in-top-enter-action,
.el-zoom-in-top-enter-to {
    transition-duration: 0ms;
    transition-delay: 0;
}
.el-zoom-in-top-leave-action,
.el-zoom-in-top-leave-to {
    transition-duration: 0ms;
    transition-delay: 0;
}
</style>
