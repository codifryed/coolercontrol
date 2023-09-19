<script setup>
import {computed, watch, ref, onMounted} from 'vue';
import AppTopbar from './AppTopbar.vue';
import AppSidebar from './AppSidebar.vue';
import AppConfig from './AppConfig.vue';
import {useLayout} from '@/layout/composables/layout';
import ProgressSpinner from 'primevue/progressspinner';
import {useDeviceStore} from '@/stores/DeviceStore'
import AppFooter from "@/layout/AppFooter.vue";

const {layoutConfig, layoutState, isSidebarActive} = useLayout();

const outsideClickListener = ref(null);

watch(isSidebarActive, (newVal) => {
  if (newVal) {
    bindOutsideClickListener();
  } else {
    unbindOutsideClickListener();
  }
});

const containerClass = computed(() => {
  return {
    'layout-theme-light': layoutConfig.darkTheme.value === 'light',
    'layout-theme-dark': layoutConfig.darkTheme.value === 'dark',
    'layout-overlay': layoutConfig.menuMode.value === 'overlay',
    'layout-static': layoutConfig.menuMode.value === 'static',
    'layout-static-inactive': layoutState.staticMenuDesktopInactive.value && layoutConfig.menuMode.value === 'static',
    'layout-overlay-active': layoutState.overlayMenuActive.value,
    'layout-mobile-active': layoutState.staticMenuMobileActive.value,
    'p-input-filled': layoutConfig.inputStyle.value === 'filled',
    'p-ripple-disabled': !layoutConfig.ripple.value
  };
});
const bindOutsideClickListener = () => {
  if (!outsideClickListener.value) {
    outsideClickListener.value = (event) => {
      if (isOutsideClicked(event)) {
        layoutState.overlayMenuActive.value = false;
        layoutState.staticMenuMobileActive.value = false;
        layoutState.menuHoverActive.value = false;
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
  const sidebarEl = document.querySelector('.layout-sidebar');
  const topbarEl = document.querySelector('.layout-menu-button');

  return !(sidebarEl.isSameNode(event.target) || sidebarEl.contains(event.target) || topbarEl.isSameNode(event.target) || topbarEl.contains(event.target));
};

let loading = ref(true);
const deviceStore = useDeviceStore();

onMounted(async () => {
  // for testing:
  // const sleep = ms => new Promise(r => setTimeout(r, ms));
  // await sleep(3000);
  const initSuccessful = await deviceStore.initializeDevices();
  loading.value = false;
  if (!initSuccessful) {
    // todo: we need to popup a dialog to notify the user about connection issues and give hints
  }

  const delay = () => new Promise(resolve => setTimeout(resolve, 100))
  let timeStarted = Date.now()
  while (true) {
    if (Date.now() - timeStarted > 1000) {
      timeStarted = Date.now()
      const fullStatusUpdate = await deviceStore.updateStatus()
      if (fullStatusUpdate) {
        deviceStore.fullStatusUpdate = fullStatusUpdate
        // we leave set to true until the Overview has reset the variable
      }
    }
    await delay()
  }
})
</script>

<template>
  <div v-if="loading">
    <div class="flex align-items-center align-items-stretch flex-wrap" style="min-height: 100vh">
      <ProgressSpinner/>
    </div>
  </div>
  <div v-else class="layout-wrapper" :class="containerClass">
    <app-topbar></app-topbar>
    <div class="layout-sidebar">
      <app-sidebar></app-sidebar>
    </div>
    <div class="layout-main-container">
      <div class="layout-main" ref="laymain">
        <router-view></router-view>
      </div>
      <app-footer></app-footer>
    </div>
    <app-config></app-config>
    <div class="layout-mask"></div>
  </div>
</template>

<style lang="scss" scoped>
    //height: 34
</style>
