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
import { type Ref, ref, computed } from 'vue'
import InputText from 'primevue/inputtext'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useI18n } from 'vue-i18n'

const props = defineProps<{
    currentName: string
    saveNameFunction: (newName: string) => Promise<boolean>
}>()

const deviceStore = useDeviceStore()
const { t } = useI18n()

const isEditingName = ref(false)
const nameInput: Ref<string> = ref('')
const nameInputRef = ref()
const isCancelling = ref(false)

const inputWidth = computed(() => {
    const length = nameInput.value.length || 1
    return `${length + 1}ch`
})

const startEditingName = (): void => {
    nameInput.value = props.currentName
    isEditingName.value = true
    setTimeout(() => nameInputRef.value?.$el?.focus())
}
const saveNameInline = async (): Promise<void> => {
    const sanitized = deviceStore.sanitizeString(nameInput.value)
    // emitting to the menu that the name has been updated can also occur in the caller's saveFunction.
    // It will also need to update the base model's name without an entity refresh.
    const success = await props.saveNameFunction(sanitized)
    if (!success) {
        nameInput.value = props.currentName
    }
    isEditingName.value = false
}
const cancelEditName = (event: KeyboardEvent): void => {
    event.preventDefault()
    isCancelling.value = true
    nameInputRef.value?.$el?.blur()
    isEditingName.value = false
}
const handleBlur = (): void => {
    if (isCancelling.value) {
        isCancelling.value = false
        return
    }
    if (!isEditingName.value) return // otherwise onBlur will save a second time
    saveNameInline()
}
</script>

<template>
    <div class="flex pl-4 py-2 text-2xl overflow-hidden items-center">
        <InputText
            v-if="isEditingName"
            ref="nameInputRef"
            id="alert-name-input"
            v-model="nameInput"
            class="font-bold mt-[1px] !border-none !p-0 text-text-color-secondary"
            :style="{ width: inputWidth }"
            @keydown.enter="saveNameInline"
            @keydown.esc="cancelEditName"
            @blur="handleBlur"
        />
        <span
            v-else
            class="font-bold overflow-hidden overflow-ellipsis cursor-pointer hover:text-text-color-secondary"
            @click="startEditingName"
            v-tooltip.bottom="t('layout.menu.tooltips.rename')"
        >
            {{ props.currentName }}
        </span>
    </div>
</template>

<style scoped lang="scss"></style>
