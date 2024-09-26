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
import { ref, computed, inject } from 'vue'
import { useLayout } from '@/layout/composables/layout'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import {
    // mdiLayersTripleOutline,
    // mdiLightningBoltOutline,
    // mdiLightningBolt,
    mdiOpenInNew,
    mdiCogOutline,
    mdiPower,
    mdiPlus,
    mdiChartBoxPlusOutline,
    mdiLayersPlus,
    mdiPlusBoxMultipleOutline,
    mdiFlaskPlusOutline,
    mdiPlusCircleMultipleOutline,
    mdiTagPlusOutline,
    mdiAccountBadgeOutline,
    mdiAccountOffOutline,
} from '@mdi/js'
import { useDeviceStore } from '@/stores/DeviceStore'
import Button from 'primevue/button'
import Menu from 'primevue/menu'
import { type DropdownInstance, ElDropdown } from 'element-plus'
import { Emitter, EventType } from 'mitt'
import { useRouter } from 'vue-router'

const { onConfigButtonClick } = useLayout()
const { getREMSize } = useDeviceStore()

const deviceStore = useDeviceStore()
const router = useRouter()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!

const logoUrl = `/logo.svg`

// const settingsStore = useSettingsStore()
// const modesMenu = ref()
// const modesItems = computed(() => {
//     const menuItems = []
//     for (const mode of settingsStore.modes) {
//         menuItems.push({
//             label: mode.name,
//             isActive: settingsStore.modeActive === mode.uid,
//             mdiIcon: mdiLightningBolt,
//             command: async () => {
//                 if (settingsStore.modeActive === mode.uid) {
//                     return
//                 }
//                 await settingsStore.activateMode(mode.uid)
//             },
//         })
//     }
//     return menuItems
// })

const settingsMenuRef = ref<DropdownInstance>()
const settingsItems = computed(() => [
    {
        label: 'Settings',
        icon: 'pi pi-fw pi-sliders-h',
        command: () => {
            settingsMenuRef.value?.handleClose()
            router.push({ name: 'settings' })
        },
    },
])
const accessMenuRef = ref<DropdownInstance>()
const accessItems = computed(() => [
    {
        label: 'Login',
        icon: 'pi pi-fw pi-sign-in',
        command: async () => {
            accessMenuRef.value?.handleClose()
            await deviceStore.login()
        },
        visible: !deviceStore.loggedIn,
    },
    {
        label: 'Logout',
        icon: 'pi pi-fw pi-sign-out',
        command: async () => {
            accessMenuRef.value?.handleClose()
            await deviceStore.logout()
        },
        visible: deviceStore.loggedIn,
    },
    {
        label: 'Change Password',
        icon: 'pi pi-fw pi-shield',
        command: async () => {
            accessMenuRef.value?.handleClose()
            await deviceStore.setPasswd()
        },
        visible: deviceStore.loggedIn,
    },
])

const restartItems = computed(() => [
    {
        label: 'Restart Web UI',
        icon: 'pi pi-fw pi-refresh',
        command: () => {
            deviceStore.reloadUI()
        },
    },
    {
        label: 'Restart Daemon and UI',
        icon: 'pi pi-fw pi-sync',
        command: async () => {
            await deviceStore.daemonClient.shutdownDaemon()
            await deviceStore.waitAndReload(1)
        },
    },
])
const externalLinkItems = computed(() => [
    {
        label: deviceStore.isTauriApp() ? 'Open in Browser' : 'Open in New Tab',
        icon: 'pi pi-fw pi-external-link',
        url: 'http://localhost:11987',
        target: '_blank',
    },
])

