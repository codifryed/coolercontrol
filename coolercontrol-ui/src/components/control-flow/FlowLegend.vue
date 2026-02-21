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
import { ref } from 'vue'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiFan, mdiTuneVariant, mdiThermometer, mdiFlaskRoundBottom } from '@mdi/js'

const expanded = ref(false)
</script>

<template>
    <div class="absolute bottom-4 left-4 z-10">
        <button
            class="rounded-lg border border-border-one bg-bg-two px-3 py-1.5 text-xs text-text-color-secondary shadow-md transition-colors hover:text-text-color"
            @click="expanded = !expanded"
        >
            Legend
        </button>
        <div
            v-if="expanded"
            class="mt-2 space-y-2 rounded-lg border border-border-one bg-bg-two p-3 shadow-lg"
        >
            <div class="flex items-center gap-2">
                <div class="h-3 w-3 rounded border-t-2 border-accent bg-bg-two" />
                <svg-icon type="mdi" :path="mdiFan" class="size-4 text-text-color-secondary" />
                <span class="text-xs text-text-color-secondary">Fan Channel</span>
            </div>
            <div class="flex items-center gap-2">
                <div class="h-3 w-3 rounded border-t-2 border-accent bg-bg-two" />
                <svg-icon
                    type="mdi"
                    :path="mdiTuneVariant"
                    class="size-4 text-text-color-secondary"
                />
                <span class="text-xs text-text-color-secondary">Profile</span>
            </div>
            <div class="flex items-center gap-2">
                <div class="h-3 w-3 rounded border-t-2 border-accent bg-bg-two" />
                <svg-icon
                    type="mdi"
                    :path="mdiThermometer"
                    class="size-4 text-text-color-secondary"
                />
                <span class="text-xs text-text-color-secondary">Temp Source</span>
            </div>
            <div class="flex items-center gap-2">
                <div class="h-3 w-3 rounded border-t-2 border-green bg-bg-two" />
                <svg-icon
                    type="mdi"
                    :path="mdiFlaskRoundBottom"
                    class="size-4 text-text-color-secondary"
                />
                <span class="text-xs text-text-color-secondary">Custom Sensor</span>
            </div>
            <div class="border-t border-border-one pt-2">
                <div class="flex items-center gap-2">
                    <svg width="28" height="10" class="shrink-0">
                        <defs>
                            <marker
                                id="legend-arrow"
                                viewBox="0 0 10 10"
                                refX="10"
                                refY="5"
                                markerWidth="6"
                                markerHeight="6"
                                orient="auto-start-reverse"
                            >
                                <path
                                    d="M 0 0 L 10 5 L 0 10 z"
                                    fill="rgb(var(--colors-border-one))"
                                />
                            </marker>
                        </defs>
                        <line
                            x1="0"
                            y1="5"
                            x2="22"
                            y2="5"
                            stroke="rgb(var(--colors-border-one))"
                            stroke-width="2"
                            marker-end="url(#legend-arrow)"
                        />
                    </svg>
                    <span class="text-xs text-text-color-secondary">Direct (animated)</span>
                </div>
                <div class="mt-1 flex items-center gap-2">
                    <div
                        class="h-0.5 w-6"
                        style="
                            background: repeating-linear-gradient(
                                to right,
                                rgb(var(--colors-border-one)) 0,
                                rgb(var(--colors-border-one)) 6px,
                                transparent 6px,
                                transparent 10px
                            );
                        "
                    />
                    <span class="text-xs text-text-color-secondary">Mix/Overlay Member</span>
                </div>
            </div>
        </div>
    </div>
</template>
