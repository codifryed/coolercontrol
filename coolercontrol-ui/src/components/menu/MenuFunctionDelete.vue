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
import { mdiDeleteOutline } from '@mdi/js'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import Button from 'primevue/button'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { UID } from '@/models/Device.ts'
import { useConfirm } from 'primevue/useconfirm'
import { useToast } from 'primevue/usetoast'
import { Profile } from '@/models/Profile.ts'
import { useI18n } from 'vue-i18n'

interface Props {
    functionUID: UID
}

const emit = defineEmits<{
    (e: 'deleted', functionUID: UID): void
}>()

const props = defineProps<Props>()

const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const confirm = useConfirm()
const toast = useToast()

const deleteFunction = (): void => {
    const functionUIDToDelete = props.functionUID
    const functionIndex: number = settingsStore.functions.findIndex(
        (fun) => fun.uid === functionUIDToDelete,
    )
    if (functionIndex === -1) {
        console.error('Function not found for removal: ' + functionUIDToDelete)
        return
    }
    if (functionUIDToDelete === '0') {
        return // can't delete default
    }
    const functionName = settingsStore.functions[functionIndex].name
    const associatedProfiles: Array<Profile> = settingsStore.profiles.filter(
        (p) => p.function_uid === functionUIDToDelete,
    )
    const deleteMessage: string =
        associatedProfiles.length === 0
            ? t('views.functions.deleteFunctionConfirm', { name: functionName })
            : t('views.functions.deleteFunctionWithProfilesConfirm', {
                  name: functionName,
                  profiles: associatedProfiles.map((p) => p.name).join(', '),
              })
    confirm.require({
        message: deleteMessage,
        header: t('views.functions.deleteFunction'),
        icon: 'pi pi-exclamation-triangle',
        accept: async () => {
            await settingsStore.deleteFunction(functionUIDToDelete)
            settingsStore.functions.splice(functionIndex, 1)
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('views.functions.functionDeleted'),
                life: 3000,
            })
            emit('deleted', functionUIDToDelete)
        },
    })
}
</script>

<template>
    <div v-tooltip.top="{ value: t('common.delete') }">
        <Button
            class="rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color"
            @click="deleteFunction"
        >
            <svg-icon type="mdi" :path="mdiDeleteOutline" :size="deviceStore.getREMSize(1.5)" />
        </Button>
    </div>
</template>

<style scoped lang="scss"></style>
