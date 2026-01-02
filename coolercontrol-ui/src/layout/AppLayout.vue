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
import { mdiMenu } from '@mdi/js'
import {
    ScrollAreaRoot,
    ScrollAreaScrollbar,
    ScrollAreaThumb,
    ScrollAreaViewport,
    SplitterGroup,
    SplitterPanel,
    SplitterResizeHandle,
} from 'radix-vue'
import AppSideTopbar from '@/layout/AppSideTopbar.vue'
import AppTreeMenu from '@/layout/AppTreeMenu.vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { computed, inject, onMounted, Ref, ref, watch } from 'vue'
import { Emitter, EventType } from 'mitt'
import { useWindowSize } from '@vueuse/core'
import Button from 'primevue/button'
import Drawer from 'primevue/drawer'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const menuPanelRef = ref<InstanceType<typeof SplitterPanel>>()
const splitterGroupRef = ref<InstanceType<typeof SplitterGroup>>()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!
const minMenuWidthRem: number = 14
const minViewWidthRem: number = 18
const splitterGroupWidthPx: Ref<number> = ref(1900)
const { width } = useWindowSize()

const calculateSplitterWidthPercent = (rem: number): number =>
    (deviceStore.getREMSize(rem) / splitterGroupWidthPx.value) * 100
const menuPanelWidthPercent = ref(calculateSplitterWidthPercent(settingsStore.mainMenuWidthRem))
const minMenuWidthPx: number = minMenuWidthRem * deviceStore.getREMSize(1)

const calculateMenuRemWidth = (percent: number): number => {
    const widthPx = (percent / 100) * splitterGroupWidthPx.value
    const widthRem = widthPx / deviceStore.getREMSize(1)
    return Math.round(widthRem * 10) / 10
}
const menuPanelMinWidth = computed((): number =>
    Math.min(calculateSplitterWidthPercent(minMenuWidthRem), 50),
)
const viewPanelMinWidth = computed((): number =>
    Math.min(calculateSplitterWidthPercent(minViewWidthRem), 50),
)
let onResize = (_: number): void => {
    // overridden after being mounted to avoid pre-mount issues
}

const toggleSideMenu = (): void => {
    menuPanelRef.value?.isCollapsed ? menuPanelRef.value?.expand() : menuPanelRef.value?.collapse()
    settingsStore.collapsedMainMenu = menuPanelRef.value?.isCollapsed ?? false
}
emitter.on('toggle-side-menu', toggleSideMenu)
const collapseSideMenu = (): void => {
    if (menuPanelRef.value?.isCollapsed) return
    menuPanelRef.value?.collapse()
    settingsStore.collapsedMainMenu = menuPanelRef.value?.isCollapsed ?? false
}
emitter.on('collapse-side-menu', collapseSideMenu)
const expandSideMenu = (): void => {
    if (!menuPanelRef.value?.isCollapsed) return
    menuPanelRef.value?.expand()
    settingsStore.collapsedMainMenu = menuPanelRef.value?.isCollapsed ?? false
}
emitter.on('expand-side-menu', expandSideMenu)

const drawerVisible = ref(false)
const isMobile = computed(() => width.value < 768)
let resizeObserver: ResizeObserver | null = null

