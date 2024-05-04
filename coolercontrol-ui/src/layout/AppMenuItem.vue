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

<script setup>
import { defineAsyncComponent, onBeforeMount, ref } from 'vue'
import { useLayout } from '@/layout/composables/layout'
import Button from 'primevue/button'
import Menu from 'primevue/menu'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import { ElColorPicker } from 'element-plus'
import 'element-plus/es/components/color-picker/style/css'
import SvgIcon from '@jamescoyle/vue-icon'
import { useDialog } from 'primevue/usedialog'
import { useConfirm } from 'primevue/useconfirm'
import { useToast } from 'primevue/usetoast'
import { CustomSensor } from '@/models/CustomSensor'

const NameEditor = defineAsyncComponent(() => import('../components/NameEditor.vue'))
const CustomSensorEditor = defineAsyncComponent(
    () => import('../components/CustomSensorEditor.vue'),
)
const dialog = useDialog()
const confirm = useConfirm()
const toast = useToast()

const { layoutState, onMenuToggle } = useLayout()

const props = defineProps({
    item: {
        type: Object,
        default: () => ({}),
    },
    index: {
        type: Number,
        default: 0,
    },
    root: {
        type: Boolean,
        default: true,
    },
    parentItemKey: {
        type: String,
        default: null,
    },
})

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const isActiveMenu = ref(
    props.item.deviceUID != null
        ? !settingsStore.allUIDeviceSettings.get(props.item.deviceUID).menuCollapsed
        : true,
)
const itemKey = ref(null)

onBeforeMount(() => {
    itemKey.value = props.parentItemKey
        ? props.parentItemKey + '-' + props.index
        : String(props.index)
    // only simply toggle for the menu
})

const itemClick = (event, item, index) => {
    if (item.disabled) {
        event.preventDefault()
        return
    }

    const { overlayMenuActive, staticMenuMobileActive } = layoutState

    if ((item.to || item.url) && (staticMenuMobileActive.value || overlayMenuActive.value)) {
        onMenuToggle()
    }

    if (item.command) {
        item.command({ originalEvent: event, item: item })
    }

    if (props.item.deviceUID != null && props.item.items != null && props.item.items.length > 0) {
        isActiveMenu.value = !isActiveMenu.value // very simply toggle
        settingsStore.allUIDeviceSettings.get(props.item.deviceUID).menuCollapsed =
            !isActiveMenu.value
    }
}

const deviceItemsValues = (deviceUID, channelName) =>
    deviceStore.currentDeviceStatus.get(deviceUID)?.get(channelName)
