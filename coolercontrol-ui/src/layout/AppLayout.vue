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
import { SplitterGroup, SplitterPanel, SplitterResizeHandle } from 'radix-vue'
import AppSideTopbar from '@/layout/AppSideTopbar.vue'
import AppConfig from '@/layout/AppConfig.vue'
import AppTreeMenu from "@/layout/AppTreeMenu.vue";
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
            <!--            todo: we might be able to add an extra handle thing to the Handle itself-->
            <!--            when the panel is collapsed-->
            <SplitterPanel
                class="truncate bg-bg-one border border-border-one p-2"
                :default-size="25"
                :min-size="15"
                collapsible
            >
                <AppTreeMenu/>
            </SplitterPanel>
            <SplitterResizeHandle class="bg-border-one w-0.5" />
            <SplitterPanel
                class="truncate bg-bg-one border border-border-one p-2"
                :default-size="75"
                :min-size="25"
                collapsible
            >
                <router-view v-slot="{ Component, route }">
                    <component :is="Component" :key="route.path" />
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
