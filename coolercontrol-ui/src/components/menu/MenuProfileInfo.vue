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
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { PopoverContent, PopoverRoot, PopoverTrigger } from 'radix-vue'
import { mdiInformationSlabCircleOutline } from '@mdi/js'
import { useI18n } from 'vue-i18n'

interface Props {}

defineProps<Props>()
const emit = defineEmits<{
    (e: 'open', value: boolean): void
}>()

const deviceStore = useDeviceStore()
const { t } = useI18n()
</script>

<template>
    <div v-tooltip.top="{ value: t('common.overview') }">
        <popover-root @update:open="(value) => emit('open', value)">
            <popover-trigger
                class="rounded-lg w-8 h-8 border-none p-0 text-text-color-secondary outline-0 text-center justify-center items-center flex hover:text-text-color hover:bg-surface-hover"
            >
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiInformationSlabCircleOutline"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </popover-trigger>
            <popover-content side="right" class="z-10">
                <div
                    class="w-full max-w-prose bg-bg-two border border-border-one p-4 rounded-lg text-text-color"
                >
                    <span class="font-bold text-lg">{{ t('components.profileInfo.title') }}</span>
                    <div class="mt-1 h-2 border-border-one border-t-2" />
                    {{ t('components.profileInfo.description') }}
                    <ul class="pl-4 list-disc list-outside">
                        <li>{{ t('components.profileInfo.type.fixed') }}</li>
                        <li>{{ t('components.profileInfo.type.fanCurve') }}</li>
                        <li>{{ t('components.profileInfo.type.mix') }}</li>
                        <li>{{ t('components.profileInfo.type.default') }}</li>
                    </ul>
                    {{ t('components.profileInfo.additionalInfo') }}
                </div>
            </popover-content>
        </popover-root>
    </div>
</template>

<style scoped lang="scss"></style>
