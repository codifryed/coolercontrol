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
import { PopoverClose, PopoverContent, PopoverRoot, PopoverTrigger } from 'radix-vue'
import { useToast } from 'primevue/usetoast'
import { useI18n } from 'vue-i18n'

interface Props {
    profileUID: UID
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'nameChange', name: string): void
    (e: 'open', value: boolean): void
}>()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const toast = useToast()
const { t } = useI18n()

const inputArea = ref()
const saveButton = ref()

const profile = computed(
    () => settingsStore.profiles.find((profile) => profile.uid === props.profileUID)!,
)
const currentName: string = profile.value.name
const nameInput: Ref<string> = ref(currentName)

const clickSaveButton = (): void => saveButton.value.$el.click()
const closeAndSave = async (): Promise<void> => {
    if (nameInvalid.value) return
    nameInput.value = deviceStore.sanitizeString(nameInput.value)
    profile.value.name = nameInput.value
    const successful = await settingsStore.updateProfile(profile.value.uid)
    if (successful) {
        toast.add({
            severity: 'success',
            summary: t('common.success'),
            detail: t('views.profiles.profileUpdated'),
            life: 3000,
        })
        emit('nameChange', profile.value.name)
    } else {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: t('views.profiles.profileUpdateError'),
            life: 3000,
        })
    }
}
const nameInvalid = computed(() => {
    return nameInput.value.length < 1 || nameInput.value.length > DEFAULT_NAME_STRING_LENGTH
})
</script>

<template>
    <div v-tooltip.top="{ value: t('layout.menu.tooltips.rename') }">
        <popover-root @update:open="(value) => emit('open', value)">
            <popover-trigger
                class="rounded-lg w-8 h-8 border-none p-0 text-text-color-secondary outline-0 text-center justify-center items-center flex hover:text-text-color hover:bg-surface-hover"
            >
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiRenameOutline"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </popover-trigger>
            <popover-content side="right" class="z-10">
                <div
                    class="w-80 bg-bg-two border-2 border-border-one p-4 rounded-lg text-text-color"
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
                        />
                    </div>
                    <br />
                    <div class="text-right mt-4">
                        <popover-close ref="saveButton" @click="closeAndSave">
                            <Button class="bg-accent/80 hover:bg-accent/100" label="Save">
                                {{ t('common.save') }}
                            </Button>
                        </popover-close>
                    </div>
                </div>
            </popover-content>
        </popover-root>
    </div>
</template>

<style scoped lang="scss"></style>
