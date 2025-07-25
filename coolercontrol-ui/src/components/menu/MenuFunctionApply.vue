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
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiFlaskRoundBottom } from '@mdi/js'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { defineAsyncComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDialog } from 'primevue/usedialog'
import { UID } from '@/models/Device.ts'

const props = defineProps<{
    functionUID: UID
}>()

const { t } = useI18n()
const deviceStore = useDeviceStore()

const dialog = useDialog()
const functionApplyWizard = defineAsyncComponent(
    () => import('../wizards/function-apply/Wizard.vue'),
)

const applyFunction = async (): Promise<void> => {
    dialog.open(functionApplyWizard, {
        props: {
            header: t('components.wizards.functionApply.applyFunction'),
            position: 'center',
            modal: true,
            dismissableMask: true,
        },
        data: {
            functionUID: props.functionUID,
        },
    })
}
</script>

<template>
    <div
        class="rounded-lg w-8 h-8 border-none p-0 text-text-color-secondary outline-0 text-center justify-center items-center flex hover:text-text-color hover:bg-surface-hover"
        v-tooltip.top="{ value: t('components.wizards.functionApply.applyFunction') }"
    >
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color outline-0"
            @click.stop.prevent="applyFunction"
        >
            <svg-icon type="mdi" :path="mdiFlaskRoundBottom" :size="deviceStore.getREMSize(1.5)" />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
