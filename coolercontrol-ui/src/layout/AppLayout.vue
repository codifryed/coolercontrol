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
import { computed, watch, ref } from 'vue'
import AppTopbar from './AppTopbar.vue'
import AppSidebar from './AppSidebar.vue'
import AppConfig from './AppConfig.vue'
import { useLayout } from '@/layout/composables/layout'

const { layoutConfig, layoutState, isSidebarActive } = useLayout()

const outsideClickListener = ref(null)

watch(isSidebarActive, (newVal) => {
    if (newVal) {
        bindOutsideClickListener()
    } else {
        unbindOutsideClickListener()
    }
})

const containerClass = computed(() => {
    return {
        'layout-overlay': layoutConfig.menuMode.value === 'overlay',
        'layout-static': layoutConfig.menuMode.value === 'static',
        'layout-static-inactive':
            layoutState.staticMenuDesktopInactive.value && layoutConfig.menuMode.value === 'static',
        'layout-overlay-active': layoutState.overlayMenuActive.value,
        'layout-mobile-active': layoutState.staticMenuMobileActive.value,
        'p-input-filled': layoutConfig.inputStyle.value === 'filled',
        'p-ripple-disabled': !layoutConfig.ripple.value,
    }
})
const bindOutsideClickListener = () => {
    if (!outsideClickListener.value) {
        outsideClickListener.value = (event) => {
            if (isOutsideClicked(event)) {
                layoutState.overlayMenuActive.value = false
                layoutState.staticMenuMobileActive.value = false
                layoutState.menuHoverActive.value = false
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
    const sidebarEl = document.querySelector('.layout-sidebar')
    const topbarEl = document.querySelector('.layout-menu-button')

    return !(
        sidebarEl.isSameNode(event.target) ||
        sidebarEl.contains(event.target) ||
        topbarEl.isSameNode(event.target) ||
        topbarEl.contains(event.target)
    )
}
</script>

<template>
    <div class="layout-wrapper" :class="containerClass">
        <app-topbar></app-topbar>
        <div class="layout-sidebar">
            <app-sidebar></app-sidebar>
        </div>
        <div class="layout-main-container">
            <div class="layout-main" ref="laymain">
                <router-view v-slot="{ Component, route }">
                    <!--          <transition name="fade">-->
                    <component :is="Component" :key="route.path" />
                    <!--          </transition>-->
                </router-view>
            </div>
        </div>
        <app-config></app-config>
        <div class="layout-mask"></div>
    </div>
</template>

<style lang="scss" scoped>
// todo: perhaps I can get this to work 'properly' someday:
//.fade-enter-active,
//.fade-leave-active {
//  transition: all 0.3s ease;
//}
//
//.fade-enter-from,
//.fade-leave-to {
//  opacity: 0;
//}
</style>
