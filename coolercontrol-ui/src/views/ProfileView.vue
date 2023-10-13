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
import {useSettingsStore} from "@/stores/SettingsStore"
import ProfileEditor from "@/components/ProfileEditor.vue"
import {ref, type Ref} from "vue"
import {Profile, ProfileType} from "@/models/Profile"
import DataTable, {type DataTableRowReorderEvent, type DataTableRowSelectEvent} from 'primevue/datatable'
import Column from 'primevue/column'
import Tag from 'primevue/tag'
import Button from 'primevue/button'
import ConfirmDialog from 'primevue/confirmdialog'
import ProfileOptions from "@/views/ProfileOptions.vue"

const settingsStore = useSettingsStore()

const selectedProfile: Ref<Profile | undefined> = ref()

const createNewProfile = (): void => {
  // const nextId = settingsStore.profiles.reduce((previous, current) => previous && previous > current ? previous : current)
  const newOrderId: number = settingsStore.profiles.slice(-1)[0].orderId + 1
  const newProfile = new Profile(
      newOrderId,
      ProfileType.DEFAULT,
      `New Profile ${newOrderId}`,
      [],
  )
  settingsStore.profiles.push(newProfile)
}

const currentProfileChanged = ref(false)

const profilesReordered = (event: DataTableRowReorderEvent) => {
  settingsStore.profiles = event.value
}

const rowSelected = (event: DataTableRowSelectEvent) => {
  if (event.data.orderId === 0) {
    selectedProfile.value = undefined
  }
}

const getProfileDetails = (profile: Profile): string => {
  if (profile.type === ProfileType.FIXED && profile.speed_duty != null) {
    return `${profile.speed_duty}%`
  } else if (profile.type === ProfileType.GRAPH && profile.temp_source != null) {
    return `${profile.temp_source.temp_name}`
  } else {
    return ''
  }
}

</script>

<template>
  <ConfirmDialog/>
  <div class="card">
    <div class="grid p-0 m-0 align-items-end justify-content-center card-container">
      <div class="col table-wrapper" style="padding-top: 0.5rem;">
        <Button rounded icon="pi pi-plus" label="New" aria-label="Create New Profile" size="small"
                @click="createNewProfile" class="mb-3"/>
        <DataTable v-model:selection="selectedProfile" :value="settingsStore.profiles" data-key="uid"
                   :meta-key-selection="false" selection-mode="single" @row-reorder="profilesReordered"
                   size="small" @row-select="rowSelected">
          <Column row-reorder header-style="width: 2.5rem"/>
          <Column field="name" header="Name"/>
          <Column field="type" header="Type" header-style="width: 6rem">
            <template #body="slotProps">
              <Tag :value="slotProps.data.type"/>
            </template>
          </Column>
          <Column>
            <template #body="slotProps">
              {{ getProfileDetails(slotProps.data) }}
            </template>
          </Column>
          <Column header-style="width: 3rem">
            <template #body="slotProps">
              <ProfileOptions :profile="slotProps.data"/>
            </template>
          </Column>
        </DataTable>
      </div>
    </div>
  </div>
  <Transition name="fade">
    <div v-if="selectedProfile!=null" class="card">
      <ProfileEditor :key="selectedProfile.uid" :profile-u-i-d="selectedProfile.uid"
                     @profile-change="(changed: boolean) => currentProfileChanged = changed"/>
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