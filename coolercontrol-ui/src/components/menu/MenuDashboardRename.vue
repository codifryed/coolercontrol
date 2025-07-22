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
import { mdiRenameOutline } from '@mdi/js'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { DEFAULT_NAME_STRING_LENGTH, useDeviceStore } from '@/stores/DeviceStore.ts'
import Button from 'primevue/button'
import { UID } from '@/models/Device.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { computed, ref, type Ref } from 'vue'
import InputText from 'primevue/inputtext'
import Popover from 'primevue/popover'
import { useI18n } from 'vue-i18n'

interface Props {
    dashboardUID: UID
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'nameChange', name: string): void
    (e: 'open', value: boolean): void
}>()

const popRef = ref()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { t } = useI18n()
const inputArea = ref()
const saveButton = ref()

const dashboard = settingsStore.dashboards.find(
    (dashboard) => dashboard.uid === props.dashboardUID,
)!
const currentName: string = dashboard.name
const nameInput: Ref<string> = ref(currentName)

const clickSaveButton = (): void => saveButton.value.$el.click()
const closeAndSave = (): void => {
    if (nameInvalid.value) return
    nameInput.value = deviceStore.sanitizeString(nameInput.value)
    dashboard.name = nameInput.value
    emit('nameChange', dashboard.name)
    popRef.value.hide()
}
const nameInvalid = computed(() => {
    return nameInput.value.length < 1 || nameInput.value.length > DEFAULT_NAME_STRING_LENGTH
})
</script>

<template>
    <div v-tooltip.top="{ value: t('layout.menu.tooltips.rename') }">
        <div
            class="rounded-lg w-8 h-8 border-none p-0 text-text-color-secondary outline-0 text-center justify-center items-center flex hover:text-text-color hover:bg-surface-hover"
            @click.stop.prevent="(event) => popRef.toggle(event)"
        >
            <svg-icon
                class="outline-0"
                type="mdi"
                :path="mdiRenameOutline"
                :size="deviceStore.getREMSize(1.5)"
            />
        </div>
        <Popover ref="popRef" @show="emit('open', true)" @hide="emit('open', false)">
            <div
                class="mt-2 w-80 bg-bg-two border-2 border-border-one p-4 rounded-lg text-text-color"
            >
                <span class="text-xl font-bold">{{ t('common.editName') }}</span>
                <div class="mt-8 flex flex-col">
                    <small class="ml-2 mb-1 font-light text-sm text-text-color-secondary">
                        {{ currentName }}
                    </small>
                    <InputText
                        ref="inputArea"
                        id="property-name"
                        class="w-20rem"
                        :invalid="nameInvalid"
                        v-model="nameInput"
                        @keydown.enter.prevent="clickSaveButton"
                        autofocus
                    />
                </div>
                <br />
                <div class="text-right mt-4">
                    <Button
                        ref="saveButton"
                        class="bg-accent/80 hover:bg-accent/100"
                        label="Save"
                        @click="closeAndSave"
                    >
                        {{ t('common.save') }}
                    </Button>
                </div>
            </div>
        </Popover>
    </div>
</template>

<style scoped lang="scss"></style>