const initDesktopMenu = (): void => {
    const splitterEl: HTMLElement | undefined = splitterGroupRef.value?.$el
    if (!splitterEl) return
    splitterGroupWidthPx.value = splitterEl.getBoundingClientRect().width
    menuPanelWidthPercent.value = calculateSplitterWidthPercent(settingsStore.mainMenuWidthRem)
    // This is called when the Splitter Handle is dragged and the REM size will change:
    onResize = (sizePercent: number): void => {
        if (
            menuPanelWidthPercent.value === sizePercent ||
            menuPanelRef.value?.isCollapsed ||
            sizePercent < menuPanelMinWidth.value
        )
            return
        menuPanelWidthPercent.value = sizePercent
        settingsStore.mainMenuWidthRem = calculateMenuRemWidth(sizePercent)
    }
    // This is called when the window is resized,
    // which resizes the Menu Splitter to maintain a certain REM size:
    resizeObserver = new ResizeObserver((_) => {
        if (
            menuPanelRef.value?.isCollapsed ||
            splitterEl.getBoundingClientRect().width < minMenuWidthPx
        )
            return
        splitterGroupWidthPx.value = splitterEl.getBoundingClientRect().width
        // We need to first use the previous REM width to recalculate the new menu width
        menuPanelWidthPercent.value = calculateSplitterWidthPercent(settingsStore.mainMenuWidthRem)
        settingsStore.mainMenuWidthRem = calculateMenuRemWidth(menuPanelWidthPercent.value)
    })
    resizeObserver.observe(splitterEl)
    // apply the saved collapse state on startup/switch to desktop
    if (settingsStore.collapsedMainMenu) {
        // timeout needed as the auto-expand happens after onMounted code.
        setTimeout(() => menuPanelRef.value?.collapse())
    }
}

const disableDesktopMenu = (): void => {
    onResize = (_: number): void => {}
    if (resizeObserver) {
        resizeObserver.disconnect()
        resizeObserver = null
    }
}

watch(isMobile, (mobile, wasMobile) => {
    if (mobile && !wasMobile) {
        // switched to mobile view - disable desktop menu logic
        disableDesktopMenu()
    } else if (!mobile && wasMobile) {
        // switched to desktop view - re-enable desktop menu logic
        // use nextTick equivalent to ensure DOM is updated
        setTimeout(initDesktopMenu)
    }
})

onMounted(async () => {
    if (isMobile.value) return
    initDesktopMenu()
})
</script>

