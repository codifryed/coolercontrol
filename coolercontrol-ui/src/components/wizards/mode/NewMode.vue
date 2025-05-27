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
import InputText from 'primevue/inputtext'
import { computed, inject, ref, Ref } from 'vue'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Emitter, EventType } from 'mitt'
import { DEFAULT_NAME_STRING_LENGTH, useDeviceStore } from '@/stores/DeviceStore.ts'
import { mdiContentSaveOutline } from '@mdi/js'
import Button from 'primevue/button'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const router = useRouter()
const { t } = useI18n()
const emit = defineEmits<{
    (e: 'close'): void
}>()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!

const chosenName: Ref<string> = ref('New Mode')

const nameInvalid = computed(() => {
    return chosenName.value.length < 1 || chosenName.value.length > DEFAULT_NAME_STRING_LENGTH
})
const saveMode = async (): Promise<void> => {
    const newModeUID = await settingsStore.createMode(chosenName.value)
    if (newModeUID == null) return
    emitter.emit('mode-add-menu', { modeUID: newModeUID })
    await router.push({ name: 'modes', params: { modeUID: newModeUID } })
    emit('close')
}
</script>

<template>
    <div class="flex flex-col justify-between min-w-96 w-[40vw] min-h-max h-[40vh]">
        <div class="flex flex-col gap-y-4">
            <div>{{ t('layout.menu.tooltips.createMode') }}:</div>
            <div class="mt-0">
                <small class="ml-3 font-light text-sm"> {{ t('common.name') }}: </small>
                <div class="mt-0 flex flex-col">
                    <InputText
                        v-model="chosenName"
                        :placeholder="t('common.name')"
                        ref="inputArea"
                        id="property-name"
                        class="w-full h-11"
                        :invalid="nameInvalid"
                        :input-style="{ background: 'rgb(var(--colors-bg-one))' }"
                    />
                </div>
            </div>
        </div>
        <div class="flex flex-row justify-between mt-4">
            <div />
            <Button
                class="bg-accent/80 hover:!bg-accent w-32"
                :label="t('common.apply')"
                v-tooltip.bottom="t('views.speed.applySetting')"
                @click="saveMode"
            >
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiContentSaveOutline"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
