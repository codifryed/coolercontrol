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
import { SplitterGroup, SplitterPanel, SplitterResizeHandle } from 'radix-vue'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'radix-vue'
import AppSideTopbar from '@/layout/AppSideTopbar.vue'
import AppConfig from '@/layout/AppConfig.vue'
import AppTreeMenu from '@/layout/AppTreeMenu.vue'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { ref } from 'vue'

const deviceStore = useDeviceStore()
const menuPanelRef = ref<InstanceType<typeof SplitterPanel>>()
</script>

<template>
    <div class="flex flex-row h-screen w-full bg-bg-one text-text-color">
        <div
            class="flex-col w-16 py-2 px-2 mx-auto h-screen bg-bg-two border-r border-r-border-one"
        >
            <app-side-topbar />
        </div>
        <SplitterGroup
            direction="horizontal"
            auto-save-id="main-splitter"
            :keyboard-resize-by="10"
            class="flex-auto"
        >
            <SplitterPanel
                ref="menuPanelRef"
                class="bg-bg-one border border-border-one"
                :default-size="25"
                :min-size="10"
                collapsible
            >
                <ScrollAreaRoot style="--scrollbar-size: 10px">
                    <ScrollAreaViewport class="p-2 pb-4 h-screen w-full">
                        <AppTreeMenu />
                    </ScrollAreaViewport>
                    <ScrollAreaScrollbar
                        class="flex select-none touch-none p-0.5 bg-transparent transition-colors duration-[120ms] ease-out data-[orientation=vertical]:w-2.5"
                        orientation="vertical"
                    >
                        <ScrollAreaThumb
                            class="flex-1 bg-border-one opacity-80 rounded-lg relative before:content-[''] before:absolute before:top-1/2 before:left-1/2 before:-translate-x-1/2 before:-translate-y-1/2 before:w-full before:h-full before:min-w-[44px] before:min-h-[44px]"
                        />
                    </ScrollAreaScrollbar>
                </ScrollAreaRoot>
            </SplitterPanel>
            <SplitterResizeHandle class="bg-border-one w-2">
                <!--Bug with dragging: :hit-area-margins="{ coarse: 2, fine: 2 }"-->
                <Button
                    class="absolute mt-11 ml-2 bg-border-one !rounded-none !rounded-l-0 !rounded-r-lg !px-1 !py-1 hover:!bg-border-one !text-text-color-secondary hover:!text-text-color z-50"
                    @click="
                        () =>
                            menuPanelRef?.isCollapsed
                                ? menuPanelRef?.expand()
                                : menuPanelRef?.collapse()
                    "
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
                class="truncate bg-bg-one border border-border-one"
                :default-size="75"
                :min-size="25"
                collapsible
            >
                <router-view v-slot="{ Component, route }">
                    <Suspense>
                        <component :is="Component" :key="route.path" />
                    </Suspense>
                </router-view>
            </SplitterPanel>
        </SplitterGroup>
        <!--todo: make this just a big menu, similar to the others on the sidebar:-->
        <app-config />
    </div>
    <!--    <div class="layout-wrapper" :class="containerClass">-->
    <!--        <app-topbar></app-topbar>-->
    <!--        <div class="layout-sidebar">-->
    <!--            <app-sidebar></app-sidebar>-->
    <!--        </div>-->
    <!--        <div class="layout-main-container">-->
    <!--            <div class="layout-main" ref="laymain">-->
    <!--                <router-view v-slot="{ Component, route }">-->
    <!--                    &lt;!&ndash;          <transition name="fade">&ndash;&gt;-->
    <!--                    <component :is="Component" :key="route.path" />-->
    <!--                    &lt;!&ndash;          </transition>&ndash;&gt;-->
    <!--                </router-view>-->
    <!--            </div>-->
    <!--        </div>-->
    <!--        <app-config/>-->
    <!--        <div class="layout-mask"></div>-->
    <!--    </div>-->
</template>

<style lang="scss" scoped></style>