<template>
    <!--Mobile View-->
    <div
        v-if="width < 768"
        class="relative align-middle justify-items-center w-full bg-bg-two text-text-color"
    >
        <Drawer
            v-model:visible="drawerVisible"
            header="CoolerControl"
            class="!w-full bg-bg-two text-text-color"
        >
            <div class="flex flex-row" @click="drawerVisible = false">
                <div class="flex-col w-18 py-2 px-3 mx-0 h-full bg-bg-two">
                    <app-side-topbar />
                </div>
                <div class="h-full w-full pr-1 pb-2 bg-bg-two">
                    <ScrollAreaRoot
                        class="h-full w-full p-2 bg-bg-one rounded-lg border-border-one border"
                        type="hover"
                        :scroll-hide-delay="100"
                    >
                        <ScrollAreaViewport class="h-full">
                            <AppTreeMenu />
                        </ScrollAreaViewport>
                        <ScrollAreaScrollbar
                            class="flex select-none touch-none py-2 bg-transparent transition-colors duration-[120ms] ease-out data-[orientation=vertical]:w-1.5"
                            orientation="vertical"
                        >
                            <ScrollAreaThumb
                                class="flex-1 bg-text-color-secondary opacity-40 rounded-lg relative before:content-[''] before:absolute before:top-1/2 before:left-1/2 before:-translate-x-1/2 before:-translate-y-1/2 before:w-full before:h-full before:min-w-2 before:min-h-[44px]"
                            />
                        </ScrollAreaScrollbar>
                    </ScrollAreaRoot>
                </div>
            </div>
        </Drawer>
        <div class="absolute top-0 left-1 z-50">
            <Button
                class="!rounded-lg left-0 top-1 !border border-border-one w-12 h-12 !p-0 text-text-color-secondary hover:text-text-color hover:bg-surface-hover outline-none bg-bg-two"
                @click="drawerVisible = true"
            >
                <svg-icon
                    type="mdi"
                    class="text-text-color"
                    :path="mdiMenu"
                    :size="deviceStore.getREMSize(2)"
                />
            </Button>
        </div>
        <div class="flex flex-row h-screen w-full p-2 bg-bg-two text-text-color">
            <ScrollAreaRoot
                class="h-full w-full bg-bg-one rounded-lg border-border-one border"
                type="hover"
                :scroll-hide-delay="100"
            >
                <ScrollAreaViewport class="h-full">
                    <router-view v-slot="{ Component, route }">
                        <Suspense>
                            <component
                                :is="Component"
                                :key="route.path + (route.query?.key ?? '')"
                            />
                        </Suspense>
                    </router-view>
                </ScrollAreaViewport>
                <ScrollAreaScrollbar
                    class="flex select-none touch-none py-2 bg-transparent transition-colors duration-[120ms] ease-out data-[orientation=vertical]:w-1.5"
                    orientation="vertical"
                >
                    <ScrollAreaThumb
                        class="flex-1 bg-text-color-secondary opacity-40 rounded-lg relative before:content-[''] before:absolute before:top-1/2 before:left-1/2 before:-translate-x-1/2 before:-translate-y-1/2 before:w-full before:h-full before:min-w-2 before:min-h-[44px]"
                    />
                </ScrollAreaScrollbar>
                <ScrollAreaScrollbar
                    class="flex select-none touch-none py-2 bg-transparent transition-colors duration-[120ms] ease-out data-[orientation=vertical]:w-1.5"
                    orientation="horizontal"
                >
                    <ScrollAreaThumb
                        class="flex-1 bg-text-color-secondary opacity-40 rounded-lg relative before:content-[''] before:absolute before:top-1/2 before:left-1/2 before:-translate-x-1/2 before:-translate-y-1/2 before:w-full before:h-full before:min-w-2 before:min-h-[44px]"
                    />
                </ScrollAreaScrollbar>
            </ScrollAreaRoot>
        </div>
    </div>
    <!--Desktop View-->
    <div v-else class="flex flex-row h-screen w-full bg-bg-two text-text-color">
        <div class="flex-col w-18 py-2 px-3 mx-auto h-screen bg-bg-two">
            <app-side-topbar />
        </div>
        <SplitterGroup
            ref="splitterGroupRef"
            direction="horizontal"
            :keyboard-resize-by="10"
            class="flex-auto py-2 pr-2"
        >
            <SplitterPanel
                ref="menuPanelRef"
                class="bg-bg-one border-border-one rounded-lg"
                :class="{
                    invisible: settingsStore.collapsedMainMenu,
                    border: !settingsStore.collapsedMainMenu,
                }"
                collapsible
                :default-size="menuPanelWidthPercent"
                :min-size="menuPanelMinWidth"
                @resize="onResize"
                @collapse="settingsStore.collapsedMainMenu = true"
            >
                <ScrollAreaRoot class="h-full p-2" type="hover" :scroll-hide-delay="100">
                    <ScrollAreaViewport class="h-full">
                        <AppTreeMenu />
                    </ScrollAreaViewport>
                    <ScrollAreaScrollbar
                        class="flex select-none touch-none py-2 bg-transparent transition-colors duration-[120ms] ease-out data-[orientation=vertical]:w-1.5"
                        orientation="vertical"
                    >
                        <ScrollAreaThumb
                            class="flex-1 bg-text-color-secondary opacity-40 rounded-lg relative before:content-[''] before:absolute before:top-1/2 before:left-1/2 before:-translate-x-1/2 before:-translate-y-1/2 before:w-full before:h-full before:min-w-2 before:min-h-[44px]"
                        />
                    </ScrollAreaScrollbar>
                </ScrollAreaRoot>
            </SplitterPanel>
            <SplitterResizeHandle
                class="bg-bg-two"
                :class="{
                    'w-3': !settingsStore.collapsedMainMenu,
                    'w-0': settingsStore.collapsedMainMenu,
                }"
                :disabled="settingsStore.collapsedMainMenu"
            >
            </SplitterResizeHandle>
            <SplitterPanel
                class="truncate bg-bg-one border border-border-one rounded-lg"
                :min-size="viewPanelMinWidth"
            >
                <router-view v-slot="{ Component, route }">
                    <Suspense>
                        <component :is="Component" :key="route.path + (route.query?.key ?? '')" />
                    </Suspense>
                </router-view>
            </SplitterPanel>
        </SplitterGroup>
    </div>
</template>

<style lang="scss" scoped></style>
