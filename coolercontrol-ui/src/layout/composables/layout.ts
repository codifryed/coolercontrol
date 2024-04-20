/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

import { computed, reactive, toRefs } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore'

const layoutConfig = reactive({
    ripple: true,
    inputStyle: 'outlined',
    menuMode: 'static',
    scale: 100, // %
    activeMenuItem: null,
})

const layoutState = reactive({
    staticMenuDesktopInactive: false,
    overlayMenuActive: false,
    profileSidebarVisible: false,
    configSidebarVisible: false,
    staticMenuMobileActive: false,
    menuHoverActive: false,
})

export function useLayout() {
    const setScale = (scale: number): void => {
        layoutConfig.scale = scale
        console.debug('New Font Size: ' + scale)
        document.documentElement.style.fontSize = layoutConfig.scale + '%'
        useDeviceStore().fontScale = layoutConfig.scale
    }

    const onConfigButtonClick = () => {
        layoutState.configSidebarVisible = !layoutState.configSidebarVisible
    }

    const onMenuToggle = () => {
        if (layoutConfig.menuMode === 'overlay') {
            layoutState.overlayMenuActive = !layoutState.overlayMenuActive
        }

        if (window.innerWidth > 991) {
            layoutState.staticMenuDesktopInactive = !layoutState.staticMenuDesktopInactive
        } else {
            layoutState.staticMenuMobileActive = !layoutState.staticMenuMobileActive
        }
    }

    const isSidebarActive = computed(
        () => layoutState.overlayMenuActive || layoutState.staticMenuMobileActive,
    )

    const isConfigSidebarActive = computed({
        get() {
            return layoutState.configSidebarVisible
        },
        set(isVisible: boolean): void {
            layoutState.configSidebarVisible = isVisible
        },
    })

    return {
        layoutConfig: toRefs(layoutConfig),
        layoutState: toRefs(layoutState),
        setScale,
        onMenuToggle,
        isSidebarActive,
        isConfigSidebarActive,
        onConfigButtonClick,
    }
}
