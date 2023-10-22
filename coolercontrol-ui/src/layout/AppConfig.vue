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

<script setup>
import RadioButton from 'primevue/radiobutton';
import Button from 'primevue/button';
import InputSwitch from 'primevue/inputswitch';
import Sidebar from 'primevue/sidebar';

import {ref} from 'vue';
import {useLayout} from '@/layout/composables/layout';

defineProps({
  simple: {
    type: Boolean,
    default: true
  }
});
const scales = ref([12, 13, 14, 15, 16]);

const {changeThemeSettings, setScale, layoutConfig, onConfigButtonClick, isConfigSidebarActive} = useLayout();

const appVersion = import.meta.env.PACKAGE_VERSION

// todo: change this to be able to switch our dark & light theme. (at first disable light - needs work)
const onChangeTheme = (theme, mode) => {
  const elementId = 'theme-css';
  const linkElement = document.getElementById(elementId);
  const cloneLinkElement = linkElement.cloneNode(true);
  const newThemeUrl = linkElement.getAttribute('href').replace(layoutConfig.theme.value, theme);
  cloneLinkElement.setAttribute('id', elementId + '-clone');
  cloneLinkElement.setAttribute('href', newThemeUrl);
  cloneLinkElement.addEventListener('load', () => {
    linkElement.remove();
    cloneLinkElement.setAttribute('id', elementId);
    changeThemeSettings(theme, mode === 'dark');
  });
  linkElement.parentNode.insertBefore(cloneLinkElement, linkElement.nextSibling);
};
const decrementScale = () => {
  setScale(layoutConfig.scale.value - 1);
  applyScale();
};
const incrementScale = () => {
  setScale(layoutConfig.scale.value + 1);
  applyScale();
};
const applyScale = () => {
  document.documentElement.style.fontSize = layoutConfig.scale.value + 'px';
};
</script>

<template>
  <Sidebar v-model:visible="isConfigSidebarActive" position="right"
           :transitionOptions="'.3s cubic-bezier(0, 0, 0.2, 1)'" class="layout-config-sidebar w-30rem">
    <h5>Scale</h5>
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

    <template v-if="!simple">
      <h5>Menu Type</h5>
      <div class="flex">
        <div class="field-radiobutton flex-1">
          <RadioButton name="menuMode" value="static" v-model="layoutConfig.menuMode.value"
                       inputId="mode1"></RadioButton>
          <label for="mode1">Static</label>
        </div>

        <div class="field-radiobutton flex-1">
          <RadioButton name="menuMode" value="overlay" v-model="layoutConfig.menuMode.value"
                       inputId="mode2"></RadioButton>
          <label for="mode2">Overlay</label>
        </div>
      </div>
    </template>

    <template v-if="!simple">
      <h5>Input Style</h5>
      <div class="flex">
        <div class="field-radiobutton flex-1">
          <RadioButton name="inputStyle" value="outlined" v-model="layoutConfig.inputStyle.value"
                       inputId="outlined_input"></RadioButton>
          <label for="outlined_input">Outlined</label>
        </div>
        <div class="field-radiobutton flex-1">
          <RadioButton name="inputStyle" value="filled" v-model="layoutConfig.inputStyle.value"
                       inputId="filled_input"></RadioButton>
          <label for="filled_input">Filled</label>
        </div>
      </div>

      <h5>Ripple Effect</h5>
      <InputSwitch v-model="layoutConfig.ripple.value"></InputSwitch>
    </template>

    <!--        <h5>PrimeOne Design - 2022</h5>-->
    <!--        <div class="grid">-->
    <!--            <div class="col-3">-->
    <!--                <button class="p-link w-2rem h-2rem" @click="onChangeTheme('lara-light-indigo', 'light')">-->
    <!--                    <img src="/layout/images/themes/lara-light-indigo.png" class="w-2rem h-2rem" alt="Lara Light Indigo" />-->
    <!--                </button>-->
    <!--            </div>-->
    <!--            <div class="col-3">-->
    <!--                <button class="p-link w-2rem h-2rem" @click="onChangeTheme('lara-light-blue', 'light')">-->
    <!--                    <img src="/layout/images/themes/lara-light-blue.png" class="w-2rem h-2rem" alt="Lara Light Blue" />-->
    <!--                </button>-->
    <!--            </div>-->
    <!--            <div class="col-3">-->
    <!--                <button class="p-link w-2rem h-2rem" @click="onChangeTheme('lara-light-purple', 'light')">-->
    <!--                    <img src="/layout/images/themes/lara-light-purple.png" class="w-2rem h-2rem" alt="Lara Light Purple" />-->
    <!--                </button>-->
    <!--            </div>-->
    <!--            <div class="col-3">-->
    <!--                <button class="p-link w-2rem h-2rem" @click="onChangeTheme('lara-light-teal', 'light')">-->
    <!--                    <img src="/layout/images/themes/lara-light-teal.png" class="w-2rem h-2rem" alt="Lara Light Teal" />-->
    <!--                </button>-->
    <!--            </div>-->
    <!--            <div class="col-3">-->
    <!--                <button class="p-link w-2rem h-2rem" @click="onChangeTheme('lara-dark-indigo', 'dark')">-->
    <!--                    <img src="/layout/images/themes/lara-dark-indigo.png" class="w-2rem h-2rem" alt="Lara Dark Indigo" />-->
    <!--                </button>-->
    <!--            </div>-->
    <!--            <div class="col-3">-->
    <!--                <button class="p-link w-2rem h-2rem" @click="onChangeTheme('lara-dark-blue', 'dark')">-->
    <!--                    <img src="/layout/images/themes/lara-dark-blue.png" class="w-2rem h-2rem" alt="Lara Dark Blue" />-->
    <!--                </button>-->
    <!--            </div>-->
    <!--            <div class="col-3">-->
    <!--                <button class="p-link w-2rem h-2rem" @click="onChangeTheme('lara-dark-purple', 'dark')">-->
    <!--                    <img src="/layout/images/themes/lara-dark-purple.png" class="w-2rem h-2rem" alt="Lara Dark Purple" />-->
    <!--                </button>-->
    <!--            </div>-->
    <!--            <div class="col-3">-->
    <!--                <button class="p-link w-2rem h-2rem" @click="onChangeTheme('lara-dark-teal', 'dark')">-->
    <!--                    <img src="/layout/images/themes/lara-dark-teal.png" class="w-2rem h-2rem" alt="Lara Dark Teal" />-->
    <!--                </button>-->
    <!--            </div>-->
    <!--          <span class="font-lite text-sm ml-auto mr-5">v{{appVersion}}</span>-->
    <!--        </div>-->
  </Sidebar>
</template>

<style lang="scss" scoped></style>
