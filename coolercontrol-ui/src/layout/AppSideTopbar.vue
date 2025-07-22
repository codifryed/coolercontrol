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
import { computed, inject, ref } from 'vue'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import {
    mdiAccountOffOutline,
    mdiAccountOutline,
    mdiArrowLeft,
    mdiBellOutline,
    mdiBellPlusOutline,
    mdiBellRingOutline,
    mdiBookmarkCheckOutline,
    mdiBookmarkOffOutline,
    mdiBookmarkOutline,
    mdiBookmarkPlusOutline,
    mdiChartBoxOutline,
    mdiChartBoxPlusOutline,
    mdiCogOutline,
    mdiFlaskPlusOutline,
    mdiHomeAnalytics,
    mdiMenuClose,
    mdiMenuOpen,
    mdiOpenInNew,
    mdiPlus,
    mdiPlusBoxMultipleOutline,
    mdiPlusCircleMultipleOutline,
    mdiPower,
    mdiTuneVerticalVariant,
} from '@mdi/js'
import { useDeviceStore } from '@/stores/DeviceStore'
import Button from 'primevue/button'
import Menu from 'primevue/menu'
import OverlayBadge from 'primevue/overlaybadge'
import { type DropdownInstance, ElDropdown } from 'element-plus'
import { Emitter, EventType } from 'mitt'
import { useRouter } from 'vue-router'
import { useConfirm } from 'primevue/useconfirm'
import { useToast } from 'primevue/usetoast'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { DaemonStatus, useDaemonState } from '@/stores/DaemonState.ts'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
const { getREMSize } = useDeviceStore()
const deviceStore = useDeviceStore()
const router = useRouter()
const confirm = useConfirm()
const toast = useToast()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!

const logoUrl = `/logo.svg`

const settingsStore = useSettingsStore()
const daemonState = useDaemonState()

const daemonBadgeSeverity = computed((): string => {
    switch (daemonState.status) {
        case DaemonStatus.OK:
            return 'success'
        case DaemonStatus.WARN:
            return 'warn'
        case DaemonStatus.ERROR:
            return 'error'
        default:
            return 'error'
    }
})
const numberOfActiveAlerts = computed((): number => settingsStore.alertsActive.length)

const dashboardMenuRef = ref<DropdownInstance>()
const dashboardItems = computed(() => {
    const dashboardItems = []
    for (const dashboard of settingsStore.dashboards) {
        dashboardItems.push({
            label: dashboard.name,
            mdiIcon: mdiChartBoxOutline,
            uid: dashboard.uid,
            command: async () => {
                dashboardMenuRef.value?.handleClose()
                await router.push({ name: 'dashboards', params: { dashboardUID: dashboard.uid } })
            },
        })
    }
    return dashboardItems
})
const modesItems = computed(() => {
    const menuItems = []
    for (const mode of settingsStore.modes) {
        const isActive = settingsStore.modeActiveCurrent === mode.uid
        const isRecentlyActive = settingsStore.modeActivePrevious === mode.uid
        menuItems.push({
            label: mode.name,
            isActive: isActive,
            isRecentlyActive: isRecentlyActive,
            mdiIcon: isActive
                ? mdiBookmarkCheckOutline
                : isRecentlyActive
                  ? mdiBookmarkOffOutline
                  : mdiBookmarkOutline,
            command: async () => {
                await settingsStore.activateMode(mode.uid)
            },
        })
    }
    return menuItems
})
const activatePreviousMode = async (): Promise<void> => {
    if (settingsStore.modeActivePrevious == null) {
        return
    }
    await settingsStore.activateMode(settingsStore.modeActivePrevious)
}

const accessMenuRef = ref<DropdownInstance>()
const accessItems = computed(() => [
    {
        label: t('layout.topbar.login'),
        icon: 'pi pi-fw pi-sign-in',
        command: async () => {
            accessMenuRef.value?.handleClose()
            await deviceStore.login()
        },
        visible: !deviceStore.loggedIn,
    },
    {
        label: t('layout.topbar.logout'),
        icon: 'pi pi-fw pi-sign-out',
        command: async () => {
            accessMenuRef.value?.handleClose()
            await deviceStore.logout()
        },
        visible: deviceStore.loggedIn,
    },
    {
        label: t('layout.topbar.changePassword'),
        icon: 'pi pi-fw pi-shield',
        command: async () => {
            accessMenuRef.value?.handleClose()
            await deviceStore.setPasswd()
        },
        visible: deviceStore.loggedIn,
    },
])

