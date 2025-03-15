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
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiHome } from '@mdi/js'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'

const { getREMSize } = useDeviceStore()
</script>

<template>
    <div class="flex h-[3.5rem] border-b-4 border-border-one items-center justify-between">
        <div class="pl-4 py-2 text-xl">404 Not Found</div>
    </div>
    <div class="m-10 text-center font-bold text-2xl">
        <p>Just like the perfect Linux 🐧 distro,<br />this page does not exist.</p>
        <router-link to="/">
            <Button
                class="mt-16 !rounded-lg border-2 bg-accent/70 text-text-color-secondary w-64 h-12 !p-0 hover:text-text-color hover:!bg-accent outline-0"
            >
                Return Home
                <svg-icon class="ml-4" type="mdi" :path="mdiHome" :size="getREMSize(1.75)" />
            </Button>
        </router-link>
    </div>
</template>

<style scoped lang="scss"></style>
