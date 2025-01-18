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
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiChevronDoubleLeft, mdiChevronDoubleRight } from '@mdi/js'
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
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { computed, inject, onMounted, Ref, ref } from 'vue'
import { Emitter, EventType } from 'mitt'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const menuPanelRef = ref<InstanceType<typeof SplitterPanel>>()
const splitterGroupRef = ref<InstanceType<typeof SplitterGroup>>()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!
const minMenuWidthRem: number = 14
const minViewWidthRem: number = 18
const splitterGroupWidthPx: Ref<number> = ref(1900)

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

onMounted(async () => {
    // apply the saved change on startup to the menu itself.
    // Note: Expand automatically happens on startup for the Splitter
    if (settingsStore.collapsedMainMenu) {
        // timeout needed as the auto-expand happens after onMounted code.
        setTimeout(menuPanelRef.value!.collapse)
    }
    const splitterEl: HTMLElement = splitterGroupRef.value?.$el!
    splitterGroupWidthPx.value = splitterEl.getBoundingClientRect().width
    menuPanelWidthPercent.value = calculateSplitterWidthPercent(settingsStore.mainMenuWidthRem)
    // This is called when the Splitter Handle is dragged and the REM size will change:
    onResize = (sizePercent: number): void => {
        if (menuPanelWidthPercent.value === sizePercent || menuPanelRef.value?.isCollapsed) return
        menuPanelWidthPercent.value = sizePercent
        settingsStore.mainMenuWidthRem = calculateMenuRemWidth(sizePercent)
    }
    // This is called when the window is resized,
    // which resizes the Menu Splitter to maintain a certain REM size:
    const resizeObserver = new ResizeObserver((_) => {
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
})
</script>

<template>
    <div class="flex flex-row h-screen w-full bg-bg-two text-text-color">
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
                            class="flex-1 bg-text-color-secondary opacity-40 rounded-lg relative before:content-[''] before:absolute before:top-1/2 before:left-1/2 before:-translate-x-1/2 before:-translate-y-1/2 before:w-full before:h-full before:min-w-[44px] before:min-h-[44px]"
                        />
                    </ScrollAreaScrollbar>
                </ScrollAreaRoot>
            </SplitterPanel>
            <SplitterResizeHandle
                class="bg-bg-two"
                :class="{
                    'w-2.5': !settingsStore.collapsedMainMenu,
                    'w-0': settingsStore.collapsedMainMenu,
                }"
                :disabled="settingsStore.collapsedMainMenu"
            >
                <!--Bug with dragging: :hit-area-margins="{ coarse: 2, fine: 2 }"-->
                    class="absolute mt-[2.625rem] bg-bg-two !rounded-none !border !px-1 !py-1 hover:!bg-bg-two !text-text-color-secondary hover:!text-text-color z-50"
                    :class="{
                        'ml-[-1.525rem] !rounded-r-0 !rounded-l-lg !border-r-0':
                            menuPanelRef?.isExpanded,
                        'ml-0 !rounded-l-0 !rounded-r-lg !border-l-0': menuPanelRef?.isCollapsed,
                    }"
                    @click="toggleSideMenu"
                >
                    <svg-icon
                        class="outline-0"
                        type="mdi"
                        :path="
                            menuPanelRef?.isCollapsed ? mdiChevronDoubleRight : mdiChevronDoubleLeft
                        "
                        :size="deviceStore.getREMSize(1.0)"
                    />
                </Button>
            </SplitterResizeHandle>
            <SplitterPanel
                class="truncate bg-bg-one border border-border-one rounded-lg"
                :min-size="viewPanelMinWidth"
            >
                <router-view v-slot="{ Component, route }">
                    <Suspense>
                        <component :is="Component" :key="route.path" />
                    </Suspense>
                </router-view>
            </SplitterPanel>
        </SplitterGroup>
    </div>
</template>

<style lang="scss" scoped></style>
