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
import SelectButton from 'primevue/selectbutton'
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

// Component boundary with a primitive model so an open tooltip survives parent
// re-renders (see MenuHealthIcon.vue); the Info page updates on every log line.
const model = defineModel<string>({ required: true })
const { t } = useI18n({ useScope: 'global' })

const backendOptions = computed(() => [
    { label: t('views.appInfo.stressNgBackend'), value: 'stress_ng' },
    { label: t('views.appInfo.builtInBackend'), value: 'built_in' },
])
</script>

<template>
    <select-button
        v-model="model"
        v-tooltip.top="{ escape: false, value: t('views.appInfo.backendTooltip') }"
        :options="backendOptions"
        option-label="label"
        option-value="value"
        :allow-empty="false"
    />
</template>