const restartItems = computed(() => {
    const items = [
        {
            label: t('layout.topbar.restartUI'),
            icon: 'pi pi-fw pi-refresh',
            command: () => {
                deviceStore.reloadUI()
            },
        },
        {
            label: t('layout.topbar.restartDaemonAndUI'),
            icon: 'pi pi-fw pi-sync',
            command: async () => {
                confirm.require({
                    message: t('layout.topbar.restartConfirmMessage'),
                    header: t('layout.topbar.restartConfirmHeader'),
                    icon: 'pi pi-exclamation-triangle',
                    defaultFocus: 'accept',
                    accept: async () => {
                        const successful = await deviceStore.daemonClient.shutdownDaemon()
                        if (successful) {
                            toast.add({
                                severity: 'success',
                                summary: t('common.success'),
                                detail: t('layout.topbar.shutdownSuccess'),
                                life: 6000,
                            })
                            await deviceStore.waitAndReload()
                        } else {
                            toast.add({
                                severity: 'error',
                                summary: t('common.error'),
                                detail: t('layout.topbar.shutdownError'),
                                life: 4000,
                            })
                        }
                    },
                })
            },
        },
    ]

    if (deviceStore.isQtApp()) {
        items.push({
            label: t('layout.topbar.quitDesktopApp'),
            icon: 'pi pi-fw pi-power-off',
            command: async () => {
                // call quit to the backend.
                // @ts-ignore
                const ipc = window.ipc
                ipc.forceQuit()
            },
        })
    }

    return items
})

const addMenuRef = ref<DropdownInstance>()
const addItems = computed(() => [
    {
        label: t('layout.add.dashboard'),
        mdiIcon: mdiChartBoxPlusOutline,
        command: () => {
            addMenuRef.value?.handleClose()
            emitter.emit('dashboard-add')
        },
    },
    {
        label: t('layout.add.mode'),
        mdiIcon: mdiBookmarkPlusOutline,
        command: () => {
            addMenuRef.value?.handleClose()
            emitter.emit('mode-add')
        },
    },
    {
        label: t('layout.add.profile'),
        mdiIcon: mdiPlusBoxMultipleOutline,
        command: () => {
            addMenuRef.value?.handleClose()
            emitter.emit('profile-add')
        },
    },
    {
        label: t('layout.add.function'),
        mdiIcon: mdiFlaskPlusOutline,
        command: () => {
            addMenuRef.value?.handleClose()
            emitter.emit('function-add')
        },
    },
    {
        label: t('layout.add.alert'),
        mdiIcon: mdiBellPlusOutline,
        command: () => {
            addMenuRef.value?.handleClose()
            emitter.emit('alert-add')
        },
    },
    {
        label: t('layout.add.customSensor'),
        mdiIcon: mdiPlusCircleMultipleOutline,
        command: () => {
            addMenuRef.value?.handleClose()
            emitter.emit('custom-sensor-add')
        },
    },
])
</script>

