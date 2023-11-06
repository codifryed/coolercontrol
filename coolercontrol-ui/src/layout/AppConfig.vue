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
import Button from 'primevue/button'
import Sidebar from 'primevue/sidebar'
import SelectButton from 'primevue/selectbutton'
import Divider from 'primevue/divider'
import InputNumber from 'primevue/inputnumber'

import {type Ref, ref} from 'vue'
import {useLayout} from '@/layout/composables/layout'
import {useDeviceStore} from "@/stores/DeviceStore";
import {useSettingsStore} from "@/stores/SettingsStore";

defineProps({
  simple: {
    type: Boolean,
    default: true
  }
})

const scales = ref([12, 13, 14, 15, 16, 17, 18, 19, 20])

const {changeThemeSettings, setScale, layoutConfig, onConfigButtonClick, isConfigSidebarActive} = useLayout()

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const appVersion = import.meta.env.PACKAGE_VERSION

// todo: refactor this to be able to switch our dark & light theme:
// const onChangeTheme = (theme, mode) => {
//   const elementId = 'theme-css';
//   const linkElement = document.getElementById(elementId);
//   const cloneLinkElement = linkElement.cloneNode(true);
//   const newThemeUrl = linkElement.getAttribute('href').replace(layoutConfig.theme.value, theme);
//   cloneLinkElement.setAttribute('id', elementId + '-clone');
//   cloneLinkElement.setAttribute('href', newThemeUrl);
//   cloneLinkElement.addEventListener('load', () => {
//     linkElement.remove();
//     cloneLinkElement.setAttribute('id', elementId);
//     changeThemeSettings(theme, mode === 'dark');
//   });
//   linkElement.parentNode.insertBefore(cloneLinkElement, linkElement.nextSibling);
// };

const decrementScale = () => {
  setScale(layoutConfig.scale.value - 1)
  applyScale()
}
const incrementScale = () => {
  setScale(layoutConfig.scale.value + 1)
  applyScale()
}
const applyScale = () => {
  document.documentElement.style.fontSize = layoutConfig.scale.value + 'px'
}

const enabledOptions = [
  {value: true, label: 'Enabled'},
  {value: false, label: 'Disabled'},
]
const menuLayoutOptions = ['static', 'overlay']
const noInitOptions = [
  {value: false, label: 'Enabled'},
  {value: true, label: 'Disabled'},
]

</script>

<template>
  <Sidebar v-model:visible="isConfigSidebarActive" position="right"
           :transitionOptions="'.3s cubic-bezier(0, 0, 0.2, 1)'" class="layout-config-sidebar w-30rem">
    <h3>CoolerControl</h3>
    v{{ appVersion }}
    <p>
      This program comes with absolutely no warranty.
    </p>
    <Divider/>

    <h6>UI Scale</h6>
    <div class="flex align-items-center">
      <Button icon="pi pi-minus" type="button" @click="decrementScale()"
              class="p-button-text p-button-rounded w-2rem h-2rem mr-2"
              :disabled="layoutConfig.scale.value === scales[0]"></Button>
      <div class="flex gap-2 align-items-center">
        <i class="pi pi-circle-fill text-300" v-for="s in scales" :key="s"
           :class="{ 'text-primary-500': s === layoutConfig.scale.value }"></i>
      </div>
      <Button icon="pi pi-plus" type="button" pButton @click="incrementScale()"
              class="p-button-text p-button-rounded w-2rem h-2rem ml-2"
              :disabled="layoutConfig.scale.value === scales[scales.length - 1]"></Button>
    </div>

    <h6>Menu Type</h6>
    <div class="flex">
      <SelectButton v-model="layoutConfig.menuMode.value" :options="menuLayoutOptions"
                    :option-label="(value: string) => deviceStore.toTitleCase(value)"
                    :unselectable="true"/>
    </div>

    <h6>Apply Settings on System Boot</h6>
    <div class="flex">
      <SelectButton v-model="settingsStore.ccSettings.apply_on_boot" :options="enabledOptions" option-label="label"
                    option-value="value" :unselectable="true"
                    v-tooltip.left="'Whether to apply your settings automatically when the daemon starts'"/>
      <!--      :allowEmpty="false"-->
    </div>

    <h6>Liquidctl Device Initialization</h6>
    <div class="flex">
      <SelectButton v-model="settingsStore.ccSettings.no_init" :options="noInitOptions" option-label="label"
                    option-value="value" :unselectable="true"
                    v-tooltip.left="'Disabling this can help avoid conflicts with other programs that also control ' +
                     'your liquidctl devices. Most devices require this step for proper communication and should ' +
                      'only be disabled with care.'"/>
      <!--      :allowEmpty="false"-->
    </div>

    <h6>Boot-Up Delay</h6>
    <div class="flex">
      <InputNumber v-model="settingsStore.ccSettings.startup_delay" showButtons :min="1" :max="10" suffix=" seconds"
                   v-tooltip.left="'The number of seconds the daemon waits before attempting to communicate ' +
                    'with devices. This can be helpful when dealing with devices that aren\'t consistently detected' +
                     ' or need extra time to fully initialize.'"/>
    </div>

    <h6>Thinkpad Full Speed</h6>
    <div class="flex">
      <SelectButton v-model="settingsStore.ccSettings.thinkpad_full_speed" :options="enabledOptions"
                    option-label="label" option-value="value" :unselectable="true"
                    v-tooltip.left="'For Thinkpad Laptops this enables Full-Speed mode. This allows the fans to ' +
                     'spin up to their absolute maximum when set to 100%, but will run the fans out of ' +
                      'specification and cause increased wear. Use with caution.'"/>
    </div>

    <!--    todo: enable thinkpad fan control helper-->

    <!--    todo: Blacklisted Device List-->

    <!--<button class="p-link w-2rem h-2rem" @click="onChangeTheme('lara-dark-teal', 'dark')">-->
    <!--    <img src="/layout/images/themes/lara-dark-teal.png" class="w-2rem h-2rem" alt="Lara Dark Teal" />-->
    <!--</button>-->
  </Sidebar>
</template>

<style lang="scss" scoped></style>