const optionsMenu = ref()
const optionsToggle = (event) => {
    // todo: this call creates a long recursive loop in primevue 3.45.0
    // Cause looks to be in this change:
    // https://github.com/primefaces/primevue/compare/3.44.0...3.45.0#diff-64d7eb2f346b2ec23be99555bc49b6016d70a799c80d187aa5c2a9e453cbe0c1
    // components/lib/utils/DomHandler.js
    // Reproduction is difficult due to this template-menu's nature. Holding version back for now.
    optionsMenu.value.toggle(event)
}
const color = ref(
    props.item.color
        ? settingsStore.allUIDeviceSettings
              .get(props.item.deviceUID)
              .sensorsAndChannels.get(props.item.name).color
        : '',
)
const hideEnabled = ref(
    props.item.name != null // sensors and channels have specific names, and are hid-able
        ? settingsStore.allUIDeviceSettings
              .get(props.item.deviceUID)
              .sensorsAndChannels.get(props.item.name).hide
        : false,
)
const optionButtonAction = async (label) => {
    if (label.includes('Hide')) {
        hideEnabled.value = !hideEnabled.value
        if (label === 'Hide All' || label === 'Show All') {
            for (const sensorChannel of settingsStore.allUIDeviceSettings
                .get(props.item.deviceUID)
                .sensorsAndChannels.values()) {
                sensorChannel.hide = hideEnabled.value
            }
            settingsStore.sidebarMenuUpdate()
        } else {
            settingsStore.allUIDeviceSettings
                .get(props.item.deviceUID)
                .sensorsAndChannels.get(props.item.name).hide = hideEnabled.value
        }
    } else if (label === 'Rename') {
        dialog.open(NameEditor, {
            props: {
                header: 'Edit Name',
                position: 'center',
                modal: true,
                dismissableMask: false,
            },
            data: {
                deviceUID: props.item.deviceUID,
                sensorName: props.item.name,
            },
            onClose: (options) => {
                const data = options.data
                if (data) {
                    const isDeviceName = props.item.name == null
                    if (data.newName) {
                        data.newName = deviceStore.sanitizeString(data.newName)
                        if (isDeviceName) {
                            settingsStore.allUIDeviceSettings.get(props.item.deviceUID).userName =
                                data.newName
                        } else {
                            settingsStore.allUIDeviceSettings
                                .get(props.item.deviceUID)
                                .sensorsAndChannels.get(props.item.name).userName = data.newName
                        }
                    } else {
                        // reset
                        if (isDeviceName) {
                            settingsStore.allUIDeviceSettings.get(props.item.deviceUID).userName =
                                undefined
                        } else {
                            settingsStore.allUIDeviceSettings
                                .get(props.item.deviceUID)
                                .sensorsAndChannels.get(props.item.name).userName = undefined
                        }
                    }
                    props.item.label = isDeviceName
                        ? settingsStore.allUIDeviceSettings.get(props.item.deviceUID).name
                        : settingsStore.allUIDeviceSettings
                              .get(props.item.deviceUID)
                              .sensorsAndChannels.get(props.item.name).name
                }
            },
        })
    } else if (label === 'Add Sensor') {
        addSensor()
    } else if (label === 'Edit') {
        const customSensor = await settingsStore.getCustomSensor(props.item.name)
        if (customSensor == null) {
            console.error(`CustomSensor not found for this name: ${props.item.name}`)
            return
        }
        dialog.open(CustomSensorEditor, {
            props: {
                header: 'Edit Custom Sensor',
                position: 'center',
                modal: true,
                dismissableMask: false,
            },
            data: {
                deviceUID: props.item.deviceUID,
                customSensor: customSensor,
                operation: 'edit',
            },
        })
    } else if (label === 'Delete') {
        confirm.require({
            message: 'Are you sure you want to delete this Custom Sensor?',
            header: 'Delete Sensor',
            icon: 'pi pi-exclamation-triangle',
            accept: async () =>
                await settingsStore.deleteCustomSensor(props.item.deviceUID, props.item.name),
        })
    } else if (label === 'Blacklist') {
        if (!settingsStore.ccDeviceSettings.has(props.item.deviceUID)) {
            console.error(`CCDeviceSetting not found for this device: ${props.item.deviceUID}`)
            return
        }
        const ccSetting = settingsStore.ccDeviceSettings.get(props.item.deviceUID)
        confirm.require({
            message:
                'Blacklisting a device requires a restart of the Daemon and UI. You can re-enable devices later in the settings menu. Are you sure you want to proceed?',
            header: 'Blacklist Device',
            icon: 'pi pi-exclamation-triangle',
            accept: async () => {
                ccSetting.disable = true
                const successful = await deviceStore.daemonClient.saveCCDeviceSettings(
                    ccSetting.uid,
                    ccSetting,
                )
                if (successful) {
                    toast.add({
                        severity: 'success',
                        summary: 'Success',
                        detail: 'Device Blacklisted. Restarting now',
                        life: 3000,
                    })
                    await deviceStore.daemonClient.shutdownDaemon()
                    await deviceStore.waitAndReload()
                } else {
                    toast.add({
                        severity: 'error',
                        summary: 'Error',
                        detail: 'Unknown error trying to blacklist device. See logs for details.',
                        life: 4000,
                    })
                }
            },
        })
    }
}

