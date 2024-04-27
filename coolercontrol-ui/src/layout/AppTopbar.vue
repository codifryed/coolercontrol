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
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { useLayout } from '@/layout/composables/layout'
import { useRouter } from 'vue-router'
import SvgIcon from '@jamescoyle/vue-icon'
import {
    mdiAccountOutline,
    mdiShieldAccountOutline,
    mdiDotsVertical,
    mdiGitlab,
    mdiLayersTripleOutline,
    mdiMenu,
    mdiOpenInNew,
    mdiTune,
    mdiRefresh,
} from '@mdi/js'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import Button from 'primevue/button'
import Menu from 'primevue/menu'

const { onMenuToggle, onConfigButtonClick } = useLayout()
const { getREMSize } = useDeviceStore()

const outsideClickListener = ref(null)
const topbarMenuActive = ref(false)
const router = useRouter()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

onMounted(() => {
    bindOutsideClickListener()
})

onBeforeUnmount(() => {
    unbindOutsideClickListener()
})

const logoUrl = `/logo.svg`

const onTopBarMenuButton = () => {
    topbarMenuActive.value = !topbarMenuActive.value
}

// const onSettingsClick = () => {
//     topbarMenuActive.value = false;
//     router.push('/documentation');
// };

const topbarMenuClasses = computed(() => {
    return {
        'layout-topbar-menu-mobile-active': topbarMenuActive.value,
    }
})

const bindOutsideClickListener = () => {
    if (!outsideClickListener.value) {
        outsideClickListener.value = (event) => {
            if (isOutsideClicked(event)) {
                topbarMenuActive.value = false
            }
        }
        document.addEventListener('click', outsideClickListener.value)
    }
}
const unbindOutsideClickListener = () => {
    if (outsideClickListener.value) {
        document.removeEventListener('click', outsideClickListener)
        outsideClickListener.value = null
    }
}
const isOutsideClicked = (event) => {
    if (!topbarMenuActive.value) return

    const sidebarEl = document.querySelector('.layout-topbar-menu')
    const topbarEl = document.querySelector('.layout-topbar-menu-button')

    return !(
        sidebarEl.isSameNode(event.target) ||
        sidebarEl.contains(event.target) ||
        topbarEl.isSameNode(event.target) ||
        topbarEl.contains(event.target)
    )
}

const modesMenu = ref()
const modesItems = computed(() => {
    const menuItems = []
    for (const mode of settingsStore.modes) {
        menuItems.push({
            label: mode.name,
            icon: settingsStore.modeActive === mode.uid ? 'pi pi-fw pi-check' : 'pi pi-fw',
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
const toggleModesMenu = (event) => {
    modesMenu.value.toggle(event)
}

const accessLevel = computed(() => (deviceStore.loggedIn ? 'Admin Access' : 'Guest Access'))
const accessMenu = ref()
const accessItems = computed(() => [
    {
        separator: true,
    },
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
        label: 'Set New Password',
        icon: 'pi pi-fw pi-shield',
        command: async () => {
            await deviceStore.setPasswd()
        },
        disabled: !deviceStore.loggedIn,
    },
])
const toggleAccessMenu = (event) => {
    accessMenu.value.toggle(event)
}
</script>

<template>
    <div class="layout-topbar">
        <button class="p-link layout-menu-button layout-topbar-button" @click="onMenuToggle()">
            <svg-icon type="mdi" :path="mdiMenu" :size="getREMSize(1.5)" />
        </button>

        <button
            class="p-link layout-topbar-menu-button layout-topbar-button"
            @click="onTopBarMenuButton()"
        >
            <svg-icon type="mdi" :path="mdiDotsVertical" :size="getREMSize(1.5)" />
        </button>

        <div class="layout-topbar-logo">
            <router-link to="/" class="layout-topbar-logo">
                <img :src="logoUrl" alt="logo" />
                <span style="font-family: rounded, serif">CoolerControl</span>
            </router-link>
        </div>

        <div class="layout-topbar-menu" :class="topbarMenuClasses">
            <Button
                class="p-link layout-topbar-button"
                @click="deviceStore.reloadUI()"
                v-tooltip.bottom="{ value: 'Reload the UI', showDelay: 500 }"
            >
                <svg-icon type="mdi" :path="mdiRefresh" :size="getREMSize(1.5)" />
                <span>Reload</span>
            </Button>
            <a href="http://localhost:11987" target="_blank" v-if="deviceStore.isTauriApp()">
                <Button
                    class="p-link layout-topbar-button"
                    v-tooltip.bottom="{ value: 'Open UI in browser window', showDelay: 500 }"
                >
                    <svg-icon type="mdi" :path="mdiOpenInNew" :size="getREMSize(1.5)" />
                    <span>Open UI in Browser</span>
                </Button>
            </a>
            <a href="https://gitlab.com/coolercontrol/coolercontrol" target="_blank">
                <Button
                    class="p-link layout-topbar-button"
                    v-tooltip.bottom="{ value: 'GitLab Project Page', showDelay: 500 }"
                >
                    <svg-icon type="mdi" :path="mdiGitlab" :size="getREMSize(1.5)" />
                    <span>Project Page</span>
                </Button>
            </a>
            <Button
                v-if="settingsStore.modes.length > 0"
                class="p-link layout-topbar-button"
                v-tooltip.bottom="{ value: 'Modes', showDelay: 500 }"
                @click="toggleModesMenu"
                aria-haspopup="true"
                aria-controls="modes-overlay-menu"
            >
                <svg-icon type="mdi" :path="mdiLayersTripleOutline" :size="getREMSize(1.5)" />
                <span>Modes</span>
            </Button>
            <Menu
                ref="modesMenu"
                id="modes-overlay-menu"
                :model="modesItems"
                :popup="true"
                style="width: auto"
            >
                <template #item="{ item, props }">
                    <a
                        class="p-menuitem-link"
                        tabindex="-1"
                        aria-hidden="true"
                        data-pc-section="action"
                        data-pd-ripple="true"
                    >
                        <span :class="item.icon" />
                        <span class="ml-2">{{ item.label }}</span>
                    </a>
                </template>
            </Menu>
            <Button
                class="p-link layout-topbar-button"
                v-tooltip.bottom="{ value: 'Access Protection', showDelay: 500 }"
                @click="toggleAccessMenu"
                aria-haspopup="true"
                aria-controls="access-overlay-menu"
            >
                <svg-icon type="mdi" :path="mdiShieldAccountOutline" :size="getREMSize(1.5)" />
                <span>Access Protection</span>
            </Button>
            <Menu ref="accessMenu" id="access-overlay-menu" :model="accessItems" :popup="true">
                <template #start>
                    <span class="inline-flex align-items-center gap-1 px-2 py-2">
                        <svg-icon type="mdi" :path="mdiAccountOutline" :size="getREMSize(1.5)" />
                        <span class="font-semibold">{{ accessLevel }} </span>
                    </span>
                </template>
            </Menu>
            <Button @click="onConfigButtonClick()" class="p-link layout-topbar-button">
                <svg-icon type="mdi" :path="mdiTune" :size="getREMSize(1.5)" />
                <span>Settings</span>
            </Button>
        </div>
    </div>
</template>

<style lang="scss" scoped></style>
