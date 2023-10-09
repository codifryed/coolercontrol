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
import Carousel from "primevue/carousel"
import Card from 'primevue/card'
import Button from 'primevue/button'
import ConfirmDialog from 'primevue/confirmdialog'
import {useConfirm} from "primevue/useconfirm"
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
const confirm = useConfirm()
const selectProfile = (currentlySelectedProfile: Profile) => {
  if (currentlySelectedProfile.orderId === 0) {
    return
  }
  if (currentProfileChanged.value) {
    confirm.require({
      message: 'You are about to discard changes made to the previous profile. Do you want to save them?',
      header: 'Discard Changes?',
      icon: 'pi pi-exclamation-triangle',
      accept: () => {
        // todo: save
        console.log('SAVED')
      },
      // reject: () => {
      //   // todo: nothing
      // }
    })
    currentProfileChanged.value = false
  }
  selectedProfile.value = currentlySelectedProfile;
  console.log("Profile selected")
}

const showProfileInfo = (data: Profile) => {
  if (data.type === ProfileType.FIXED) {
    return `${data.speed_duty}%`
  } else if (data.type === ProfileType.GRAPH) {
    return `${data.temp_source?.temp_name}`
  } else {
    return ''
  }
}

</script>

<template>
  <ConfirmDialog/>
  <div class="card">
    <div class="grid p-0 m-0 align-items-end justify-content-center card-container">
      <div class="col p-0 carousel-wrapper">
        <Carousel :value="settingsStore.profiles" :num-visible="3" :num-scroll="1">
          <template #item="slotProps">
            <Card @click="selectProfile(slotProps.data)" class="mx-2"
                  :style="{'cursor': (slotProps.data.orderId != 0) ? 'pointer' : 'hand'}">
              <template #title>{{ slotProps.data.name }}</template>
              <template #subtitle>{{ ProfileType[slotProps.data.type] }}</template>
              <template #content>{{ showProfileInfo(slotProps.data) }}&nbsp;</template>
              <template #footer>
                <ProfileOptions :profile="slotProps.data"/>
              </template>
            </Card>
          </template>
        </Carousel>
      </div>
      <div class="col-fixed" style="width: 60px">
        <Button rounded icon="pi pi-plus" outlined aria-label="Create New Profile" size="small"
                @click="createNewProfile"/>
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

.carousel-wrapper :deep(.p-carousel-item) {
  padding-bottom: 2px;
}

.carousel-wrapper :deep(.p-card-footer) {
  padding: 0;
}

.carousel-wrapper :deep(.p-card-content) {
  padding: 0;
}

.carousel-wrapper :deep(.p-card-body) {
  padding: 14px;
}
</style>