const addSensor = () => {
    const tempNumbers = []
    for (const device of deviceStore.allDevices()) {
        if (device.uid !== props.item.deviceUID) {
            continue
        }
        for (const temp of device.status.temps) {
            tempNumbers.push(Number(temp.name.replace(/^\D+/g, '')))
        }
    }
    tempNumbers.sort()
    const newSensorNumber = tempNumbers.length === 0 ? 1 : tempNumbers[tempNumbers.length - 1] + 1
    const newCustomSensor = new CustomSensor(`sensor${newSensorNumber}`)
    dialog.open(CustomSensorEditor, {
        props: {
            header: 'Add Custom Sensor',
            position: 'center',
            modal: true,
            dismissableMask: false,
        },
        data: {
            deviceUID: props.item.deviceUID,
            customSensor: newCustomSensor,
            operation: 'add',
        },
    })
}

const hideOrShowLabel = (label) => {
    if (label === 'Hide' && hideEnabled.value) {
        return 'Show'
    } else if (label === 'Hide' && !hideEnabled.value) {
        return label
    } else if (label === 'Hide All' && hideEnabled.value) {
        return 'Show All'
    } else if (label === 'Hide All' && !hideEnabled.value) {
        return label
    } else {
        return label
    }
}

const setNewColor = (newColor) => {
    if (newColor == null) {
        settingsStore.allUIDeviceSettings
            .get(props.item.deviceUID)
            .sensorsAndChannels.get(props.item.name).userColor = undefined
        color.value = settingsStore.allUIDeviceSettings
            .get(props.item.deviceUID)
            .sensorsAndChannels.get(props.item.name).defaultColor
    } else {
        settingsStore.allUIDeviceSettings
            .get(props.item.deviceUID)
            .sensorsAndChannels.get(props.item.name).userColor = newColor
    }
}

settingsStore.$onAction(({ name, after }) => {
    if (name === 'sidebarMenuUpdate') {
        after(() => {
            if (props.parentItemKey != null && props.parentItemKey.includes('-')) {
                // sensor/channel menu items only
                hideEnabled.value = settingsStore.allUIDeviceSettings
                    .get(props.item.deviceUID)
                    .sensorsAndChannels.get(props.item.name).hide
            }
        })
    }
})
</script>

