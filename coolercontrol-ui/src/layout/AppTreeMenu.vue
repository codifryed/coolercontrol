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
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import {
    mdiBellCircleOutline,
    mdiBellOutline,
    mdiBellRingOutline,
    mdiBookmarkCheckOutline,
    mdiBookmarkMultipleOutline,
    mdiBookmarkOffOutline,
    mdiBookmarkOutline,
    mdiChartBoxMultipleOutline,
    mdiChartBoxOutline,
    mdiChartLine,
    mdiChartMultiple,
    mdiCircleMultipleOutline,
    mdiDotsVertical,
    mdiFan,
    mdiFlask,
    mdiFlaskOutline,
    mdiHomeAnalytics,
    mdiLedOn,
    mdiLightningBoltCircle,
    mdiMemory,
    mdiPinOutline,
    mdiSineWave,
    mdiSpeedometer,
    mdiTelevisionShimmer,
    mdiThermometer,
} from '@mdi/js'
import { computed, inject, onMounted, onUnmounted, reactive, Reactive, ref, Ref, watch } from 'vue'
import { ElDropdown, ElTree } from 'element-plus'
import 'element-plus/es/components/tree/style/css'
import Popover from 'primevue/popover'
import { ChannelValues, useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { Emitter, EventType } from 'mitt'
import { Color, DeviceType, UID } from '@/models/Device.ts'
import MenuRename from '@/components/menu/MenuRename.vue'
import MenuColor from '@/components/menu/MenuColor.vue'
import MenuDeviceInfo from '@/components/menu/MenuDeviceInfo.vue'
import MenuDashboardAdd from '@/components/menu/MenuDashboardAdd.vue'
import MenuDashboardRename from '@/components/menu/MenuDashboardRename.vue'
import SubSubMenuCustomSensorDelete from '@/components/menu/SubMenuCustomSensorDelete.vue'
import SubMenuDashboardDelete from '@/components/menu/SubMenuDashboardDelete.vue'
import MenuCustomSensorAdd from '@/components/menu/MenuCustomSensorAdd.vue'
import { useRoute, useRouter } from 'vue-router'
import MenuFunctionRename from '@/components/menu/MenuFunctionRename.vue'
import MenuFunctionDelete from '@/components/menu/MenuFunctionDelete.vue'
import MenuFunctionAdd from '@/components/menu/MenuFunctionAdd.vue'
import MenuFunctionDuplicate from '@/components/menu/MenuFunctionDuplicate.vue'
import SubMenuDashboardDuplicate from '@/components/menu/SubMenuDashboardDuplicate.vue'
import MenuProfileDelete from '@/components/menu/MenuProfileDelete.vue'
import MenuProfileRename from '@/components/menu/MenuProfileRename.vue'
import MenuProfileDuplicate from '@/components/menu/MenuProfileDuplicate.vue'
import MenuProfileAdd from '@/components/menu/MenuProfileAdd.vue'
import MenuModeAdd from '@/components/menu/MenuModeAdd.vue'
import MenuModeRename from '@/components/menu/MenuModeRename.vue'
import MenuModeDelete from '@/components/menu/MenuModeDelete.vue'
import MenuModeDuplicate from '@/components/menu/MenuModeDuplicate.vue'
import MenuModeUpdate from '@/components/menu/MenuModeUpdate.vue'
import { TreeNodeData } from 'element-plus/es/components/tree-v2/src/types'
import MenuModeInfo from '@/components/menu/MenuModeInfo.vue'
import MenuProfileInfo from '@/components/menu/MenuProfileInfo.vue'
import MenuDashboardInfo from '@/components/menu/MenuDashboardInfo.vue'
import MenuFunctionInfo from '@/components/menu/MenuFunctionInfo.vue'
import MenuCustomSensorInfo from '@/components/menu/MenuCustomSensorInfo.vue'
import TreeIcon from '@/components/TreeIcon.vue'
import { AlertState } from '@/models/Alert.ts'
import MenuAlertInfo from '@/components/menu/MenuAlertInfo.vue'
import MenuAlertRename from '@/components/menu/MenuAlertRename.vue'
import MenuAlertAdd from '@/components/menu/MenuAlertAdd.vue'
import MenuAlertDelete from '@/components/menu/MenuAlertDelete.vue'
import MenuDashboardHome from '@/components/menu/MenuDashboardHome.vue'
import MenuControlView from '@/components/menu/MenuControlView.vue'
import { VueDraggable } from 'vue-draggable-plus'
import { useI18n } from 'vue-i18n'
import SubSubMenuMoveTop from '@/components/menu/SubMenuMoveTop.vue'
import SubSubMenuMoveBottom from '@/components/menu/SubMenuMoveBottom.vue'
import SubSubMenuDisable from '@/components/menu/SubMenuDisable.vue'

// interface Tree {
//     label: string
//     children?: Tree[]
// }
interface Tree {
    // necessary for test filter
    [key: string]: any
}

interface PinnedItems {
    // rootId: string
    id: string
    ref: any
}

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const router = useRouter()
const route = useRoute()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!
const { t } = useI18n()

const deviceChannelValues = (deviceUID: UID, channelName: string): ChannelValues | undefined =>
    deviceStore.currentDeviceStatus.get(deviceUID)?.get(channelName)
const deviceChannelColor = (deviceUID: UID | undefined, channelName: string): Ref<Color> => {
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

const deviceChannelIconSize = (deviceUID: UID | undefined, name: string | undefined): number => {
    if (deviceUID == null) {
        // Group root like Dashboards, Modes, etc
        return 1.75
    } else if (
        // group items
        deviceUID.startsWith('Dashboards') ||
        deviceUID.startsWith('Modes') ||
        deviceUID.startsWith('Profiles') ||
        deviceUID.startsWith('Functions')
    ) {
        return 1.0
    } else if (deviceUID && !name) {
        // Device roots
        return 1.75
    } else {
        // channels, etc.
        return 1.5
    }
}

// const filterText: Ref<string> = ref('')
const treeRef = ref<InstanceType<typeof ElTree>>()
const speedControlMenuClass = ({ isControllable }: TreeNodeData) =>
    isControllable ? 'speed-control-menu' : ''
const nodeProps = {
    children: 'children',
    label: 'label',
    class: speedControlMenuClass,
}
const data: Ref<Tree[]> = ref([])

// Remove computed wrapper for menu data
const pinnedItems: Ref<Array<PinnedItems>> = ref([])
const pinItem = (item: any) => {
    pinnedItems.value.push({
        id: item.id,
        ref: item,
    })
}
const isPinned = (item: any): boolean => {
    return pinnedItems.value.some((pinnedItem) => pinnedItem.id === item.id)
}
const unPinItem = (item: any) => {
    pinnedItems.value = pinnedItems.value.filter((pinnedItem) => pinnedItem.id !== item.id)
}

enum Menu {
    COLOR,
    RENAME,
    DEVICE_INFO,
    CUSTOM_SENSOR_INFO,
    CUSTOM_SENSOR_ADD,
    DASHBOARD_INFO,
    DASHBOARD_ADD,
    DASHBOARD_HOME,
    DASHBOARD_RENAME,
    MODE_INFO,
    MODE_ADD,
    PROFILE_INFO,
    PROFILE_ADD,
    FUNCTION_INFO,
    FUNCTION_ADD,
    ALERT_INFO,
    ALERT_ADD,
    IS_CONTROLLABLE,
}
enum SubMenu {
    MOVE_TOP,
    PIN,
    DISABLE,
    CUSTOM_SENSOR_DELETE,
    DASHBOARD_DUPLICATE,
    DASHBOARD_DELETE,
    MOVE_BOTTOM,
}

const createTreeMenu = (): void => {
    data.value.length = 0
    const result: Tree[] = []
    if (settingsStore.menuEntitiesAtBottom) {
        result.push(customSensorsTree())
        result.push(...devicesTreeArray())
        result.push(dashboardsTree())
        result.push(modesTree())
        result.push(profilesTree())
        result.push(functionsTree())
        result.push(alertsTree())
    } else {
        result.push(dashboardsTree())
        result.push(modesTree())
        result.push(profilesTree())
        result.push(functionsTree())
        result.push(alertsTree())
        result.push(customSensorsTree())
        result.push(...devicesTreeArray())
    }
    data.value.push(...result)
}

const dashboardsTree = (): any => {
    return {
        id: 'dashboards',
        label: t('layout.menu.dashboards'),
        icon: mdiChartBoxMultipleOutline,
        name: null, // devices should not have names
        menus: [Menu.DASHBOARD_INFO, Menu.DASHBOARD_ADD],
        subMenus: [SubMenu.MOVE_TOP, SubMenu.MOVE_BOTTOM],
        children: settingsStore.dashboards.map((dashboard) => {
            return {
                id: dashboard.uid,
                label: dashboard.name,
                icon:
                    dashboard.uid === settingsStore.homeDashboard
                        ? mdiHomeAnalytics
                        : mdiChartBoxOutline,
                deviceUID: 'Dashboards',
                dashboardUID: dashboard.uid,
                name: dashboard.uid,
                to: { name: 'dashboards', params: { dashboardUID: dashboard.uid } },
                menus: [Menu.DASHBOARD_HOME, Menu.DASHBOARD_RENAME],
                subMenus: [
                    SubMenu.MOVE_TOP,
                    SubMenu.PIN,
                    SubMenu.DASHBOARD_DUPLICATE,
                    SubMenu.DASHBOARD_DELETE,
                    SubMenu.MOVE_BOTTOM,
                ],
            }
        }),
    }
}
const modesTree = (): any => {
    return {
        id: 'modes',
        label: t('layout.menu.modes'),
        icon: mdiBookmarkMultipleOutline,
        name: null, // devices should not have names
        menus: [Menu.MODE_INFO, Menu.MODE_ADD],
        submenus: [SubMenu.MOVE_TOP, SubMenu.MOVE_BOTTOM],
        children: settingsStore.modes.map((mode) => {
            const isActive: boolean = settingsStore.modeActiveCurrent === mode.uid
            const isRecentlyActive: boolean = settingsStore.modeActivePrevious === mode.uid
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
        label: t('layout.menu.profiles'),
        name: null, // devices should not have names
        icon: mdiChartMultiple,
        menus: [Menu.PROFILE_INFO, Menu.PROFILE_ADD],
        subMenus: [SubMenu.MOVE_TOP, SubMenu.MOVE_BOTTOM],
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
        label: t('layout.menu.functions'),
        icon: mdiFlaskOutline,
        name: null, // devices should not have names
        menus: [Menu.FUNCTION_INFO, Menu.FUNCTION_ADD],
        subMenus: [SubMenu.MOVE_TOP, SubMenu.MOVE_BOTTOM],
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

const alertsTree = (): any => {
    return {
        id: 'alerts',
        label: t('layout.menu.alerts'),
        name: null, // devices should not have names
        icon: mdiBellCircleOutline,
        menus: [Menu.ALERT_INFO, Menu.ALERT_ADD],
        subMenus: [SubMenu.MOVE_TOP, SubMenu.MOVE_BOTTOM],
        children: settingsStore.alerts.map((alert) => {
            const isActive: boolean = settingsStore.alertsActive.includes(alert.uid)
            return {
                id: `alerts_${alert.uid}`,
                label: alert.name,
                icon: isActive ? mdiBellRingOutline : mdiBellOutline,
                deviceUID: 'Alerts',
                uid: alert.uid,
                alertIsActive: isActive,
                to: { name: 'alerts', params: { alertUID: alert.uid } },
                options: [{ alertRename: true }, { alertDelete: true }],
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
                menus: [Menu.COLOR, Menu.RENAME],
                subMenus: [SubMenu.MOVE_TOP, SubMenu.CUSTOM_SENSOR_DELETE, SubMenu.MOVE_BOTTOM],
            })
        }
        return {
            id: 'custom-sensors',
            label: t('layout.menu.customSensors'),
            icon: mdiCircleMultipleOutline,
            name: null, // devices should not have names
            deviceUID: deviceUID,
            menus: [Menu.CUSTOM_SENSOR_INFO, Menu.CUSTOM_SENSOR_ADD],
            subMenus: [SubMenu.MOVE_TOP, SubMenu.MOVE_BOTTOM],
            children: sensorsChildren,
        }
    }
}

const hoverMenusAreClosed: Ref<boolean> = ref(true)
const setHoverMenuStatus = (isOpen: boolean): void => {
    hoverMenusAreClosed.value = !isOpen
    const elements = document.querySelectorAll('.el-collapse-item__header')
    for (const element of elements) {
        if (isOpen) {
            // This overrides the hover:bg-bg-two style set in the CSS style
            // The reason for this is to avoid a flicker effect that happens when we add or remove
            // css classes, which is due to the EL component also changing the classes (is-active).
            element.setAttribute(
                'style',
                'background-color: rgb(var(--colors-bg-one)); cursor: default;',
            )
        } else {
            element.removeAttribute('style')
        }
    }
}

const aSubMenuIsOpen: Ref<boolean> = ref(false)
const subMenuStatusChange = (isOpen: boolean, _data: any): void => {
    aSubMenuIsOpen.value = isOpen
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
            menus: [Menu.DEVICE_INFO, Menu.RENAME],
            subMenus: [SubMenu.MOVE_TOP, SubMenu.DISABLE, SubMenu.MOVE_BOTTOM],
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
                menus: [Menu.COLOR, Menu.RENAME],
                subMenus: [SubMenu.MOVE_TOP, SubMenu.PIN, SubMenu.DISABLE, SubMenu.MOVE_BOTTOM],
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
                    menus: [Menu.COLOR, Menu.RENAME],
                    subMenus: [SubMenu.MOVE_TOP, SubMenu.PIN, SubMenu.DISABLE, SubMenu.MOVE_BOTTOM],
                })
            }
        }
        for (const channel of device.status.channels) {
            if (channel.name.toLowerCase().includes('power')) {
                // @ts-ignore
                deviceItem.children.push({
                    id: `${device.uid}_${channel.name}`,
                    label: deviceSettings.sensorsAndChannels.get(channel.name)!.name,
                    name: channel.name,
                    hasColor: true,
                    color: deviceChannelColor(device.uid, channel.name),
                    icon: mdiLightningBoltCircle,
                    to: {
                        name: 'single-dashboard',
                        params: { deviceUID: device.uid, channelName: channel.name },
                    },
                    deviceUID: device.uid,
                    watts: channel.watts,
                    menus: [Menu.COLOR, Menu.RENAME],
                    subMenus: [SubMenu.MOVE_TOP, SubMenu.PIN, SubMenu.DISABLE, SubMenu.MOVE_BOTTOM],
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
                    menus: [Menu.COLOR, Menu.RENAME],
                    subMenus: [SubMenu.MOVE_TOP, SubMenu.PIN, SubMenu.DISABLE, SubMenu.MOVE_BOTTOM],
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
                const isControllable: boolean = channelInfo.speed_options?.fixed_enabled ?? false
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
                    isControllable: isControllable,
                    menus: [Menu.COLOR, Menu.RENAME, Menu.IS_CONTROLLABLE],
                    subMenus: [SubMenu.MOVE_TOP, SubMenu.PIN, SubMenu.DISABLE, SubMenu.MOVE_BOTTOM],
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
                        params: { deviceId: device.uid, channelName: channelName },
                    },
                    deviceUID: device.uid,
                    menus: [Menu.RENAME],
                    subMenus: [SubMenu.MOVE_TOP, SubMenu.PIN, SubMenu.DISABLE, SubMenu.MOVE_BOTTOM],
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
                    to: {
                        name: 'device-lcd',
                        params: { deviceId: device.uid, channelName: channelName },
                    },
                    deviceUID: device.uid,
                    menus: [Menu.RENAME],
                    subMenus: [SubMenu.MOVE_TOP, SubMenu.PIN, SubMenu.DISABLE, SubMenu.MOVE_BOTTOM],
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

const expandedNodeIds = (): Array<string> => {
    return data.value
        .filter((node: any) => !settingsStore.collapsedMenuNodeIds.includes(node.id))
        .map((node: any) => node.id)
}
const expandedIds = ref(expandedNodeIds())
const addDashbaord = (dashboardUID: UID) => {
    const newDashboard = settingsStore.dashboards.find(
        (dashboard) => dashboard.uid === dashboardUID,
    )!
    const dashboardParent = data.value.find((item: any) => item.id === 'dashboards')
    dashboardParent!.children.push({
        id: dashboardUID,
        label: newDashboard.name,
        icon: mdiChartBoxOutline,
        deviceUID: 'Dashboards',
        dashboardUID: dashboardUID,
        name: dashboardUID,
        to: { name: 'dashboards', params: { dashboardUID: dashboardUID } },
        menus: [Menu.DASHBOARD_HOME, Menu.DASHBOARD_RENAME],
        subMenus: [
            SubMenu.MOVE_TOP,
            SubMenu.PIN,
            SubMenu.DASHBOARD_DUPLICATE,
            SubMenu.DASHBOARD_DELETE,
            SubMenu.MOVE_BOTTOM,
        ],
    })
}

interface DashboardUIDObj {
    dashboardUID: UID
}

const addDashboardMenu = (dashboardUIDObj: DashboardUIDObj): void =>
    addDashbaord(dashboardUIDObj.dashboardUID)
emitter.on('dashboard-add-menu', addDashboardMenu)

const deleteDashboard = async (dashboardUID: UID): Promise<void> => {
    if (route.params != null && route.params.dashboardUID === dashboardUID) {
        await router.push({ name: 'system-overview' })
    }
    const dashboardParent = data.value.find((item: any) => item.id === 'dashboards')
    dashboardParent!.children = dashboardParent!.children.filter(
        (item: any) => item.id !== dashboardUID,
    )
}
const homeDashboardSet = (): void => {
    const dashboardParent = data.value.find((item: any) => item.id === 'dashboards')
    dashboardParent!.children.forEach((item: any) => {
        item.icon =
            item.dashboardUID === settingsStore.homeDashboard
                ? mdiHomeAnalytics
                : mdiChartBoxOutline
    })
}

/**
 * Updates the mode tree nodes to reflect the current active modes.
 *
 * This is called whenever the active modes change, and also on any settings change.
 * Note that this also performs a router push to the system overview page under certain conditions.
 *
 * @param {string} _ - the UID of the mode that was just activated/deactivated
 */
const activeModesChange = async (_: UID): Promise<void> => {
    treeRef
        .value!.getNode('modes')
        .getChildren()
        .forEach((data: TreeNodeData) => {
            const isActive: boolean = settingsStore.modeActiveCurrent === data.uid
            const isRecentlyActive: boolean = settingsStore.modeActivePrevious === data.uid
            data.icon = isActive
                ? mdiBookmarkCheckOutline
                : isRecentlyActive
                  ? mdiBookmarkOffOutline
                  : mdiBookmarkOutline
            data.isActive = isActive
            data.isRecentlyActive = isRecentlyActive
        })
    if (route.params != null && route.params.modeUID != null) {
        // if on any Modes View page, redirect so that the view doesn't contain outdated info,
        // otherwise we don't need to redirect.
        await router.push({ name: 'system-overview' })
    }
}
emitter.on('active-modes-change-menu', activeModesChange)
const addMode = (modeUID: UID): void => {
    const newMode = settingsStore.modes.find((mode) => mode.uid === modeUID)!
    const isActive: boolean = settingsStore.modeActiveCurrent === newMode.uid
    const isRecentlyActive: boolean = settingsStore.modeActivePrevious === newMode.uid
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
                { modeRename: true },
                { modeUpdate: true },
                { modeDuplicate: true },
                { modeDelete: true },
            ],
        },
        'modes',
    )
    adjustTreeLeaves()
}

interface ModeUIDObj {
    modeUID: UID
}

const addModeMenu = (modeUIDObj: ModeUIDObj): void => addMode(modeUIDObj.modeUID)
emitter.on('mode-add-menu', addModeMenu)

const deleteMode = async (modeUID: UID): Promise<void> => {
    if (route.params != null && route.params.modeUID === modeUID) {
        await router.push({ name: 'system-overview' })
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
    adjustTreeLeaves()
}

interface ProfileUIDObj {
    profileUID: UID
}

const addProfileMenu = (profileUIDObj: ProfileUIDObj): void => addProfile(profileUIDObj.profileUID)
emitter.on('profile-add-menu', addProfileMenu)

const deleteProfile = async (profileUID: UID): Promise<void> => {
    if (route.params != null && route.params.profileUID === profileUID) {
        await router.push({ name: 'system-overview' })
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
    adjustTreeLeaves()
}

interface FunctionUIDObj {
    functionUID: UID
}

const addFunctionMenu = (functionUIDObj: FunctionUIDObj): void =>
    addFunction(functionUIDObj.functionUID)
emitter.on('function-add-menu', addFunctionMenu)

const deleteFunction = async (functionUID: UID): Promise<void> => {
    if (route.params != null && route.params.functionUID === functionUID) {
        await router.push({ name: 'system-overview' })
    }
    treeRef.value!.remove(treeRef.value!.getNode(`functions_${functionUID}`))
}

interface AlertUIDObj {
    alertUID: UID
}

const addAlert = (alertUIDObj: AlertUIDObj): void => {
    const newAlert = settingsStore.alerts.find((alert) => alert.uid === alertUIDObj.alertUID)
    if (newAlert == null) {
        console.error('Alert with UID: ' + alertUIDObj.alertUID + ' not found')
        return
    }
    const isActive = newAlert.state === AlertState.Active
    treeRef.value!.append(
        {
            id: `alerts_${newAlert.uid}`,
            label: newAlert.name,
            icon: isActive ? mdiBellRingOutline : mdiBellOutline,
            deviceUID: 'Alerts',
            uid: newAlert.uid,
            alertIsActive: isActive,
            to: { name: 'alerts', params: { alertUID: newAlert.uid } },
            options: [{ alertRename: true }, { alertDelete: true }],
        },
        'alerts',
    )
    adjustTreeLeaves()
}
emitter.on('alert-add-menu', addAlert)

const deleteAlert = async (alertUID: UID): Promise<void> => {
    if (route.params != null && route.params.alertUID === alertUID) {
        await router.push({ name: 'system-overview' })
    }
    treeRef.value!.remove(treeRef.value!.getNode(`alerts_${alertUID}`))
}

const alertStateChange = (): void => {
    treeRef
        .value!.getNode('alerts')
        .getChildren()
        .forEach((data: TreeNodeData) => {
            const isActive = settingsStore.alertsActive.includes(data.uid)
            data.alertIsActive = isActive
            data.icon = isActive ? mdiBellRingOutline : mdiBellOutline
        })
}
emitter.on('alert-state-change', alertStateChange)

watch(settingsStore.alertsActive, alertStateChange)

const adjustTreeLeaves = (): void => {
    const dynamicAdjustment = (): void => {
        const mainMenu = document.getElementById('main-menu')
        const children = mainMenu!.getElementsByClassName('el-tree-node__children')
        for (const child of children) {
            const els = child!.getElementsByClassName('el-tree-node__expand-icon is-leaf')
            if (els.length > 0) {
                for (const el of els) {
                    el.innerHTML = '<div class="w-2"/>'
                    el.classList.add('border-l')
                    el.classList.add('border-border-one/70')
                    el.classList.add('!visible')
                    el.classList.add('!h-[inherit]')
                    el.classList.remove('el-icon')
                    el.classList.add('w-2')
                }
            }
        }
    }
    setTimeout(dynamicAdjustment)
}

// add group class to all collapse header items (used to show options menus on hover)
const addGroup = (): void => {
    setTimeout(() => {
        const elements = document.querySelectorAll('.el-collapse-item__header')
        for (const element of elements) {
            element.classList.add('group')
        }
    })
}
const moveToTop = (item: any, data: any): void => {
    item.subMenuRef.hide()
    data.splice(data.indexOf(item), 1)
    data.unshift(item)
}

const moveToBottom = (item: any, data: any): void => {
    item.subMenuRef.hide()
    data.splice(data.indexOf(item), 1)
    data.push(item)
}
onMounted(async () => {
    adjustTreeLeaves()

    // Listen for language change events, refresh menu
    window.addEventListener('language-changed', () => {
        createTreeMenu()
    })

    addGroup()
})

// Remove event listeners when component is unmounted
onUnmounted(() => {
    window.removeEventListener('language-changed', () => {
        createTreeMenu()
    })
})
</script>

<template>
    <div id="system-menu" class="flex text-text-color mx-0 pb-1 tree-text">
        <span class="ml-1 text-2xl mb-2 items-center tree-text">
            {{ daemonState.systemName }}
        </span>
    </div>
    <!--Pined Items-->
    <el-collapse
        v-if="pinnedItems.length > 0"
        expand-icon-position="left"
        :model-value="'pinned'"
        @change="(_activeNames) => addGroup()"
        :before-collapse="() => hoverMenusAreClosed"
    >
        <el-collapse-item name="pinned" :key="'pinned'">
            <template #title>
                <div class="flex group h-full w-full items-center justify-between outline-none">
                    <div class="flex flex-row items-center min-w-0">
                        <svg-icon
                            class="mr-1.5 min-w-7 w-7"
                            type="mdi"
                            :path="mdiPinOutline"
                            :style="{
                                color: deviceChannelColor(undefined, '').value,
                            }"
                            :size="
                                deviceStore.getREMSize(deviceChannelIconSize(undefined, undefined))
                            "
                        />
                        <div class="flex flex-col overflow-hidden">
                            <div class="tree-text leading-tight">
                                {{ t('layout.menu.pinned') }}
                            </div>
                            <div></div>
                        </div>
                    </div>
                </div>
            </template>
            <VueDraggable
                v-model="pinnedItems"
                :scroll="true"
                :force-auto-scroll-fallback="true"
                :fallback-on-body="true"
                :animation="300"
                :direction="'vertical'"
                :scroll-sensitivity="deviceStore.getREMSize(5)"
                :scroll-speed="deviceStore.getREMSize(1.25)"
                :bubble-scroll="true"
                :revert-on-spill="true"
                :force-fallback="true"
                :fallback-tolerance="15"
                :clone="toRaw"
                @start="setHoverMenuStatus(true)"
                @end="setHoverMenuStatus(false)"
            >
                <div v-for="pinnedItem in pinnedItems" :key="pinnedItem.id">
                </div>
            </VueDraggable>
        </el-collapse-item>
    </el-collapse>
    <!--Main Menu-->
    <VueDraggable
        v-model="data"
        target=".cc-root-items"
        :scroll="true"
        :force-auto-scroll-fallback="true"
        :fallback-on-body="true"
        :animation="300"
        :direction="'vertical'"
        :scroll-sensitivity="deviceStore.getREMSize(5)"
        :scroll-speed="deviceStore.getREMSize(1.25)"
        :bubble-scroll="true"
        :revert-on-spill="true"
        :force-fallback="true"
        :fallback-tolerance="15"
        :clone="toRaw"
        @start="setHoverMenuStatus(true)"
        @end="setHoverMenuStatus(false)"
    >
        <el-collapse
            class="cc-root-items"
            expand-icon-position="left"
            :model-value="expandedIds"
            @change="(_activeNames) => addGroup()"
            :before-collapse="() => hoverMenusAreClosed"
        >
            <el-collapse-item v-for="item in data" :name="item.id" :key="item.id">
                <template #title>
                    <!--Root Elements-->
                    <div class="flex group h-full w-full items-center justify-between outline-none">
                        <div class="flex flex-row items-center min-w-0">
                            <svg-icon
                                v-if="item.icon"
                                class="mr-1.5 min-w-7 w-7"
                                type="mdi"
                                :path="item.icon"
                                :style="{
                                    color: deviceChannelColor(item.deviceUID, item.name).value,
                                }"
                                :size="
                                    deviceStore.getREMSize(
                                        deviceChannelIconSize(item.deviceUID, item.name),
                                    )
                                "
                            />
                            <div class="flex flex-col overflow-hidden">
                                <div class="tree-text leading-tight">
                                    {{ item.label }}
                                </div>
                                <div></div>
                            </div>
                        </div>
                        <div
                            class="hidden mr-1 justify-end whitespace-normal"
                            :class="{ 'group-hover:flex': hoverMenusAreClosed }"
                        >
                            <div v-for="menu in item.menus">
                                <menu-device-info
                                    v-if="menu === Menu.DEVICE_INFO"
                                    :device-u-i-d="item.deviceUID"
                                    @click.stop
                                    @open="setHoverMenuStatus"
                                />
                                <menu-rename
                                    v-else-if="menu === Menu.RENAME"
                                    :device-u-i-d="item.deviceUID"
                                    :channel-name="item.name"
                                    @click.stop
                                    @name-change="(value: string) => (item.label = value)"
                                    @open="setHoverMenuStatus"
                                />
                                <menu-custom-sensor-info
                                    v-else-if="menu === Menu.CUSTOM_SENSOR_INFO"
                                    @open="setHoverMenuStatus"
                                />
                                <menu-custom-sensor-add
                                    v-else-if="menu === Menu.CUSTOM_SENSOR_ADD"
                                />
                                <menu-dashboard-info
                                    v-else-if="menu === Menu.DASHBOARD_INFO"
                                    @open="setHoverMenuStatus"
                                />
                                <menu-dashboard-add
                                    v-else-if="menu === Menu.DASHBOARD_ADD"
                                    @added="addDashbaord"
                                />
                                <menu-mode-info
                                    v-else-if="menu === Menu.MODE_INFO"
                                    @open="setHoverMenuStatus"
                                />
                                <menu-mode-add
                                    v-else-if="menu === Menu.MODE_ADD"
                                    @added="addMode"
                                />
                                <menu-profile-info
                                    v-else-if="menu === Menu.PROFILE_INFO"
                                    @open="setHoverMenuStatus"
                                />
                                <menu-profile-add
                                    v-else-if="menu === Menu.PROFILE_ADD"
                                    @added="addProfile"
                                />
                                <menu-function-info
                                    v-else-if="menu === Menu.FUNCTION_INFO"
                                    @open="setHoverMenuStatus"
                                />
                                <menu-function-add
                                    v-else-if="menu === Menu.FUNCTION_ADD"
                                    @added="addFunction"
                                />
                                <menu-alert-info
                                    v-else-if="menu === Menu.ALERT_INFO"
                                    @open="setHoverMenuStatus"
                                />
                                <menu-alert-add v-else-if="menu === Menu.ALERT_ADD" />
                            </div>
                            <div
                                v-if="item.subMenus"
                                v-tooltip.top="{ value: t('layout.menu.tooltips.options') }"
                            >
                                <div
                                    class="rounded-lg w-8 h-8 border-none p-0 text-text-color-secondary outline-0 text-center justify-center items-center flex hover:text-text-color hover:bg-surface-hover"
                                    @click.stop.prevent="(event) => item.subMenuRef.toggle(event)"
                                >
                                    <svg-icon
                                        class="outline-0"
                                        type="mdi"
                                        :path="mdiDotsVertical"
                                        :size="deviceStore.getREMSize(1.5)"
                                    />
                                </div>
                                <Popover
                                    :ref="(el) => (item.subMenuRef = el)"
                                    @show="() => setHoverMenuStatus(true)"
                                    @hide="() => setHoverMenuStatus(false)"
                                >
                                    <div
                                        class="mt-2.5 bg-bg-two border border-border-one p-1 rounded-lg text-text-color"
                                    >
                                        <ul>
                                            <li v-for="subMenu in item.subMenus">
                                                <sub-sub-menu-move-top
                                                    v-if="subMenu === SubMenu.MOVE_TOP"
                                                    @moveTop="moveToTop(item, data)"
                                                />
                                                <sub-sub-menu-disable
                                                    v-else-if="subMenu === SubMenu.DISABLE"
                                                    @close="item.subMenuRef.hide()"
                                                />
                                                <sub-sub-menu-move-bottom
                                                    v-else-if="subMenu === SubMenu.MOVE_BOTTOM"
                                                    @moveBottom="moveToBottom(item, data)"
                                                />
                                            </li>
                                        </ul>
                                    </div>
                                </Popover>
                            </div>
                        </div>
                    </div>
                </template>
                <template v-if="item.children == null || item.children.length === 0" #icon>
                    <div class="w-4" />
                </template>
            </el-collapse-item>
        </el-collapse>
    </VueDraggable>
    <div>
        <el-tree
            ref="treeRef"
            id="main-menu"
            class="w-full"
            :data="data"
            :props="nodeProps"
            node-key="id"
            empty-text="No Matches"
            :indent="deviceStore.getREMSize(0.5)"
            :default-expanded-keys="expandedNodeIds()"
            :render-after-expand="false"
            :icon="TreeIcon"
            @node-collapse="(node) => settingsStore.collapsedMenuNodeIds.push(node.id)"
            @node-expand="
                (node) => {
                    const indexOfNode = settingsStore.collapsedMenuNodeIds.indexOf(node.id)
                    if (indexOfNode < 0) return
                    settingsStore.collapsedMenuNodeIds.splice(indexOfNode, 1)
                }
            "
        >
            <template #default="{ node, data }">
                <el-dropdown
                    :ref="(el) => (data.dropdownRef = el)"
                    :id="data.id"
                    class="ml-0.5 h-full w-full outline-none"
                    :show-timeout="0"
                    :hide-timeout="0"
                    :disabled="data.options == null || data.options.length == 0"
                    placement="top-end"
                    :popper-options="{
                        modifiers: [
                            {
                                name: 'offset',
                                options: {
                                    offset: [
                                        0,
                                        data.isControllable
                                            ? -deviceStore.getREMSize(2.7)
                                            : -deviceStore.getREMSize(2.4),
                                    ],
                                },
                            },
                        ],
                    }"
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
                        class="flex h-full items-center justify-between outline-none"
                        :class="{
                            'text-accent':
                                route.fullPath === '/' &&
                                data.dashboardUID === settingsStore.homeDashboard,
                        }"
                        tabindex="0"
                        exact
                        :exact-active-class="data.to != null ? 'text-accent font-medium' : ''"
                        :to="!data.to ? '' : data.to"
                        v-slot="{ isActive }"
                    >
                        <div class="flex flex-row items-center min-w-0">
                            <svg-icon
                                v-if="data.icon"
                                class="mr-1.5 min-w-6"
                                :class="{
                                    'text-accent': data.isActive,
                                    'text-error': data.alertIsActive,
                                }"
                                type="mdi"
                                :path="data.icon ?? ''"
                                :style="{
                                    color: deviceChannelColor(data.deviceUID, data.name).value,
                                }"
                                :size="
                                    deviceStore.getREMSize(
                                        deviceChannelIconSize(data.deviceUID, data.name),
                                    )
                                "
                            />
                            <div class="flex flex-col overflow-hidden">
                                <div
                                    class="tree-text leading-tight"
                                    :class="{ 'mr-2': data.deviceUID && !data.name }"
                                >
                                    {{ node.label }}
                                </div>
                                <div v-if="data.isControllable" class="mt-0.5">
                                    <menu-control-view
                                        :device-u-i-d="data.deviceUID"
                                        :channel-name="data.name"
                                        :class="{ 'text-text-color-secondary': !isActive }"
                                    />
                                </div>
                            </div>
                        </div>
                        <div class="flex ml-2 justify-end">
                            <div v-if="data.temp != null" class="items-end tree-data">
                                {{ deviceChannelValues(data.deviceUID, data.name)!.temp }}
                                <span>°&nbsp;&nbsp;&nbsp;</span>
                            </div>
                            <div v-else-if="data.freq != null" class="items-end tree-data">
                                {{
                                    formatFrequency(
                                        deviceChannelValues(data.deviceUID, data.name)!.freq!,
                                    )
                                }}
                                <span style="font-size: 0.62rem">
                                    {{ settingsStore.frequencyPrecision === 1 ? 'Mhz' : 'Ghz' }}
                                </span>
                            </div>
                            <div v-else-if="data.watts != null" class="items-end tree-data">
                                {{ deviceChannelValues(data.deviceUID, data.name)!.watts }}
                                <span style="font-size: 0.62rem">W&nbsp;&nbsp;&nbsp;</span>
                            </div>
                            <div
                                v-else-if="data.duty != null && data.rpm == null"
                                class="content-end tree-data"
                            >
                                {{ deviceChannelValues(data.deviceUID, data.name)!.duty }}
                                <span style="font-size: 0.7rem">%&nbsp;&nbsp;&nbsp;</span>
                            </div>
                            <div
                                v-else-if="data.rpm != null && data.duty == null"
                                class="items-end tree-data"
                            >
                                {{ deviceChannelValues(data.deviceUID, data.name)!.rpm }}
                                <span style="font-size: 0.7rem">rpm</span>
                            </div>
                            <div
                                v-else-if="data.duty != null && data.rpm != null"
                                class="items-end flex flex-col leading-none tree-data"
                            >
                                <span :class="{ 'mb-0.5': data.isControllable }">
                                    {{ deviceChannelValues(data.deviceUID, data.name)!.duty }}
                                    <span style="font-size: 0.7rem">%&nbsp;&nbsp;&nbsp;</span>
                                </span>
                                <span :class="{ 'mt-0.5': data.isControllable }">
                                    {{ deviceChannelValues(data.deviceUID, data.name)!.rpm }}
                                    <span style="font-size: 0.7rem">rpm</span>
                                </span>
                            </div>
                        </div>
                    </router-link>
                    <template #dropdown>
                        <div
                            class="border border-border-one bg-bg-two rounded-lg flex content-center items-center justify-center p-[2px]"
                        >
                            <div v-for="option in data.options">
                                <menu-rename
                                    v-if="option.rename"
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
                                <menu-device-info
                                    v-else-if="option.deviceInfo"
                                    :device-u-i-d="data.deviceUID"
                                    @open="(isOpen) => subMenuStatusChange(isOpen, data)"
                                />
                                <menu-dashboard-info
                                    v-else-if="option.dashboardInfo"
                                    @open="(isOpen) => subMenuStatusChange(isOpen, data)"
                                />
                                <menu-dashboard-add
                                    v-else-if="option.dashboardAdd"
                                    @added="addDashbaord"
                                />
                                <menu-dashboard-home
                                    v-else-if="option.dashboardHome"
                                    :dashboard-u-i-d="data.dashboardUID"
                                    @rearrange="rearrangeDashboards"
                                />
                                <menu-dashboard-rename
                                    v-else-if="option.dashboardRename"
                                    :dashboard-u-i-d="data.dashboardUID"
                                    @name-change="(name: string) => (data.label = name)"
                                    @open="(isOpen) => subMenuStatusChange(isOpen, data)"
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
                                <menu-mode-info
                                    v-else-if="option.modeInfo"
                                    @open="(isOpen) => subMenuStatusChange(isOpen, data)"
                                />
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
                                <menu-profile-info
                                    v-else-if="option.profileInfo"
                                    @open="(isOpen) => subMenuStatusChange(isOpen, data)"
                                />
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
                                <menu-function-info
                                    v-else-if="option.functionInfo"
                                    @open="(isOpen) => subMenuStatusChange(isOpen, data)"
                                />
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
                                <menu-alert-info
                                    v-else-if="option.alertInfo"
                                    @open="(isOpen) => subMenuStatusChange(isOpen, data)"
                                />
                                <menu-alert-add v-else-if="option.alertAdd" />
                                <menu-alert-rename
                                    v-else-if="option.alertRename"
                                    :alert-u-i-d="data.uid"
                                    @name-change="(name: string) => (data.label = name)"
                                    @open="(isOpen) => subMenuStatusChange(isOpen, data)"
                                />
                                <menu-alert-delete
                                    v-else-if="option.alertDelete"
                                    :alert-u-i-d="data.uid"
                                    @deleted="deleteAlert"
                                />
                                <menu-custom-sensor-info
                                    v-else-if="option.customSensorInfo"
                                    @open="(isOpen) => subMenuStatusChange(isOpen, data)"
                                />
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

