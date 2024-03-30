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
import { UID } from '@/models/Device.ts'
import { useToast } from 'primevue/usetoast'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const dialog = useDialog()
const toast = useToast()

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
            header: 'Mode Settings',
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

const activateMode = async (modeUID: UID): Promise<void> => {
    await settingsStore.getActiveMode() // verify what settings/mode is active
    if (settingsStore.modeActive === modeUID) {
        toast.add({
            severity: 'success',
            summary: 'Success',
            detail: 'Mode Already Active',
            life: 3000,
        })
    } else {
        await settingsStore.activateMode(modeUID)
    }
}

const saveModeDeviceSettings = async (modeUID: UID): Promise<void> => {
    await settingsStore.getActiveMode() // verify what settings/mode is active
    if (settingsStore.modeActive === modeUID) {
        settingsStore.modeInEdit = undefined
        toast.add({
            severity: 'success',
            summary: 'Success',
            detail: 'No changes made',
            life: 3000,
        })
    } else {
        await settingsStore.updateModeSettings(modeUID)
        settingsStore.modeInEdit = undefined
    }
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
                                    'Modes are saved snapshots of all your device\'s settings. ' +
                                    'They can be used to quickly switch between different configurations. ' +
                                    'Note that internal Profile and Function settings are not saved ' +
                                    'in Modes, only the base device settings.',
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
                        label="New Mode from Current Settings"
                        aria-label="Create New Mode"
                        size="small"
                        @click="createNewMode"
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
                    <Column field="active" header="" header-style="width: 6rem">
                        <template #body="slotProps">
                            <Tag
                                style="background-color: var(--cc-red)"
                                v-if="slotProps.data.uid === settingsStore.modeActive"
                                value="Active"
                            />
                        </template>
                    </Column>
                    <Column field="edit" header="" header-style="width: 15rem" class="text-right">
                        <template #body="slotProps">
                            <div v-if="settingsStore.modeInEdit === slotProps.data.uid">
                                <Button
                                    class="mr-3"
                                    icon="pi pi-save"
                                    label="Save"
                                    rounded
                                    size="small"
                                    @click="saveModeDeviceSettings(slotProps.data.uid)"
                                />
                                <Button
                                    icon="pi pi-spin pi-cog"
                                    class="w-7rem"
                                    label="Editing"
                                    rounded
                                    outlined
                                    size="small"
                                    @click="settingsStore.modeInEdit = undefined"
                                />
                            </div>
                            <div v-else>
                                <Button
                                    :disabled="slotProps.data.uid !== settingsStore.modeActive"
                                    class="w-7rem"
                                    icon="pi pi-cog"
                                    label="Edit"
                                    rounded
                                    outlined
                                    size="small"
                                    @click="settingsStore.modeInEdit = slotProps.data.uid"
                                    v-tooltip.bottom="{
                                        value:
                                            'This enables edit mode, which allows you to make ' +
                                            'changes to your current device settings, and then save ' +
                                            'or discard them once you are finished.',
                                        showDelay: 500,
                                    }"
                                />
                            </div>
                        </template>
                    </Column>
                    <Column field="activate" header="" header-style="width: 8rem">
                        <template #body="slotProps">
                            <Button
                                :disabled="
                                    settingsStore.modeInEdit != null ||
                                    slotProps.data.uid === settingsStore.modeActive
                                "
                                icon="pi pi-play"
                                label="Activate"
                                rounded
                                size="small"
                                @click="activateMode(slotProps.data.uid)"
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
