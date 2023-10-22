<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2023  Guy Boldon
  - |
  - This program is free software: you can redistribute it and/or modify
  - it under the terms of the GNU General Public License as published by
  - the Free Software Foundation, either version 3 of the License, or
  - (at your option) any later version.
  - |
  - This program is distributed in the hope that it will be useful,
  - but WITHOUT ANY WARRANTY; without even the implied warranty of
  - MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  - GNU General Public License for more details.
  - |
  - You should have received a copy of the GNU General Public License
  - along with this program.  If not, see <https://www.gnu.org/licenses/>.
  -->

<script setup lang="ts">
import {ref} from "vue"
import {Function} from "@/models/Profile"
import Menu from "primevue/menu"
import Button from "primevue/button"
import {useSettingsStore} from "@/stores/SettingsStore"
import {useConfirm} from "primevue/useconfirm"

interface Props {
  function: Function
}

const props = defineProps<Props>()
const settingsStore = useSettingsStore()
const optionsMenu = ref()
const confirm = useConfirm()

const optionsToggle = (event: any) => {
  optionsMenu.value.toggle(event)
}

const duplicateFunction = (functionToDuplicate: Function): void => {
  const newFunction = new Function(
      `Copy of ${functionToDuplicate.name}`,
      functionToDuplicate.f_type,
      functionToDuplicate.response_delay,
      functionToDuplicate.deviance,
      functionToDuplicate.sample_window,
  )
  settingsStore.functions.push(newFunction)
  settingsStore.saveFunction(newFunction.uid)
}

const deleteFunction = (functionToDelete: Function): void => {
  if (functionToDelete.uid === '0') {
    return
  }
  confirm.require({
    message: `Are you sure you want to delete the function: "${functionToDelete.name}"?`,
    header: 'Delete Function',
    icon: 'pi pi-exclamation-triangle',
    position: 'top',
    accept: () => {
      settingsStore.functions.splice(
          settingsStore.functions.findIndex((fun) => fun.uid === props.function.uid),
          1
      )
      settingsStore.deleteFunction(props.function.uid)
    },
    reject: () => {
    }
  })
}

const functionOptions = () => {
  return props.function.uid === '0' // the non-deletable default profile
      ? [
        {
          label: 'Duplicate',
          icon: 'pi pi-copy',
          command: () => duplicateFunction(props.function),
        },
      ]
      : [
        {
          label: 'Duplicate',
          icon: 'pi pi-copy',
          command: () => duplicateFunction(props.function),
        },
        {
          label: 'Delete',
          icon: 'pi pi-trash',
          command: () => deleteFunction(props.function),
        }
      ]
}
</script>

<template>
  <div class="flex">
    <Button aria-label="Function Card Options" icon="pi pi-ellipsis-v" rounded text plain size="small"
            class="ml-auto p-3" aria-controls="options_layout"
            style="height: 0.1rem; width: 0.1rem; box-shadow: none;" type="button" aria-haspopup="true"
            @click.stop.prevent="optionsToggle($event)"/>
    <Menu ref="optionsMenu" id="options_layout" :model="functionOptions()" popup
          class="w-8rem">
      <template #item="{ label, item, props }">
        <a class="flex" v-bind="props.action">
          <span v-bind="props.icon"/><span v-bind="props.label">{{ label }}</span>
        </a>
      </template>
    </Menu>
  </div>
</template>

<style scoped lang="scss">

</style>