.el-collapse {
    --el-fill-color-blank: rgb(var(--colors-bg-one));
    --el-collapse-header-text-color: rgb(var(--colors-text-color));
    --el-collapse-header-font-size: 1rem;
    --el-collapse-content-font-size: 1rem;
    --el-collapse-content-text-color: rgb(var(--colors-text-color));
    border-top: 0;
    border-bottom: 0;
    --el-collapse-border-color: rgb(var(--colors-bg-one));
    --el-collapse-header-height: 2.5rem;
}

.tree-text {
    // This is THE WAY to handle elements overflowing with white-space: nowrap
    // same as tw line-clamp-1
    overflow: hidden;
    display: -webkit-box;
    line-clamp: 1;
    -webkit-line-clamp: 1;
    -webkit-box-orient: vertical;
}

.tree-data {
    white-space: nowrap;
    align-items: end;
    align-content: end;
    //display: inline-block;
    //min-width: 5rem;
}

.sortable-fallback {
    color: rgb(var(--colors-text-color));
    background-color: rgba(var(--colors-bg-two) / 0.625);
    border-radius: 0.5rem;
}

//.custom-tree-node {
//    flex: 1;
//    display: flex;
//    align-items: center;
//    justify-content: space-between;
//}
</style>
<style lang="scss">
/******************************************************************************************
* Unscoped Style needed to deeply affect the element components
*/
.el-collapse-icon-position-left .el-collapse-item__header {
    gap: 0.125rem;
}

.el-collapse-item__header {
    border-bottom: 0;
    //display: block;
    border-radius: 0.5rem;
}

.el-collapse-item__header:hover {
    background-color: rgb(var(--colors-bg-two));
}

.el-collapse-item__header.focusing:focus:not(:hover) {
    // strange issue with fast clicking then triggers this (likely upstream bug)
    color: rgb(var(--colors-text-color));
}

.el-collapse-item__title {
    width: 100%;
}

.el-collapse-item__wrap {
    border-bottom: 0;
    // creates a bit of separation between expanded items
    margin-bottom: 0.5rem;
}

.el-collapse-item__content {
    line-height: normal;
    padding-bottom: 0;
}

.el-collapse-item__arrow {
    font-size: 1rem;
    //font-weight: 800;
    padding-left: 1px !important;
}

.el-tree-node__content {
    border-radius: 0.5rem;
}

.speed-control-menu {
    --el-tree-node-content-height: 3rem;
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
.el-tree-node__expand-icon {
    font-size: 1rem;
    padding-left: 1px !important;
}
</style>
