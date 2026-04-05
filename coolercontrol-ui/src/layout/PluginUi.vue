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
import { inject, Ref } from 'vue'
import type { DynamicDialogInstance } from 'primevue/dynamicdialogoptions'
import { usePluginIframe } from '@/composables/usePluginIframe.ts'

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const pluginId = dialogRef.value.data.pluginId ?? 'Unknown'
const closeDialog = () => {
    dialogRef.value.close()
}

const pluginIframe = usePluginIframe(pluginId, 'modal', closeDialog)
</script>

<template>
    <div class="flex flex-col min-w-[40vw] min-h-[40vw] w-[60vw] h-[80vh]">
        <iframe
            :ref="
                (el: any) => {
                    pluginIframe.iframeRef.value = el
                }
            "
            :name="`iframe-${pluginId}`"
            :src="pluginIframe.pluginUrl()"
            class="w-full h-full"
            sandbox="allow-scripts"
        >
            This browser does not support iframes.
        </iframe>
    </div>
</template>

<style scoped lang="scss"></style>
