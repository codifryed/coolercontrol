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
import { useI18n } from 'vue-i18n'
import Popover from 'primevue/popover'
import ChannelCalibrationPanel from '@/components/ChannelCalibrationPanel.vue'
import type { UID } from '@/models/Device'

defineProps<{
    deviceUID: UID
    channelName: string
}>()

const { t } = useI18n()
const popRef = ref()

function toggle(event: Event) {
    popRef.value?.toggle(event)
}

function hide() {
    popRef.value?.hide()
}

defineExpose({ toggle })
</script>

<template>
    <Popover ref="popRef" append-to="body">
        <div class="w-80 rounded-lg border border-border-one bg-bg-two p-3">
            <div class="mb-2 text-sm font-semibold text-text-color">
                {{ t('components.channelExtensionSettings.calibration.heading') }}
            </div>
            <channel-calibration-panel
                :device-u-i-d="deviceUID"
                :channel-name="channelName"
                @request-close="hide"
            />
        </div>
    </Popover>
</template>
