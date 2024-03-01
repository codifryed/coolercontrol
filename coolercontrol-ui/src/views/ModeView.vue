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
import { useSettingsStore } from '@/stores/SettingsStore'
import { type Ref, ref } from 'vue'
import DataTable, {
    type DataTableRowReorderEvent,
    type DataTableRowSelectEvent,
} from 'primevue/datatable'
import Column from 'primevue/column'
import Tag from 'primevue/tag'
import Button from 'primevue/button'
import ModeOptions from '@/components/ModeOptions.vue'
import { useDialog } from 'primevue/usedialog'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiInformationVariantCircleOutline } from '@mdi/js'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { Mode } from '@/models/Mode.ts'
import ModeEditor from '@/components/ModeEditor.vue'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const dialog = useDialog()

settingsStore.getActiveMode() // verify what settings/mode is active

const selectedMode: Ref<Mode | undefined> = ref()

const createNewMode = (): void => {
    const newOrderId: number = settingsStore.modes.length + 1
    const newModeName = `Mode ${newOrderId}`
    settingsStore.createMode(newModeName)
}

const modesReordered = (event: DataTableRowReorderEvent) => {
    settingsStore.modes = event.value
    settingsStore.saveModeOrder()
}

const modeRowSelected = (_event: DataTableRowSelectEvent) => {
    if (selectedMode.value == null) {
        return
    }
    dialog.open(ModeEditor, {
        props: {
            header: 'Edit Mode',
            position: 'center',
            modal: true,
            dismissableMask: false,
        },
        data: {
            modeUID: selectedMode.value.uid,
        },
        onClose: () => (selectedMode.value = undefined),
    })
}

const modeDeleted = (): void => {
    selectedMode.value = undefined // cleanup after delete
}
</script>

<template>
    <div class="card">
        <div class="grid p-0 m-0 align-items-end justify-content-center card-container">
            <div class="col table-wrapper p-0">
                <div
                    class="flex flex-wrap align-items-center justify-content-between gap-2 mb-2 ml-2"
                >
                    <span class="text-xl text-800 font-bold"
                        >Modes
                        <Button
                            link
                            v-tooltip.bottom="{
                                value:
                                    'Modes are saved versions of all your device settings.' +
                                    'They can be used to quickly switch between different configurations.',
                                autoHide: false,
                            }"
                            class="p-0 ml-1 vertical-align-top"
                        >
                            <svg-icon
                                type="mdi"
                                :path="mdiInformationVariantCircleOutline"
                                :size="deviceStore.getREMSize(1.1)"
                            />
                        </Button>
                    </span>
                    <Button
                        rounded
                        icon="pi pi-plus"
                        label="New Mode"
                        aria-label="Create New Mode"
                        size="small"
                        @click="createNewMode"
                        v-tooltip.bottom="{
                            value: 'Creates a new Mode based on your current device settings.',
                            autoHide: false,
                        }"
                    />
                </div>
                <DataTable
                    v-model:selection="selectedMode"
                    :value="settingsStore.modes"
                    data-key="uid"
                    :meta-key-selection="false"
                    selection-mode="single"
                    @row-reorder="modesReordered"
                    size="small"
                    @row-select="modeRowSelected"
                >
                    <Column row-reorder header-style="width: 2.5rem" />
                    <Column field="name" header="Name" />
                    <Column field="active" header="">
                        <template #body="slotProps">
                            <Tag
                                style="background-color: var(--cc-red)"
                                v-if="slotProps.data.uid === settingsStore.activeModeUID"
                                value="Active"
                            />
                        </template>
                    </Column>
                    <Column field="activate" header="" header-style="width: 10rem">
                        <template #body="slotProps">
                            <Button
                                :disabled="slotProps.data.uid === settingsStore.activeModeUID"
                                icon="pi pi-play"
                                label="Activate"
                                rounded
                                size="small"
                                @click="settingsStore.activateMode(slotProps.data.uid)"
                            />
                        </template>
                    </Column>
                    <Column header-style="width: 3rem">
                        <template #body="slotProps">
                            <ModeOptions :mode="slotProps.data" @delete="modeDeleted" />
                        </template>
                    </Column>
                </DataTable>
            </div>
        </div>
    </div>
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
