<script setup>
import {ref, onBeforeMount, watch} from 'vue';
import {useRoute} from 'vue-router';
import {useLayout} from '@/layout/composables/layout';
import Button from 'primevue/button';
import Menu from 'primevue/menu';

const route = useRoute();

const {layoutConfig, layoutState, setActiveMenuItem, onMenuToggle} = useLayout();

const props = defineProps({
  item: {
    type: Object,
    default: () => ({})
  },
  index: {
    type: Number,
    default: 0
  },
  root: {
    type: Boolean,
    default: true
  },
  parentItemKey: {
    type: String,
    default: null
  }
});

const isActiveMenu = ref(true);
const itemKey = ref(null);

onBeforeMount(() => {
  itemKey.value = props.parentItemKey ? props.parentItemKey + '-' + props.index : String(props.index);

  // todo: enable simple toggle in the menu, not only one open and all open on start
  // const activeItem = layoutState.activeMenuItem;

  // isActiveMenu.value = activeItem === itemKey.value || activeItem ? activeItem.startsWith(itemKey.value + '-') : false;
});

// watch(
//     () => layoutConfig.activeMenuItem.value,
//     (newVal) => {
//       isActiveMenu.value = newVal === itemKey.value || newVal.startsWith(itemKey.value + '-');
//     }
// );
const itemClick = (event, item) => {
  if (item.disabled) {
    event.preventDefault();
    return;
  }

  const {overlayMenuActive, staticMenuMobileActive} = layoutState;

  if ((item.to || item.url) && (staticMenuMobileActive.value || overlayMenuActive.value)) {
    onMenuToggle();
  }

  if (item.command) {
    item.command({originalEvent: event, item: item});
  }

  // const foundItemKey = item.items ? (isActiveMenu.value ? props.parentItemKey : itemKey) : itemKey.value;

  // setActiveMenuItem(foundItemKey);
  isActiveMenu.value = !isActiveMenu.value;  // very simply toggle
  // todo: save state
};

const optionsMenu = ref();
const optionsToggle = (event) => {
  optionsMenu.value.toggle(event);
};
</script>

<template>
  <li :class="{ 'layout-root-menuitem': root, 'active-menuitem': isActiveMenu }">
    <div v-if="root && item.visible !== false" class="layout-menuitem-root-text">{{ item.label }}</div>
    <a v-if="(!item.to || item.items) && item.visible !== false" :href="item.url"
       @click="itemClick($event, item, index)"
       :class="item.class" :target="item.target" tabindex="0">
      <i :class="item.icon" class="layout-menuitem-icon" :style="item.iconStyle"></i>
      <span class="layout-menuitem-text">{{ item.label }}</span>
      <span class="layout-menuitem-text ml-auto"></span>
      <i class="pi pi-fw pi-angle-down layout-submenu-toggler" v-if="item.items"></i>
      <Button v-if="item.options" aria-label="options" icon="pi pi-ellipsis-v" rounded text plain size="small"
              class="ml-1 p-3 channel-options"
              style="height: 0.1rem; width: 0.1rem; box-shadow: none; visibility: hidden"
              type="button" aria-haspopup="true" @click.stop.prevent="optionsToggle"/>
      <Button v-else icon="pi pi-ellipsis-v" rounded text plain size="small"
              class="ml-1 p-3"
              style="height: 0.1rem; width: 0.1rem; box-shadow: none; visibility: hidden"
              type="button"/>
      <Menu ref="optionsMenu" :model="item.options" :popup="true">
        <template #item="{ label, item, props }">
          <a class="flex" v-bind="props.action">
            <span v-bind="props.icon"/><span v-bind="props.label">{{ label }}</span>
          </a>
        </template>
      </Menu>
    </a>

    <router-link v-if="item.to && !item.items && item.visible !== false" @click="itemClick($event, item, index)"
                 :class="[item.class, 'device-channel']" exact-active-class="active-route" exact
                 tabindex="0" :to="item.to">
      <i :class="item.icon" class="layout-menuitem-icon" :style="item.iconStyle"></i>
      <span class="layout-menuitem-text">{{ item.label }}</span>
      <i class="pi pi-fw pi-angle-down layout-submenu-toggler" v-if="item.items"></i>
      <span v-if="item.temp" class="layout-menuitem-text ml-auto">{{ item.temp }}<span
          class="ml-1">°&nbsp;&nbsp;&nbsp;</span></span>
      <span v-else-if="(item.duty && !item.rpm && item.rpm !== 0)" class="layout-menuitem-text ml-auto text-right">
        {{ item.duty }}<span style="font-size: 0.7rem"> %&nbsp;&nbsp;&nbsp;</span>
      </span>
      <span v-else-if="(!item.duty && item.rpm != null)" class="layout-menuitem-text ml-auto text-right">
        {{ item.rpm }}<span style="font-size: 0.7rem">rpm</span>
      </span>
      <span v-else-if="(item.duty && item.rpm != null)" class="layout-menuitem-text ml-auto text-right">
        {{ item.duty }}<span class="ml-1" style="font-size: 0.7rem">%&nbsp;&nbsp;&nbsp;</span><br/>{{ item.rpm }}<span
          class="ml-1"
          style="font-size: 0.7rem">rpm</span>
      </span>
      <span v-else class="layout-menuitem-text ml-auto"></span>
      <Button v-if="item.options" aria-label="options" icon="pi pi-ellipsis-v" rounded text plain size="small"
              class="ml-1 p-3 channel-options"
              style="height: 0.1rem; width: 0.1rem; box-shadow: none; visibility: hidden"
              type="button" aria-haspopup="true" @click.stop.prevent="optionsToggle"/>
      <Button v-else icon="pi pi-ellipsis-v" rounded text plain size="small"
              class="ml-1 p-3"
              style="height: 0.1rem; width: 0.1rem; box-shadow: none; visibility: hidden"
              type="button"/>
      <Menu ref="optionsMenu" :model="item.options" :popup="true">
        <template #item="{ label, item, props }">
          <a class="flex" v-bind="props.action">
            <span v-bind="props.icon"/><span v-bind="props.label">{{ label }}</span>
          </a>
        </template>
      </Menu>
    </router-link>
    <Transition v-if="item.items && item.visible !== false" name="layout-submenu">
      <ul v-show="root ? true : isActiveMenu" class="layout-submenu">
        <!--      <ul v-show=true class="layout-submenu">-->
        <app-menu-item v-for="(child, i) in item.items" :key="child" :index="i" :item="child" :parentItemKey="itemKey"
                       :root="false">
        </app-menu-item>
      </ul>
    </Transition>
  </li>
</template>

<style lang="scss" scoped>
//
</style>
