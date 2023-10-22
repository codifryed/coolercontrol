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
import Tag from "primevue/tag"
import Button from "primevue/button"
import Column from "primevue/column"
import DataTable, {type DataTableRowReorderEvent, type DataTableRowSelectEvent} from "primevue/datatable"
import {useSettingsStore} from "@/stores/SettingsStore"
import {ref, type Ref} from "vue"
import {Function} from "@/models/Profile"
import FunctionEditor from "@/components/FunctionEditor.vue"
import FunctionOptions from "@/components/FunctionOptions.vue"
import ConfirmDialog from "primevue/confirmdialog";

const settingsStore = useSettingsStore()

const selectedFunction: Ref<Function | undefined> = ref()

const createNewFunction = (): void => {
  const newOrderId: number = settingsStore.functions.length + 1
  const newFunction = new Function(`New Function ${newOrderId}`,)
  settingsStore.functions.push(newFunction)
  settingsStore.saveFunction(newFunction.uid)
}

const functionsReordered = (event: DataTableRowReorderEvent) => {
  settingsStore.functions = event.value
  settingsStore.saveFunctionsOrder()
}


const rowSelected = (event: DataTableRowSelectEvent) => {
  if (event.data.uid === '0') {
    selectedFunction.value = undefined
  }
}

const getFunctionDetails = (fun: Function): string => {
  // todo: show deviance and delay options
  // if (fun.f_type === FunctionType.ExponentialMovingAvg)
  // } else {
  return ''
  // }
}
</script>

<template>
  <ConfirmDialog/>
  <div class="card">
    <div class="grid p-0 m-0 align-items-end justify-content-center card-container">
      <div class="col table-wrapper p-0">
        <Button rounded icon="pi pi-plus" label="New" aria-label="Create New Function" size="small"
                @click="createNewFunction" class="mb-3"/>
        <DataTable v-model:selection="selectedFunction" :value="settingsStore.functions" data-key="uid"
                   :meta-key-selection="false" selection-mode="single" @row-reorder="functionsReordered"
                   size="small" @row-select="rowSelected">
          <Column row-reorder header-style="width: 2.5rem"/>
          <Column field="name" header="Name"/>
          <Column field="type" header="Type" header-style="width: 6rem">
            <template #body="slotProps">
              <Tag :value="slotProps.data.f_type"/>
            </template>
          </Column>
          <Column>
            <template #body="slotProps">
              {{ getFunctionDetails(slotProps.data) }}
            </template>
          </Column>
          <Column header-style="width: 3rem">
            <template #body="slotProps">
              <FunctionOptions :function="slotProps.data"/>
            </template>
          </Column>
        </DataTable>
      </div>
    </div>
  </div>
  <Transition name="fade">
    <div v-if="selectedFunction!=null" class="card">
      <FunctionEditor :key="selectedFunction.uid" :function-u-i-d="selectedFunction.uid"/>
    </div>
  </Transition>

</template>

<style scoped lang="scss">
.fade-enter-active,
.fade-leave-active {
  transition: all 0.3s ease;
}

.fade-enter-from,
.fade-leave-to {
  height: 0;
  opacity: 0;
}

.table-wrapper :deep(.p-datatable-wrapper) {
  border-radius: 12px;
}
</style>