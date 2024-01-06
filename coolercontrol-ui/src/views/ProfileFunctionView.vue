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
import { Function, Profile, ProfileType } from '@/models/Profile'
import DataTable, {
    type DataTableRowReorderEvent,
    type DataTableRowSelectEvent,
} from 'primevue/datatable'
import Column from 'primevue/column'
import Tag from 'primevue/tag'
import Button from 'primevue/button'
import ProfileOptions from '@/components/ProfileOptions.vue'
import { useDialog } from 'primevue/usedialog'
import FunctionOptions from '@/components/FunctionOptions.vue'
import ProfileEditor from '@/components/ProfileEditor.vue'
import FunctionEditor from '@/components/FunctionEditor.vue'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiInformationVariantCircleOutline } from '@mdi/js'
import { useDeviceStore } from '@/stores/DeviceStore.ts'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const dialog = useDialog()

const selectedProfile: Ref<Profile | undefined> = ref()
const selectedFunction: Ref<Function | undefined> = ref()

const createNewProfile = (): void => {
    const newOrderId: number = settingsStore.profiles.length + 1
    const newProfile = new Profile(`New Profile ${newOrderId}`, ProfileType.Default)
    settingsStore.profiles.push(newProfile)
    settingsStore.saveProfile(newProfile.uid)
}

const profilesReordered = (event: DataTableRowReorderEvent) => {
    settingsStore.profiles = event.value
    settingsStore.saveProfilesOrder()
}

const profileRowSelected = (event: DataTableRowSelectEvent) => {
    if (selectedProfile.value == null) {
        return
    }
    if (event.data.uid === '0') {
        selectedProfile.value = undefined
        return
    }
    dialog.open(ProfileEditor, {
        props: {
            header: 'Edit Profile',
            position: 'center',
            modal: true,
            dismissableMask: false,
        },
        data: {
            profileUID: selectedProfile.value.uid,
        },
        onClose: (options: any) => {
            selectedProfile.value = undefined
            const data = options.data
            if (data && data.functionUID != null) {
                dialog.open(FunctionEditor, {
                    props: {
                        header: 'Edit Function',
                        position: 'center',
                        modal: true,
                        dismissableMask: false,
                    },
                    data: {
                        functionUID: data.functionUID,
                    },
                })
            }
        },
    })
}

const profileDeleted = (): void => {
    selectedProfile.value = undefined
}
const getProfileDetails = (profile: Profile): string => {
    // todo: handle MIX profiles in the future
    if (profile.p_type === ProfileType.Graph && profile.temp_source != null) {
        return `${profile.temp_source.temp_name}`
    } else {
        return ''
    }
}

const getFunctionName = (profile: Profile): string => {
    return (
        settingsStore.functions.find((fn: Function) => fn.uid === profile.function_uid)?.name ??
        'Unknown'
    )
}

const createNewFunction = (): void => {
    const newOrderId: number = settingsStore.functions.length + 1
    const newFunction = new Function(`New Function ${newOrderId}`)
    settingsStore.functions.push(newFunction)
    settingsStore.saveFunction(newFunction.uid)
}

const functionsReordered = (event: DataTableRowReorderEvent) => {
    settingsStore.functions = event.value
    settingsStore.saveFunctionsOrder()
}

const functionRowSelected = (event: DataTableRowSelectEvent) => {
    if (selectedFunction.value == null) {
        return
    }
    if (event.data.uid === '0') {
        selectedFunction.value = undefined
        return
    }
    dialog.open(FunctionEditor, {
        props: {
            header: 'Edit Function',
            position: 'center',
            modal: true,
            dismissableMask: false,
        },
        data: {
            functionUID: selectedFunction.value.uid,
        },
        onClose: () => (selectedFunction.value = undefined),
    })
}

const functionDeleted = (): void => {
    selectedFunction.value = undefined
}

// const getFunctionDetails = (fun: Function): string => {
//   // todo: possibly show some other options (not currently)
//   return ''
// }
</script>

<template>
    <div class="card">
        <div class="grid p-0 m-0 align-items-end justify-content-center card-container">
            <div class="col table-wrapper p-0">
                <div
                    class="flex flex-wrap align-items-center justify-content-between gap-2 mb-2 ml-2"
                >
                    <span class="text-xl text-800 font-bold"
                        >Profiles
                        <Button
                            link
                            v-tooltip.bottom="{
                                value:
                                    'Profiles are speed profiles which you can apply to any devices. ' +
                                    'A Default Profile is whatever the device is doing without applying anything, i.e. no setting.',
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
                        label="New Profile"
                        aria-label="Create New Profile"
                        size="small"
                        @click="createNewProfile"
                    />
                </div>
                <DataTable
                    v-model:selection="selectedProfile"
                    :value="settingsStore.profiles"
                    data-key="uid"
                    :meta-key-selection="false"
                    selection-mode="single"
                    @row-reorder="profilesReordered"
                    size="small"
                    @row-select="profileRowSelected"
                >
                    <Column row-reorder header-style="width: 2.5rem" />
                    <Column field="name" header="Name" />
                    <Column field="type" header="Type" header-style="width: 6rem">
                        <template #body="slotProps">
                            <Tag :value="slotProps.data.p_type" />
                        </template>
                    </Column>
                    <Column header="Function" header-style="width: 12rem">
                        <template #body="slotProps">
                            {{ getFunctionName(slotProps.data) }}
                        </template>
                    </Column>
                    <Column header="Temp Source(s)">
                        <template #body="slotProps">
                            {{ getProfileDetails(slotProps.data) }}
                        </template>
                    </Column>
                    <Column header-style="width: 3rem">
                        <template #body="slotProps">
                            <ProfileOptions :profile="slotProps.data" @delete="profileDeleted" />
                        </template>
                    </Column>
                </DataTable>
            </div>
        </div>
    </div>
    <div class="card">
        <div class="grid p-0 m-0 align-items-end justify-content-center card-container">
            <div class="col table-wrapper p-0">
                <div
                    class="flex flex-wrap align-items-center justify-content-between gap-2 mb-2 ml-2"
                >
                    <span class="text-xl text-800 font-bold"
                        >Functions
                        <Button
                            link
                            v-tooltip.bottom="{
                                value:
                                    'Functions determine how speed profiles are evaluated and applied. ' +
                                    'An Identity Function returns whatever the output from the speed profile is without any adjustment.',
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
                        label="New Function"
                        aria-label="Create New Function"
                        size="small"
                        @click="createNewFunction"
                    />
                </div>
                <DataTable
                    v-model:selection="selectedFunction"
                    :value="settingsStore.functions"
                    data-key="uid"
                    :meta-key-selection="false"
                    selection-mode="single"
                    @row-reorder="functionsReordered"
                    size="small"
                    @row-select="functionRowSelected"
                >
                    <Column row-reorder header-style="width: 2.5rem" />
                    <Column field="name" header="Name" />
                    <Column field="type" header="Type" header-style="width: 6rem">
                        <template #body="slotProps">
                            <Tag :value="slotProps.data.f_type" />
                        </template>
                    </Column>
                    <Column>
                        <template #body>
                            {{ '' }}
                        </template>
                    </Column>
                    <Column header-style="width: 3rem">
                        <template #body="slotProps">
                            <FunctionOptions :function="slotProps.data" @delete="functionDeleted" />
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