const addMenuRef = ref<DropdownInstance>()
const addItems = computed(() => [
    {
        label: 'Dashboard',
        mdiIcon: mdiChartBoxPlusOutline,
        command: () => {
            addMenuRef.value?.handleClose()
            emitter.emit('dashboard-add')
        },
    },
    {
        label: 'Mode',
        mdiIcon: mdiLayersPlus,
        command: () => {
            addMenuRef.value?.handleClose()
        },
    },
    {
        label: 'Profile',
        mdiIcon: mdiPlusBoxMultipleOutline,
        command: () => {
            addMenuRef.value?.handleClose()
        },
    },
    {
        label: 'Function',
        mdiIcon: mdiFlaskPlusOutline,
        command: () => {
            addMenuRef.value?.handleClose()
        },
    },
    {
        label: 'Custom Sensor',
        mdiIcon: mdiPlusCircleMultipleOutline,
        command: () => {
            addMenuRef.value?.handleClose()
            router.push({ name: 'custom-sensors' })
        },
    },
    {
        label: 'Tag',
        mdiIcon: mdiTagPlusOutline,
        command: () => {
            addMenuRef.value?.handleClose()
        },
    },
])
</script>

<template>
    <div class="flex flex-col h-full align-middle justify-items-center">
        <Button
            class="mt-auto !rounded-lg border-none text-text-color-secondary w-12 h-12 !p-0 hover:text-text-color hover:bg-surface-hover"
            v-tooltip.right="{ value: 'System Info' }"
        >
            <router-link to="/" class="">
                <img :src="logoUrl" alt="logo" />
            </router-link>
        </Button>

        <!--Add-->
        <el-dropdown
            ref="addMenuRef"
            :show-timeout="100"
            :hide-timeout="100"
            :popper-options="{
                modifiers: [{ name: 'computeStyles', options: { gpuAcceleration: true } }],
            }"
            popper-class="ml-[3.68rem] mt-[-3.75rem]"
        >
            <Button
                class="mt-3 !rounded-lg border-none w-12 h-12 !p-0 text-text-color-secondary hover:text-text-color hover:bg-surface-hover outline-0"
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

        <!--Settings-->
        <el-dropdown
            ref="settingsMenuRef"
            :show-timeout="100"
            :hide-timeout="100"
            :popper-options="{
                modifiers: [{ name: 'computeStyles', options: { gpuAcceleration: true } }],
            }"
            popper-class="ml-[3.68rem] mt-[-3.75rem]"
        >
            <router-link :to="{ name: 'settings' }">
                <Button
                    class="mt-4 !rounded-lg border-none text-text-color-secondary w-12 h-12 !p-0 hover:text-text-color hover:bg-surface-hover outline-0"
                >
                    <svg-icon type="mdi" :path="mdiCogOutline" :size="getREMSize(1.75)" />
                </Button>
            </router-link>
            <template #dropdown>
                <Menu :model="settingsItems" append-to="self" />
            </template>
        </el-dropdown>

        <!--Modes-->
        <!--        <el-dropdown-->
        <!--            :show-timeout="100"-->
        <!--            :hide-timeout="100"-->
        <!--            :popper-options="{modifiers: [{name: 'computeStyles',options: {gpuAcceleration: true}}]}"-->
        <!--            popper-class="ml-[3.68rem] mt-[-3.75rem]"-->
        <!--        >-->
        <!--            &lt;!&ndash;                v-if="settingsStore.modes.length > 0"&ndash;&gt;-->
        <!--            <Button-->
        <!--                class="mt-4 !rounded-lg border-none w-12 h-12 !p-0 text-text-color-secondary hover:text-text-color hover:bg-surface-hover"-->
        <!--                aria-haspopup="true"-->
        <!--                aria-controls="modes-overlay-menu"-->
        <!--            >-->
        <!--                <svg-icon type="mdi" :path="mdiLayersTripleOutline" :size="getREMSize(1.75)" />-->
        <!--            </Button>-->
        <!--            <template #dropdown>-->
        <!--                <Menu ref="modesMenu" :model="modesItems" append-to="self">-->
        <!--                    <template #start>-->
        <!--                        <span class="inline-flex align-items-center gap-1 px-2 py-2">-->
        <!--                            <svg-icon-->
        <!--                                class="text-text-color"-->
        <!--                                type="mdi"-->
        <!--                                :path="mdiLightningBoltOutline"-->
        <!--                                :size="getREMSize(1.5)"-->
        <!--                            />-->
        <!--                            <span class="font-semibold ml-0.5">Activate Mode</span><br />-->
        <!--                        </span>-->
        <!--                        <div class="px-1">-->
        <!--                            <div class="border-b border-border-one" />-->
        <!--                        </div>-->
        <!--                    </template>-->
        <!--                    <template #item="{ item }">-->
        <!--                        <a tabindex="-1" aria-hidden="true" data-pc-section="action">-->
        <!--                            <span class="inline-flex align-items-center px-0.5">-->
        <!--                                <svg-icon-->
        <!--                                    type="mdi"-->
        <!--                                    :class="[item.isActive ? 'text-accent' : 'text-accent/0']"-->
        <!--                                    :path="item.mdiIcon"-->
        <!--                                    :size="getREMSize(1.5)"-->
        <!--                                />-->
        <!--                                <span class="ml-1.5">{{ item.label }}</span>-->
        <!--                            </span>-->
        <!--                        </a>-->
        <!--                    </template>-->
        <!--                    &lt;!&ndash;<template #item="{ item, props }">&ndash;&gt;-->
        <!--                    &lt;!&ndash;    <a tabindex="-1" aria-hidden="true" data-pc-section="action">&ndash;&gt;-->
        <!--                    &lt;!&ndash;        <span :class="item.icon"/>&ndash;&gt;-->
        <!--                    &lt;!&ndash;        <span class="ml-2">{{ item.label }}</span>&ndash;&gt;-->
        <!--                    &lt;!&ndash;    </a>&ndash;&gt;-->
        <!--                    &lt;!&ndash;</template>&ndash;&gt;-->
        <!--                </Menu>-->
        <!--            </template>-->
        <!--        </el-dropdown>-->

        <!--Access Protection-->
        <el-dropdown
            ref="accessMenuRef"
            :show-timeout="100"
            :hide-timeout="100"
            :popper-options="{
                modifiers: [{ name: 'computeStyles', options: { gpuAcceleration: true } }],
            }"
            popper-class="ml-[3.68rem] mt-[-3.75rem]"
        >
            <Button
                class="mt-4 !rounded-lg border-none text-text-color-secondary w-12 h-12 !p-0 hover:text-text-color hover:bg-surface-hover outline-0"
                aria-haspopup="true"
                aria-controls="access-overlay-menu"
            >
                <svg-icon
                    type="mdi"
                    :path="deviceStore.loggedIn ? mdiAccountBadgeOutline : mdiAccountOffOutline"
                    :size="getREMSize(1.75)"
                />
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

        <!--Open In Browser-->
        <el-dropdown
            :show-timeout="100"
            :hide-timeout="100"
            :popper-options="{
                modifiers: [{ name: 'computeStyles', options: { gpuAcceleration: true } }],
            }"
            popper-class="ml-[3.68rem] mt-[-3.75rem]"
        >
            <a href="http://localhost:11987" target="_blank" class="!outline-none">
                <Button
                    class="mt-4 !rounded-lg border-none text-text-color-secondary w-12 h-12 !p-0 hover:text-text-color hover:bg-surface-hover outline-0"
                >
                    <svg-icon type="mdi" :path="mdiOpenInNew" :size="getREMSize(1.75)" />
                </Button>
            </a>
            <template #dropdown>
                <Menu :model="externalLinkItems" append-to="self" />
            </template>
        </el-dropdown>

        <!--Reload-->
        <el-dropdown
            :show-timeout="100"
            :hide-timeout="100"
            :popper-options="{
                modifiers: [{ name: 'computeStyles', options: { gpuAcceleration: true } }],
            }"
            popper-class="ml-[3.68rem] mt-[-3.75rem]"
        >
            <Button
                class="mt-4 !rounded-lg border-none text-text-color-secondary w-12 h-12 !p-0 hover:text-text-color hover:bg-surface-hover outline-0"
            >
                <svg-icon type="mdi" :path="mdiPower" :size="getREMSize(1.85)" />
            </Button>
            <template #dropdown>
                <Menu :model="restartItems" append-to="self" />
            </template>
        </el-dropdown>

        <!--bottom filler-->
        <div class="flex-1 h-full" />
    </div>
</template>

<style></style>