<template>
    <li :class="{ 'layout-root-menuitem': root, 'active-menuitem': isActiveMenu }">
        <div v-if="root && item.visible !== false" class="layout-menuitem-root-text">
            {{ item.label }}
        </div>
        <a
            v-if="(!item.to || item.items) && item.visible !== false"
            :href="item.url"
            @click="itemClick($event, item, index)"
            :class="item.class"
            :target="item.target"
            tabindex="0"
        >
            <!--      root element icon and label:-->
            <svg-icon
                class="layout-menuitem-icon"
                :style="item.iconStyle"
                type="mdi"
                :path="item.icon ?? ''"
                :size="deviceStore.getREMSize(1.3)"
            />
            <span class="layout-menuitem-text">{{ item.label }}</span>
            <span class="layout-menuitem-text ml-auto"></span>
            <i class="pi pi-fw pi-angle-down layout-submenu-toggler" v-if="item.items"></i>
            <Button
                v-if="item.customSensors"
                aria-label="options"
                icon="pi pi-plus"
                rounded
                text
                plain
                size="small"
                class="ml-1 p-3 channel-options"
                style="height: 0.1rem; width: 0.1rem; box-shadow: none; visibility: hidden"
                type="button"
                aria-haspopup="true"
                @click.stop.prevent="addSensor"
            />
            <Button
                v-else-if="item.options"
                aria-label="options"
                icon="pi pi-ellipsis-v"
                rounded
                text
                plain
                size="small"
                class="ml-1 p-3 channel-options"
                style="height: 0.1rem; width: 0.1rem; box-shadow: none; visibility: hidden"
                type="button"
                aria-haspopup="true"
                @click.stop.prevent="optionsToggle"
            />
            <Button
                v-else
                icon="pi pi-ellipsis-v"
                rounded
                text
                plain
                size="small"
                class="ml-1 p-3"
                style="height: 0.1rem; width: 0.1rem; box-shadow: none; visibility: hidden"
                type="button"
            />
            <!--      Options Menu for root elements:-->
            <Menu ref="optionsMenu" :model="item.options" :popup="true">
                <template #item="{ label, item, props }">
                    <a class="flex p-menuitem-link" @click="optionButtonAction(label)">
                        <span
                            v-if="item.label.includes('Hide') && !hideEnabled"
                            class="pi pi-fw pi-eye-slash mr-2"
                        />
                        <span
                            v-else-if="item.label.includes('Hide') && hideEnabled"
                            class="pi pi-fw pi-eye mr-2"
                        />
                        <span v-else v-bind="props.icon" />
                        <span v-bind="props.label">{{ hideOrShowLabel(label) }}</span>
                    </a>
                </template>
            </Menu>
        </a>

        <router-link
            v-if="
                (!hideEnabled || settingsStore.displayHiddenItems) &&
                item.to &&
                !item.items &&
                item.visible !== false
            "
            @click="itemClick($event, item, index)"
            :class="[item.class, 'device-channel']"
            :exact-active-class="hideEnabled ? '' : 'active-route'"
            exact
            tabindex="0"
            :to="hideEnabled ? '' : item.to"
        >
            <div
                v-if="item.color"
                class="color-wrapper pi pi-fw layout-menuitem-icon"
                @click.stop.prevent
            >
                <el-color-picker
                    v-model="color"
                    color-format="hex"
                    :predefine="settingsStore.predefinedColorOptions"
                    @change="setNewColor"
                    :disabled="hideEnabled"
                />
            </div>
            <svg-icon
                v-else
                class="layout-menuitem-icon"
                :style="item.iconStyle"
                type="mdi"
                :path="item.icon ?? ''"
                :size="deviceStore.getREMSize(1.3)"
            />
            <span class="layout-menuitem-text" :class="{ 'disabled-text': hideEnabled }">
                {{ item.label }}
            </span>
            <i class="pi pi-fw pi-angle-down layout-submenu-toggler" v-if="item.items"></i>
            <span
                v-if="item.temp"
                class="layout-menuitem-text ml-auto"
                :class="{ 'disabled-text': hideEnabled }"
            >
                {{ deviceItemsValues(item.deviceUID, item.name).temp }}
                <span>°&nbsp;&nbsp;&nbsp;</span>
            </span>
            <span
                v-else-if="item.freq != null"
                class="layout-menuitem-text ml-auto"
                :class="{ 'disabled-text': hideEnabled }"
            >
                {{ deviceItemsValues(item.deviceUID, item.name).freq }}
                <span style="font-size: 0.62rem">mhz</span>
            </span>
            <span
                v-else-if="item.duty != null && !item.rpm && item.rpm !== 0"
                class="layout-menuitem-text ml-auto text-right"
                :class="{ 'disabled-text': hideEnabled }"
            >
                {{ deviceItemsValues(item.deviceUID, item.name).duty }}
                <span style="font-size: 0.7rem">%&nbsp;&nbsp;&nbsp;</span>
            </span>
            <span
                v-else-if="item.duty == null && item.rpm != null"
                class="layout-menuitem-text ml-auto text-right"
                :class="{ 'disabled-text': hideEnabled }"
            >
                {{ deviceItemsValues(item.deviceUID, item.name).rpm }}
                <span style="font-size: 0.7rem">rpm</span>
            </span>
            <span
                v-else-if="item.duty != null && item.rpm != null"
                class="layout-menuitem-text ml-auto text-right"
                :class="{ 'disabled-text': hideEnabled }"
            >
                {{ deviceItemsValues(item.deviceUID, item.name).duty }}
                <span style="font-size: 0.7rem">%&nbsp;&nbsp;&nbsp;</span>
                <br />
                {{ deviceItemsValues(item.deviceUID, item.name).rpm }}
                <span style="font-size: 0.7rem">rpm</span>
            </span>
            <span v-else class="layout-menuitem-text ml-auto"></span>
            <Button
                v-if="item.options"
                aria-label="options"
                icon="pi pi-ellipsis-v"
                rounded
                text
                plain
                size="small"
                class="ml-1 p-3 channel-options"
                style="height: 0.1rem; width: 0.1rem; box-shadow: none; visibility: hidden"
                type="button"
                aria-haspopup="true"
                @click.stop.prevent="optionsToggle"
            />
            <Button
                v-else
                icon="pi pi-ellipsis-v"
                rounded
                text
                plain
                size="small"
                class="ml-1 p-3"
                style="height: 0.1rem; width: 0.1rem; box-shadow: none; visibility: hidden"
                type="button"
            />
            <Menu ref="optionsMenu" :model="item.options" :popup="true">
                <template #item="{ label, item, props }">
                    <a class="flex p-menuitem-link" @click="optionButtonAction(item.label)">
                        <span
                            v-if="item.label === 'Hide' && hideEnabled"
                            class="pi pi-fw pi-eye mr-2"
                        />
                        <span
                            v-else-if="item.label === 'Hide' && !hideEnabled"
                            class="pi pi-fw pi-eye-slash mr-2"
                        />
                        <span v-else v-bind="props.icon" />
                        <span v-bind="props.label">{{ hideOrShowLabel(label) }}</span>
                    </a>
                </template>
            </Menu>
        </router-link>
        <Transition v-if="item.items && item.visible !== false" name="layout-submenu">
            <ul v-show="root ? true : isActiveMenu" class="layout-submenu">
                <!--      <ul v-show=true class="layout-submenu">-->
                <app-menu-item
                    v-for="(child, i) in item.items"
                    :key="child"
                    :index="i"
                    :item="child"
                    :parentItemKey="itemKey"
                    :root="false"
                >
                </app-menu-item>
            </ul>
        </Transition>
    </li>
