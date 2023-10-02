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
import Menu from "primevue/menu";
import ConfirmDialog from 'primevue/confirmdialog';
import {useConfirm} from "primevue/useconfirm";

const settingsStore = useSettingsStore()

const selectedProfile: Ref<Profile | undefined> = ref()

const createNewProfile = (): void => {
  // const nextId = settingsStore.profiles.reduce((previous, current) => previous && previous > current ? previous : current)
  const newId: number = settingsStore.profiles.slice(-1)[0].id + 1
  const newProfile = new Profile(
      newId,
      ProfileType.DEFAULT,
      `New Profile ${newId}`,
      true,
      [],
  )
  settingsStore.profiles.push(newProfile)
}

const duplicateProfile = (profileToDuplicate: Profile): void => {
  const newId: number = settingsStore.profiles.slice(-1)[0].id + 1
  const newProfile = new Profile(
      newId,
      profileToDuplicate.type,
      `Copy of ${profileToDuplicate.name}`,
      profileToDuplicate.reset_to_default,
      profileToDuplicate.speed_profile,
      profileToDuplicate.speed_duty,
      profileToDuplicate.temp_source,
  )
  settingsStore.profiles.push(newProfile)
}

const optionsMenu = ref()
const currentOptionsMenuProfile = ref()
const currentProfileChanged = ref(false)
const confirm = useConfirm()
const optionsToggle = (event: any, currentProfile: Profile) => {
  optionsMenu.value.toggle(event);
  currentOptionsMenuProfile.value = currentProfile
};
const selectProfile = (currentlySelectedProfile: Profile) => {
  if (currentlySelectedProfile.id === 0) {
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

// todo: dynamic profileOptions (disable delete)
const profileOptions = ref([
  // {
  //   label: 'Edit',
  //   icon: 'pi pi-pencil',
  //   command: () => {
  //   },
  // },
  {
    label: 'Duplicate',
    icon: 'pi pi-copy',
    command: () => duplicateProfile(currentOptionsMenuProfile.value),
  },
  {
    label: 'Delete',
    icon: 'pi pi-trash',
    command: () => settingsStore.profiles.splice(
        settingsStore.profiles.findIndex((profile) => profile.id === currentOptionsMenuProfile.value.id),
        1
    ),
  }
])

</script>

<template>
  <ConfirmDialog/>
  <div class="card">
    <div class="grid p-0 m-0 align-items-end justify-content-center card-container">
      <div class="col p-0">
        <Carousel :value="settingsStore.profiles" :num-visible="3" :num-scroll="1">
          <template #item="slotProps">
            <Card @click="selectProfile(slotProps.data)" class="mx-2"
                  :style="{'cursor': (slotProps.data.id != 0) ? 'pointer' : 'hand'}">
              <template #title>{{ slotProps.data.name }}</template>
              <template #subtitle>{{ ProfileType[slotProps.data.type] }}</template>
              <template #content></template>
              <template #footer>
                <div class="flex">
                  <Button aria-label="Profile Card Options" icon="pi pi-ellipsis-v" rounded text plain size="small"
                          class="ml-auto p-3"
                          style="height: 0.1rem; width: 0.1rem; box-shadow: none;" type="button" aria-haspopup="true"
                          @click.stop.prevent="optionsToggle($event, slotProps.data)"/>
                  <Menu ref="optionsMenu" :model="profileOptions" :popup="true" class="w-8rem">
                    <template #item="{ label, item, props }">
                      <a class="flex" v-bind="props.action">
                        <span v-bind="props.icon"/><span v-bind="props.label">{{ label }}</span>
                      </a>
                    </template>
                  </Menu>
                </div>
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
      <!--      <Transition name="fade"> some kind of issue with this transition. Disabled for now.-->
      <ProfileEditor :key="selectedProfile.id" :profile-id="selectedProfile.id"
                     @profile-change="currentProfileChanged = true"/>
      <!--      </Transition>-->
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
</style>