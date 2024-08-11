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
import {ref, computed} from 'vue'
import {useLayout} from '@/layout/composables/layout'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import {
    mdiLayersTripleOutline,
    mdiLightningBoltOutline,
    mdiLightningBolt,
    mdiOpenInNew,
    mdiCogOutline,
    mdiRestart,
    mdiPower,
    mdiPlus,
    mdiNotePlusOutline,
    mdiChartBoxPlusOutline,
    mdiLayersPlus,
    mdiPlusBoxMultipleOutline,
    mdiFlaskPlusOutline,
    mdiPlusCircleMultipleOutline,
    mdiTagPlusOutline,
    mdiSecurity,
    mdiShieldLockOutline,
    mdiShieldLockOpenOutline,
    mdiInformationOutline,
} from '@mdi/js'
import {useDeviceStore} from '@/stores/DeviceStore'
import {useSettingsStore} from '@/stores/SettingsStore'
import Button from 'primevue/button'
import Menu from 'primevue/menu'
import {ElDropdown} from 'element-plus'

const {onConfigButtonClick} = useLayout()
const {getREMSize} = useDeviceStore()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const logoUrl = `/logo.svg`

const modesMenu = ref()
const modesItems = computed(() => {
    const menuItems = []
    for (const mode of settingsStore.modes) {
        menuItems.push({
            label: mode.name,
            isActive: settingsStore.modeActive === mode.uid,
            mdiIcon: mdiLightningBolt,
            command: async () => {
                if (settingsStore.modeActive === mode.uid) {
                    return
                }
                await settingsStore.activateMode(mode.uid)
            },
        })
    }
    return menuItems
})

const accessLevel = computed(() => (deviceStore.loggedIn ? 'Write Access Enabled' : 'Read-only Access'))
const accessMenu = ref()
const accessItems = computed(() => [
    {
        label: 'Login',
        icon: 'pi pi-fw pi-sign-in',
        command: async () => {
            await deviceStore.login()
        },
        visible: !deviceStore.loggedIn,
    },
    {
        label: 'Logout',
        icon: 'pi pi-fw pi-sign-out',
        command: async () => {
            await deviceStore.logout()
        },
        visible: deviceStore.loggedIn,
    },
    {
        label: 'Change Password',
        icon: 'pi pi-fw pi-shield',
        command: async () => {
            await deviceStore.setPasswd()
        },
        visible: deviceStore.loggedIn,
    },
])