</template>

<style lang="scss" scoped>
.color-wrapper :deep(.el-color-picker__trigger) {
    border: 0 !important;
    padding: 0 !important;
    height: 1rem !important;
    width: 1rem !important;
}

.color-wrapper :deep(.el-color-picker__mask) {
    border: 0 !important;
    padding: 0 !important;
    height: 1rem !important;
    width: 1rem !important;
    top: 0;
    left: 0;
    background-color: rgba(0, 0, 0, 0.7);
}

.color-wrapper :deep(.el-color-picker__color) {
    border: 0 !important;
    //border-radius: 10px !important;
}

.color-wrapper :deep(.el-color-picker__color-inner) {
    border-radius: 4px !important;
}

.color-wrapper :deep(.el-color-picker .el-color-picker__icon) {
    display: none;
}

.disabled-text {
    opacity: 0.2;
}

.layout-menuitem-text {
    //max-width: 8em; // fallback if needed
    max-width: 18ch;
}
</style>

<style>
.el-color-picker__panel {
    padding: 1rem;
    border-radius: 12px;
    background-color: var(--surface-card);
}

.el-color-picker__panel.el-popper {
    border-color: var(--surface-border);
}

.el-button {
    border-color: var(--surface-border);
    background-color: var(--cc-bg-two);
}

el-button:focus,
.el-button:hover {
    color: var(--text-color);
    border-color: var(--surface-border);
    background-color: var(--cc-bg-three);
}

.el-button.is-text:not(.is-disabled):focus,
.el-button.is-text:not(.is-disabled):hover {
    background-color: var(--surface-card);
}

.el-input__wrapper {
    background-color: var(--cc-bg-three);
    box-shadow: none;
}

.el-input__inner {
    color: var(--text-color-secondary);
}

.el-input__wrapper:hover,
.el-input__wrapper:active,
.el-input__wrapper:focus {
    box-shadow: var(--cc-context-color);
}
</style>
