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
import { ref } from 'vue'
import Menu from 'primevue/menu'
import Button from 'primevue/button'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useConfirm } from 'primevue/useconfirm'
import { Mode } from '@/models/Mode.ts'

interface Props {
    mode: Mode
}

const props = defineProps<Props>()
const emit = defineEmits<{
    delete: []
}>()
const settingsStore = useSettingsStore()
const optionsMenu = ref()
const confirm = useConfirm()

const optionsToggle = (event: any) => {
    optionsMenu.value.toggle(event)
}

const deleteMode = (modeToDelete: Mode): void => {
    confirm.require({
        message: `Are you sure you want to delete the Mode: "${modeToDelete.name}"?`,
        header: 'Delete Profile',
        icon: 'pi pi-exclamation-triangle',
        position: 'top',
        accept: () => {
            settingsStore.deleteMode(modeToDelete.uid)
            emit('delete')
        },
        reject: () => {},
    })
}

const modeOptions = () => {
    return [
        {
            label: 'Delete',
            icon: 'pi pi-trash',
            command: () => deleteMode(props.mode),
        },
    ]
}
</script>

<template>
    <div class="flex">
        <Button
            aria-label="Profile Card Options"
            icon="pi pi-ellipsis-v"
            rounded
            text
            plain
            size="small"
            class="ml-auto p-3"
            aria-controls="options_layout"
            style="height: 0.1rem; width: 0.1rem; box-shadow: none"
            type="button"
            aria-haspopup="true"
            @click.stop.prevent="optionsToggle($event)"
        />
        <Menu ref="optionsMenu" id="options_layout" :model="modeOptions()" popup class="w-8rem">
            <template #item="{ label, props }">
                <a class="flex" v-bind="props.action">
                    <span v-bind="props.icon" /><span v-bind="props.label">{{ label }}</span>
                </a>
            </template>
        </Menu>
    </div>
</template>

<style scoped lang="scss"></style>
