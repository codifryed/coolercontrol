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
import SvgIcon from '@jamescoyle/vue-icon'
import OverlayBadge from 'primevue/overlaybadge'

// This component boundary is load-bearing: the menu re-renders every second
// with live values, and PrimeVue's tooltip directive tears down an open
// tooltip on every update of its host element. With only primitive props,
// unchanged renders skip this subtree and a hovered badge tooltip stays open.
interface Props {
    iconPath: string
    size: number
    tooltip: string
    unhealthy: boolean
    color?: string
    active?: boolean
    alertActive?: boolean
    spins?: boolean
}
const props = defineProps<Props>()
</script>

<template>
    <overlay-badge
        v-tooltip.top="props.tooltip"
        value="!"
        severity="warn"
        class="[&>[data-pc-name=pcbadge]]:!h-4 [&>[data-pc-name=pcbadge]]:!min-w-4 [&>[data-pc-name=pcbadge]]:!text-xs [&>[data-pc-name=pcbadge]]:!leading-4"
        :class="{ '[&>[data-pc-name=pcbadge]]:!hidden': !props.unhealthy }"
    >
        <svg-icon
            type="mdi"
            :path="props.iconPath"
            :size="props.size"
            :style="props.color != null ? { color: props.color } : undefined"
            :class="{
                'text-accent': props.active,
                'text-error': props.alertActive,
                'animate-spin-slow': props.spins,
            }"
        />
    </overlay-badge>
</template>
