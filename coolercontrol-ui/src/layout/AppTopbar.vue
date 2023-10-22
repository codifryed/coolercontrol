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
import {ref, computed, onMounted, onBeforeUnmount} from 'vue';
import {useLayout} from '@/layout/composables/layout';
import {useRouter} from 'vue-router';
import SvgIcon from '@jamescoyle/vue-icon'
import {mdiDotsVertical, mdiGitlab, mdiHelpCircleOutline, mdiMenu, mdiTune} from '@mdi/js'

const {layoutConfig, onMenuToggle, onConfigButtonClick} = useLayout();

const outsideClickListener = ref(null);
const topbarMenuActive = ref(false);
const router = useRouter();

onMounted(() => {
  bindOutsideClickListener();
});

onBeforeUnmount(() => {
  unbindOutsideClickListener();
});

const logoUrl = computed(() => {
  return `/layout/images/${layoutConfig.darkTheme.value ? 'logo-dark' : 'logo-dark'}.svg`;
});

const onTopBarMenuButton = () => {
  topbarMenuActive.value = !topbarMenuActive.value;
};

// const onSettingsClick = () => {
//     topbarMenuActive.value = false;
//     router.push('/documentation');
// };

const topbarMenuClasses = computed(() => {
  return {
    'layout-topbar-menu-mobile-active': topbarMenuActive.value
  };
});

const bindOutsideClickListener = () => {
  if (!outsideClickListener.value) {
    outsideClickListener.value = (event) => {
      if (isOutsideClicked(event)) {
        topbarMenuActive.value = false;
      }
    };
    document.addEventListener('click', outsideClickListener.value);
  }
};
const unbindOutsideClickListener = () => {
  if (outsideClickListener.value) {
    document.removeEventListener('click', outsideClickListener);
    outsideClickListener.value = null;
  }
};
const isOutsideClicked = (event) => {
  if (!topbarMenuActive.value) return;

  const sidebarEl = document.querySelector('.layout-topbar-menu');
  const topbarEl = document.querySelector('.layout-topbar-menu-button');

  return !(sidebarEl.isSameNode(event.target) || sidebarEl.contains(event.target) || topbarEl.isSameNode(event.target) || topbarEl.contains(event.target));
};
</script>

<template>
  <div class="layout-topbar">
    <!--        todo: enable only on mobile view:-->
    <button class="p-link layout-menu-button layout-topbar-button" @click="onMenuToggle()">
      <svg-icon type="mdi" :path="mdiMenu" size="1.5rem"/>
    </button>

    <button class="p-link layout-topbar-menu-button layout-topbar-button" @click="onTopBarMenuButton()">
      <svg-icon type="mdi" :path="mdiDotsVertical" size="1.5rem"/>
    </button>

    <div class="layout-topbar-logo">
      <router-link to="/" class="layout-topbar-logo">
        <img :src="logoUrl" alt="logo"/>
        <span style="font-family: rounded,serif;">CoolerControl</span>
      </router-link>
    </div>

    <div class="layout-topbar-menu" :class="topbarMenuClasses">
      <!--      <button @click="onTopBarMenuButton()" class="p-link layout-topbar-button">-->
      <!--        <i class="pi pi-calendar"></i>-->
      <!--        <span>Calendar</span>-->
      <!--      </button>-->
      <a href="https://gitlab.com/coolercontrol/coolercontrol" target="_blank">
        <button class="p-link layout-topbar-button">
          <svg-icon type="mdi" :path="mdiGitlab" size="1.5rem"/>
          <span>Project Page</span>
        </button>
      </a>
      <a href="https://gitlab.com/coolercontrol/coolercontrol/-/wikis/home" target="_blank">
        <button class="p-link layout-topbar-button">
          <svg-icon type="mdi" :path="mdiHelpCircleOutline" size="1.5rem"/>
          <span>Wiki</span>
        </button>
      </a>
      <button @click="onConfigButtonClick()" class="p-link layout-topbar-button">
        <svg-icon type="mdi" :path="mdiTune" size="1.5rem"/>
        <span>Settings</span>
      </button>
    </div>
  </div>
</template>

<style lang="scss" scoped></style>