<template>
    <div class="flex flex-col h-full align-middle justify-items-center">
        <Button
            id="logo"
            class="mt-0.5 mx-0.5 !rounded-lg border-none text-text-color-secondary w-12 h-12 !p-0 hover:text-text-color hover:bg-surface-hover/15"
            v-tooltip.right="t('layout.topbar.applicationInfo')"
        >
            <router-link :to="{ name: 'app-info' }" class="outline-none">
                <OverlayBadge
                    :severity="daemonBadgeSeverity"
                    class="[&>[data-pc-name=pcbadge]]:!right-[50.3%] [&>[data-pc-name=pcbadge]]:!top-[74.5%] [&>[data-pc-name=pcbadge]]:!outline-bg-one [&>[data-pc-name=pcbadge]]:!outline-[1px] [&>[data-pc-name=pcbadge]]:w-[0.6rem] [&>[data-pc-name=pcbadge]]:h-[0.6rem]"
                >
                    <img :src="logoUrl" alt="logo" class="w-10 h-10" />
                </OverlayBadge>
            </router-link>
        </Button>

        <!--Back-->
        <Button
            id="back"
            class="mt-4 ml-0.5 !rounded-lg border-none w-12 h-12 !p-0 text-text-color-secondary hover:text-text-color hover:bg-surface-hover outline-none"
            v-tooltip.right="t('layout.topbar.back')"
            @click="router.back()"
        >
            <svg-icon type="mdi" :path="mdiArrowLeft" :size="getREMSize(1.75)" />
        </Button>

        <!--Add-->
        <el-dropdown
            id="add"
            ref="addMenuRef"
            :show-timeout="0"
            :hide-timeout="100"
            :popper-options="{
                modifiers: [{ name: 'computeStyles', options: { gpuAcceleration: true } }],
            }"
            popper-class="ml-[3.75rem] mt-[-3.75rem]"
        >
            <Button
                class="mt-3 mx-0.5 !rounded-lg border-none w-12 h-12 !p-0 text-text-color-secondary hover:text-text-color hover:bg-surface-hover outline-none"
                aria-haspopup="true"
                aria-controls="modes-overlay-menu"
            >
                <svg-icon type="mdi" :path="mdiPlus" :size="getREMSize(2.0)" />
            </Button>
            <template #dropdown>
                <Menu :model="addItems" append-to="self">
                    <template #item="{ item, props }">
                        <a
                            v-bind="props.action"
                            class="inline-flex items-center px-0.5 w-full h-full"
                        >
                            <svg-icon type="mdi" :path="item.mdiIcon" :size="getREMSize(1.5)" />
                            <span class="ml-1.5">{{ item.label }}</span>
                        </a>
                    </template>
                </Menu>
            </template>
        </el-dropdown>

        <div class="px-1 mt-3">
            <div class="border-b border-text-color-secondary" />
        </div>

        <!--Dashboards Quick Menu-->
        <el-dropdown
            id="dashboard-quick"
            ref="dashboardMenuRef"
            :show-timeout="0"
            :hide-timeout="100"
            :popper-options="{
                modifiers: [{ name: 'computeStyles', options: { gpuAcceleration: true } }],
            }"
            popper-class="ml-[3.75rem] mt-[-3.75rem]"
        >
            <Button
                class="mt-4 ml-0.5 !rounded-lg border-none text-text-color-secondary w-12 h-12 !p-0 hover:text-text-color hover:bg-surface-hover outline-none"
                @click="router.push({ name: 'system-overview' })"
            >
                <svg-icon
                    :class="{
                        'text-accent':
                            router.currentRoute.value.fullPath === '/' ||
                            router.currentRoute.value.params.dashboardUID ===
                                settingsStore.homeDashboard,
                    }"
                    type="mdi"
                    :path="mdiHomeAnalytics"
                    :size="getREMSize(1.75)"
                />
            </Button>
            <template #dropdown>
                <Menu :model="dashboardItems" append-to="self">
                    <template #item="{ item, props }">
                        <a
                            v-bind="props.action"
                            class="inline-flex items-center px-0.5 w-full h-full"
                        >
                            <svg-icon
                                type="mdi"
                                :class="{
                                    'text-accent':
                                        router.currentRoute.value.params.dashboardUID ===
                                            item.uid ||
                                        (router.currentRoute.value.fullPath === '/' &&
                                            item.uid === settingsStore.homeDashboard),
                                }"
                                :path="
                                    item.uid === settingsStore.homeDashboard
                                        ? mdiHomeAnalytics
                                        : (item.mdiIcon ?? '')
                                "
                                :size="getREMSize(1.25)"
                            />
                            <span
                                class="ml-1.5"
                                :class="{
                                    'text-accent':
                                        router.currentRoute.value.params.dashboardUID ===
                                            item.uid ||
                                        (router.currentRoute.value.fullPath === '/' &&
                                            item.uid === settingsStore.homeDashboard),
                                }"
                            >
                                {{ item.label }}
                            </span>
                        </a>
                    </template>
                </Menu>
            </template>
        </el-dropdown>

        <!--Controls-->
        <router-link
            exact
            :to="{ name: 'system-controls' }"
            class="outline-none"
            v-slot="{ isActive }"
        >
            <Button
                id="controls"
                class="mt-4 ml-0.5 !rounded-lg border-none w-12 h-12 !p-0 text-text-color-secondary hover:text-text-color hover:bg-surface-hover outline-none"
                v-tooltip.right="t('layout.topbar.controls')"
            >
                <svg-icon
                    type="mdi"
                    :path="mdiTuneVerticalVariant"
                    :size="getREMSize(1.75)"
                    :class="{ 'text-accent': isActive }"
                />
            </Button>
        </router-link>

        <!--Modes Quick Menu-->
        <el-dropdown
            id="modes-quick"
            :show-timeout="0"
            :hide-timeout="100"
            :popper-options="{
                modifiers: [{ name: 'computeStyles', options: { gpuAcceleration: true } }],
            }"
            popper-class="ml-[3.75rem] mt-[-3.75rem]"
        >
            <Button
                class="mt-4 ml-0.5 !rounded-lg border-none text-text-color-secondary w-12 h-12 !p-0 hover:text-text-color hover:bg-surface-hover outline-none"
                @click="activatePreviousMode"
                v-tooltip.right="{
                    value: t('layout.topbar.modes'),
                    disabled: modesItems.length > 0,
                }"
            >
                <svg-icon
                    type="mdi"
                    :class="{ 'text-accent': settingsStore.modeActiveCurrent }"
                    :path="mdiBookmarkOutline"
                    :size="getREMSize(1.75)"
                />
            </Button>
            <template #dropdown>
                <Menu v-if="modesItems.length > 0" :model="modesItems" append-to="self">
                    <template #item="{ item, props }">
                        <a
                            v-bind="props.action"
                            class="inline-flex items-center px-0.5 w-full h-full"
                            :class="{ 'text-accent': item.isActive }"
                        >
                            <svg-icon
                                type="mdi"
                                :class="{
                                    'text-text-color-secondary/40':
                                        !item.isRecentlyActive && !item.isActive,
                                }"
                                :path="item.mdiIcon ?? ''"
                                :size="getREMSize(1.5)"
                            />
                            <span class="ml-1.5">{{ item.label }}</span>
                        </a>
                    </template>
                </Menu>
            </template>
        </el-dropdown>

        <!--Expand/Collapse Main Menu-->
        <Button
            v-if="!settingsStore.hideMenuCollapseIcon"
            id="collapse-menu"
            class="mt-2 ml-0.5 !rounded-lg border-none text-text-color-secondary w-12 h-12 !p-0 hover:text-text-color hover:bg-surface-hover outline-none"
            v-tooltip.right="{
                value: settingsStore.collapsedMainMenu
                    ? t('layout.topbar.expandMenu')
                    : t('layout.topbar.collapseMenu'),
            }"
            @click="emitter.emit('toggle-side-menu')"
        >
            <svg-icon
                type="mdi"
                :path="settingsStore.collapsedMainMenu ? mdiMenuClose : mdiMenuOpen"
                :size="getREMSize(1.75)"
            />
        </Button>

        <!--Alerts-->
        <router-link
            exact
            :to="{ name: 'alerts-overview' }"
            class="outline-none"
            v-slot="{ isActive }"
        >
            <Button
                id="alerts-quick"
                class="mt-4 ml-0.5 !rounded-lg border-none text-text-color-secondary w-12 h-12 !p-0 hover:text-text-color hover:bg-surface-hover outline-none"
                v-tooltip.right="t('layout.topbar.alerts')"
            >
                <OverlayBadge
                    v-if="numberOfActiveAlerts > 0"
                    :severity="'error'"
                    :value="numberOfActiveAlerts"
                >
                    <svg-icon
                        type="mdi"
                        :class="isActive ? 'text-accent' : 'text-error'"
                        :path="mdiBellRingOutline"
                        :size="getREMSize(1.75)"
                    />
                </OverlayBadge>
                <svg-icon
                    v-else
                    type="mdi"
                    :path="mdiBellOutline"
                    :size="getREMSize(1.75)"
                    :class="{ 'text-accent': isActive }"
                />
            </Button>
        </router-link>

        <!--Settings-->
        <router-link exact :to="{ name: 'settings' }" class="outline-none" v-slot="{ isActive }">
            <Button
                id="settings"
                class="mt-4 ml-0.5 !rounded-lg border-none w-12 h-12 !p-0 text-text-color-secondary hover:text-text-color hover:bg-surface-hover outline-none"
                v-tooltip.right="t('layout.topbar.settings')"
            >
                <svg-icon
                    type="mdi"
                    :path="mdiCogOutline"
                    :size="getREMSize(1.75)"
                    :class="{ 'text-accent': isActive }"
                />
            </Button>
        </router-link>

        <!--filler-->
        <div
            v-if="settingsStore.hideMenuCollapseIcon"
            class="flex-1 h-full cursor-pointer text-bg-two hover:text-text-color-secondary/50"
            @click="emitter.emit('toggle-side-menu')"
        >
            <div class="flex h-full items-center justify-center justify-items-center">
                <svg-icon
                    id="collapse-menu"
                    type="mdi"
                    :path="settingsStore.collapsedMainMenu ? mdiMenuClose : mdiMenuOpen"
                    :size="getREMSize(1.75)"
                />
            </div>
        </div>
        <div v-else class="flex-1 h-full" />

        <!--Open In Browser-->
        <a
            v-if="deviceStore.isQtApp()"
            href="http://localhost:11987"
            target="_blank"
            class="!outline-none"
        >
            <Button
                class="mt-4 ml-0.5 !rounded-lg border-none text-text-color-secondary w-12 h-12 !p-0 hover:text-text-color hover:bg-surface-hover outline-none"
                v-tooltip.right="t('layout.topbar.openInBrowser')"
            >
                <svg-icon type="mdi" :path="mdiOpenInNew" :size="getREMSize(1.5)" />
            </Button>
        </a>

        <!--Access Protection-->
        <el-dropdown
            id="access"
            ref="accessMenuRef"
            :show-timeout="0"
            :hide-timeout="100"
            :popper-options="{
                modifiers: [{ name: 'computeStyles', options: { gpuAcceleration: true } }],
            }"
            popper-class="ml-[3.75rem] mb-[-3.8rem]"
        >
            <Button
                class="mt-4 ml-0.5 !rounded-lg border-none text-text-color-secondary w-12 h-12 !p-0 hover:text-text-color hover:bg-surface-hover outline-none"
                aria-haspopup="true"
                aria-controls="access-overlay-menu"
            >
                <OverlayBadge v-if="!deviceStore.loggedIn" :severity="'error'">
                    <svg-icon type="mdi" :path="mdiAccountOffOutline" :size="getREMSize(1.75)" />
                </OverlayBadge>
                <svg-icon v-else type="mdi" :path="mdiAccountOutline" :size="getREMSize(1.75)" />
            </Button>
            <template #dropdown>
                <Menu :model="accessItems" append-to="self">
                    <!--                    <template #start>-->
                    <!--                        <span class="inline-flex align-items-center gap-1 px-2 py-2">-->
                    <!--                            <svg-icon-->
                    <!--                                class="text-text-color"-->
                    <!--                                type="mdi"-->
                    <!--                                :path="-->
                    <!--                                    deviceStore.loggedIn-->
                    <!--                                        ? mdiShieldLockOpenOutline-->
                    <!--                                        : mdiShieldLockOutline-->
                    <!--                                "-->
                    <!--                                :size="getREMSize(1.5)"-->
                    <!--                            />-->
                    <!--                            <span class="font-semibold ml-0.5">{{ accessLevel }}</span-->
                    <!--                            ><br />-->
                    <!--                        </span>-->
                    <!--                        <div class="px-1">-->
                    <!--                            <div class="border-b border-border-one" />-->
                    <!--                        </div>-->
                    <!--                    </template>-->
                </Menu>
            </template>
        </el-dropdown>

        <!--Power-->
        <el-dropdown
            id="restart"
            :show-timeout="0"
            :hide-timeout="100"
            :popper-options="{
                modifiers: [{ name: 'computeStyles', options: { gpuAcceleration: true } }],
            }"
            popper-class="ml-[3.75rem] mb-[-4rem]"
        >
            <Button
                class="mt-4 ml-0.5 !rounded-lg border-none text-text-color-secondary w-12 h-12 !p-0 hover:text-text-color hover:bg-surface-hover outline-none"
                @click="deviceStore.reloadUI()"
            >
                <svg-icon type="mdi" :path="mdiPower" :size="getREMSize(1.85)" />
            </Button>
            <template #dropdown>
                <Menu :model="restartItems" append-to="self" />
            </template>
        </el-dropdown>

        <!--bottom filler-->
        <div class="h-0.5" />
    </div>
</template>

<style></style>