const restartItems = computed(() => [
    {
        label: 'Web UI',
        icon: 'pi pi-fw pi-desktop',
        command: () => {
            deviceStore.reloadUI()
        },
    },
    {
        label: 'Daemon and UI',
        icon: 'pi pi-fw pi-server',
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

const addItems = computed(() => [
    {
        label: 'Overview',
        mdiIcon: mdiChartBoxPlusOutline,
    },
    {
        label: 'Mode',
        mdiIcon: mdiLayersPlus,
    },
    {
        label: 'Profile',
        mdiIcon: mdiPlusBoxMultipleOutline,
    },
    {
        label: 'Function',
        mdiIcon: mdiFlaskPlusOutline,
    },
    {
        label: 'Custom Sensor',
        mdiIcon: mdiPlusCircleMultipleOutline,
    },
    {
        label: 'Tag',
        mdiIcon: mdiTagPlusOutline,
    },
])
</script>

<template>
    <div class="flex flex-col h-full align-middle justify-items-center">
        <router-link to="/" class="">
            <img :src="logoUrl" alt="logo"/>
        </router-link>

        <!--Add-->
        <el-dropdown popper-class="ml-[3.68rem] mt-[-3.75rem]">
            <Button
                class="mt-3 !rounded-lg border-none w-12 h-12 !p-0 text-text-color-secondary
                        hover:text-text-color-primary hover:bg-surface-hover"
                aria-haspopup="true"
                aria-controls="modes-overlay-menu"
            >
                <svg-icon class="text-text-color-secondary" type="mdi"
                          :path="mdiPlus" :size="getREMSize(2.0)"/>
            </Button>
            <template #dropdown>
                <Menu :model="addItems">
                    <template #start>
                    <span class="inline-flex align-items-center gap-1 px-2 py-2">
                        <svg-icon class="text-text-color" type="mdi"
                                  :path="mdiNotePlusOutline" :size="getREMSize(1.5)"/>
                        <span class="font-semibold ml-0.5">New</span><br/>
                    </span>
                        <div class="px-1">
                            <div class="border-b border-border-one"/>
                        </div>
                    </template>
                    <template #item="{ item, props }">
                        <a tabindex="-1" aria-hidden="true" data-pc-section="action">
                        <span class="inline-flex align-items-center px-0.5">
                            <svg-icon class="text-text-color-secondary" type="mdi"
                                      :path="item.mdiIcon" :size="getREMSize(1.5)"/>
                            <span class="ml-1.5">{{ item.label }}</span>
                        </span>
                        </a>
                    </template>
                </Menu>
            </template>
        </el-dropdown>

        <div class="px-1 mt-3">
            <div class="border-b-2 border-border-one"/>
        </div>

        <!--Settings-->
        <Button @click="onConfigButtonClick()"
                class="mt-4 !rounded-lg h-12 w-12 !p-0 border-none text-text-color-secondary
                hover:text-text-color-primary hover:bg-surface-hover"
                v-tooltip.right="{ value: 'Settings' }"
        >
            <svg-icon type="mdi" :path="mdiCogOutline" :size="getREMSize(1.75)"/>
        </Button>

        <!--Modes-->
        <el-dropdown popper-class="ml-[3.68rem] mt-[-3.75rem]">
            <!--                v-if="settingsStore.modes.length > 0"-->
            <Button
                class="mt-4 !rounded-lg border-none w-12 h-12 !p-0 text-text-color-secondary
                        hover:text-text-color-primary hover:bg-surface-hover"
                aria-haspopup="true"
                aria-controls="modes-overlay-menu"
            >
                <svg-icon type="mdi" :path="mdiLayersTripleOutline" :size="getREMSize(1.75)"/>
            </Button>
            <template #dropdown>
                <Menu ref="modesMenu" :model="modesItems">
                    <template #start>
                    <span class="inline-flex align-items-center gap-1 px-2 py-2">
                        <svg-icon class="text-text-color" type="mdi"
                                  :path="mdiLightningBoltOutline" :size="getREMSize(1.5)"/>
                        <span class="font-semibold ml-0.5">Activate Mode</span><br/>
                    </span>
                        <div class="px-1">
                            <div class="border-b border-border-one"/>
                        </div>
                    </template>
                    <template #item="{ item, props }">
                        <a tabindex="-1" aria-hidden="true" data-pc-section="action">
                        <span class="inline-flex align-items-center px-0.5">
                            <svg-icon type="mdi"
                                      :class="[item.isActive ? 'text-accent' : 'text-accent/0']"
                                      :path="item.mdiIcon" :size="getREMSize(1.5)"/>
                            <span class="ml-1.5">{{ item.label }}</span>
                        </span>
                        </a>
                    </template>
                    <!--<template #item="{ item, props }">-->
                    <!--    <a tabindex="-1" aria-hidden="true" data-pc-section="action">-->
                    <!--        <span :class="item.icon"/>-->
                    <!--        <span class="ml-2">{{ item.label }}</span>-->
                    <!--    </a>-->
                    <!--</template>-->
                </Menu>
            </template>
        </el-dropdown>

        <!--Access Protection-->
        <el-dropdown popper-class="ml-[3.68rem] mt-[-3.75rem]">
            <Button
                class="mt-4 !rounded-lg border-none text-text-color-secondary w-12 h-12 !p-0
            hover:text-text-color-primary hover:bg-surface-hover"
                aria-haspopup="true"
                aria-controls="access-overlay-menu"
            >
                <svg-icon type="mdi" :path="mdiSecurity" :size="getREMSize(1.75)"/>
            </Button>
            <template #dropdown>
                <Menu ref="accessMenu" :model="accessItems">
                    <template #start>
                    <span class="inline-flex align-items-center gap-1 px-2 py-2">
                        <svg-icon class="text-text-color" type="mdi"
                                  :path="deviceStore.loggedIn ? mdiShieldLockOpenOutline: mdiShieldLockOutline"
                                  :size="getREMSize(1.5)"/>
                        <span class="font-semibold ml-0.5">{{ accessLevel }}</span><br/>
                    </span>
                        <div class="px-1">
                            <div class="border-b border-border-one"/>
                        </div>
                    </template>
                </Menu>
            </template>
        </el-dropdown>

        <!--Open In Browser-->
        <el-dropdown popper-class="ml-[3.68rem] mt-[-3.75rem]">
            <a href="http://localhost:11987" target="_blank" class="!outline-none">
                <Button
                    class="mt-4 !rounded-lg border-none text-text-color-secondary w-12 h-12 !p-0
                            hover:text-text-color-primary hover:bg-surface-hover"
                >
                    <svg-icon type="mdi" :path="mdiOpenInNew" :size="getREMSize(1.75)"/>
                </Button>
            </a>
            <template #dropdown>
                <Menu :model="externalLinkItems">
                </Menu>
            </template>
        </el-dropdown>

        <!--Reload-->
        <el-dropdown popper-class="ml-[3.68rem] mt-[-3.75rem]">
            <Button
                class="mt-4 !rounded-lg border-none text-text-color-secondary w-12 h-12 !p-0
                        hover:text-text-color-primary hover:bg-surface-hover"
            >
                <svg-icon type="mdi" :path="mdiPower" :size="getREMSize(1.85)"/>
            </Button>
            <template #dropdown>
                <Menu :model="restartItems">
                    <template #start>
                    <span class="inline-flex align-items-center gap-1 px-2 py-2">
                        <svg-icon class="text-text-color" type="mdi"
                                  :path="mdiRestart" :size="getREMSize(1.5)"/>
                        <span class="font-semibold ml-0.5">Restart</span><br/>
                    </span>
                        <div class="px-1">
                            <div class="border-b border-border-one"/>
                        </div>
                    </template>
                </Menu>
                <!--<Button @click="deviceStore.reloadUI()"-->
                <!--        class="!rounded-lg hover:!text-text-color-primary hover:!bg-border-one-->
                <!--        !bg-bg-two !text-text-color border !border-border-one-->
                <!--        !py-3"-->
                <!-- >-->
                <!--    Restart UI-->
                <!--</Button>-->
            </template>
        </el-dropdown>

        <!--Info-->
        <div class="flex-1 h-full"/>
        <Button
            class="mt-auto !rounded-lg border-none text-text-color-secondary w-12 h-12 !p-0
                    hover:text-text-color-primary hover:bg-surface-hover"
            v-tooltip.right="{ value: 'System Info' }">
            <svg-icon :class="['text-green']" type="mdi" :path="mdiInformationOutline" :size="getREMSize(1.75)"/>
        </Button>
    </div>
</template>

<style>
</style>
