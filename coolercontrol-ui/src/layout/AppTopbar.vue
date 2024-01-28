<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2023  Guy Boldon
  - |
  - This program is free software: you can redistribute it and/or modify
  - it under the terms of the GNU General Public License as published by
  - the Free Software Foundation, either version 3 of the License, or
  - (at your option) any later version.
  - |
  - This program is distributed in the hope that it will be useful,
  - but WITHOUT ANY WARRANTY; without even the implied warranty of
  - MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  - GNU General Public License for more details.
  - |
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
    mdiMenu,
    mdiOpenInNew,
    mdiTune,
} from '@mdi/js'
import { useDeviceStore } from '@/stores/DeviceStore'
import Button from 'primevue/button'
import Menu from 'primevue/menu'

const { layoutConfig, onMenuToggle, onConfigButtonClick } = useLayout()
const { getREMSize } = useDeviceStore()

const outsideClickListener = ref(null)
const topbarMenuActive = ref(false)
const router = useRouter()
const deviceStore = useDeviceStore()

onMounted(() => {
    bindOutsideClickListener()
})

onBeforeUnmount(() => {
    unbindOutsideClickListener()
})

const logoUrl = computed(() => {
    return `/layout/images/${layoutConfig.darkTheme.value ? 'logo-dark' : 'logo-dark'}.svg`
})

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

const accessLevel = computed(() => (deviceStore.loggedIn ? 'Admin Access' : 'Guest Access'))
const accessMenu = ref()
const accessItems = computed(() => [
    {
        label: 'Login',
        icon: 'pi pi-fw pi-sign-in',
        command: async () => {
            await deviceStore.login()
        },
        disabled: deviceStore.loggedIn,
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
