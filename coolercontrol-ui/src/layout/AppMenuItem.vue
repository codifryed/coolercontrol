<script setup>
import {defineAsyncComponent, onBeforeMount, ref} from 'vue';
import {useRoute} from 'vue-router';
import {useLayout} from '@/layout/composables/layout';
import Button from 'primevue/button';
import Menu from 'primevue/menu';
import {useDeviceStore} from "@/stores/DeviceStore";
import {useSettingsStore} from "@/stores/SettingsStore";
import {ElColorPicker} from 'element-plus';
import 'element-plus/es/components/color-picker/style/css';
import SvgIcon from "@jamescoyle/vue-icon";
import {useDialog} from 'primevue/usedialog'

const NameEditor = defineAsyncComponent(() => import('../components/NameEditor.vue'))
const route = useRoute();
const dialog = useDialog();

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

const deviceStore = useDeviceStore();
const settingsStore = useSettingsStore();
const isActiveMenu = ref(
    props.item.deviceUID != null
        ? !settingsStore.allUIDeviceSettings.get(props.item.deviceUID).menuCollapsed
        : true
);
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

const itemClick = (event, item, index) => {
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
  if (props.item.deviceUID != null && props.item.items != null && props.item.items.length > 0) {
    isActiveMenu.value = !isActiveMenu.value;  // very simply toggle
    settingsStore.allUIDeviceSettings.get(props.item.deviceUID).menuCollapsed = !isActiveMenu.value
  }
}

const deviceItemsValues = (deviceUID, channelName) => deviceStore.currentDeviceStatus.get(deviceUID)?.get(channelName);
const optionsMenu = ref();
const optionsToggle = (event) => {
  optionsMenu.value.toggle(event);
};
const color = ref(
    props.item.color
        ? settingsStore.allUIDeviceSettings.get(props.item.deviceUID).sensorsAndChannels.getValue(props.item.name).color
        : ''
);
const hideEnabled = ref(
    props.item.name != null // sensors and channels have specific names, and are hid-able
        ? settingsStore.allUIDeviceSettings
            .get(props.item.deviceUID).sensorsAndChannels
            .getValue(props.item.name)
            .hide
        : false
);
const optionButtonAction = (label) => {
  if (label.includes('Hide')) {
    hideEnabled.value = !hideEnabled.value;
    if (label === 'Hide All' || label === 'Show All') {
      for (const sensorChannel of settingsStore.allUIDeviceSettings
          .get(props.item.deviceUID).sensorsAndChannels
          .values()) {
        sensorChannel.hide = hideEnabled.value;
      }
      settingsStore.sidebarMenuUpdate()
    } else {
      settingsStore.allUIDeviceSettings
          .get(props.item.deviceUID).sensorsAndChannels
          .getValue(props.item.name)
          .hide = hideEnabled.value;
    }
  } else if (label === 'Rename') {
    const dialogRef = dialog.open(NameEditor, {
      props: {
        header: 'Edit Name',
        position: 'center',
        modal: true,
        dismissableMask: true,
      },
      data: {
        deviceUID: props.item.deviceUID,
        sensorName: props.item.name,
      },
      onClose: (options) => {
        const data = options.data
        if (data) {
          const isDeviceName = props.item.name == null
          if (data.newName) {
            data.newName = deviceStore.sanitizeString(data.newName)
            if (isDeviceName) {
              settingsStore.allUIDeviceSettings
                  .get(props.item.deviceUID)
                  .userName = data.newName
            } else {
              settingsStore.allUIDeviceSettings
                  .get(props.item.deviceUID).sensorsAndChannels
                  .getValue(props.item.name)
                  .userName = data.newName
            }
          } else {
            // reset
            if (isDeviceName) {
              settingsStore.allUIDeviceSettings
                  .get(props.item.deviceUID)
                  .userName = undefined
            } else {
              settingsStore.allUIDeviceSettings
                  .get(props.item.deviceUID).sensorsAndChannels
                  .getValue(props.item.name)
                  .userName = undefined
            }
          }
          props.item.label = isDeviceName
              ? settingsStore.allUIDeviceSettings.get(props.item.deviceUID).name
              : settingsStore.allUIDeviceSettings.get(props.item.deviceUID).sensorsAndChannels.getValue(props.item.name).name
        }
      }
    })
  }
}
const hideOrShowLabel = (label) => {
  if (label === "Hide" && hideEnabled.value) {
    return "Show";
  } else if (label === "Hide" && !hideEnabled.value) {
    return label;
  } else if (label === "Hide All" && hideEnabled.value) {
    return "Show All";
  } else if (label === "Hide All" && !hideEnabled.value) {
    return label;
  } else {
    return label;
  }
}

const setNewColor = (newColor) => {
  if (newColor == null) {
    settingsStore.allUIDeviceSettings
        .get(props.item.deviceUID).sensorsAndChannels
        .getValue(props.item.name)
        .userColor = undefined;
    color.value = settingsStore.allUIDeviceSettings
        .get(props.item.deviceUID).sensorsAndChannels
        .getValue(props.item.name)
        .defaultColor
  } else {
    settingsStore.allUIDeviceSettings
        .get(props.item.deviceUID).sensorsAndChannels
        .getValue(props.item.name)
        .userColor = newColor;
  }
}

settingsStore.$onAction(({name, after}) => {
  if (name === 'sidebarMenuUpdate') {
    after(() => {
      if (props.parentItemKey != null && props.parentItemKey.includes('-')) { // sensor/channel menu items only
        hideEnabled.value = settingsStore.allUIDeviceSettings
            .get(props.item.deviceUID).sensorsAndChannels
            .getValue(props.item.name)
            .hide
      }
    })
  }
});

</script>

<template>
  <li :class="{ 'layout-root-menuitem': root, 'active-menuitem': isActiveMenu }">
    <div v-if="root && item.visible !== false" class="layout-menuitem-root-text">{{ item.label }}</div>
    <a v-if="(!item.to || item.items) && item.visible !== false" :href="item.url"
       @click="itemClick($event, item, index)"
       :class="item.class" :target="item.target" tabindex="0">
      <!--      root element icon and label:-->
      <svg-icon class="layout-menuitem-icon" :style="item.iconStyle" type="mdi" :path="item.icon ?? ''" size="16"/>
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
      <!--      Options Menu for root elements:-->
      <Menu ref="optionsMenu" :model="item.options" :popup="true">
        <template #item="{ label, item, props }">
          <a class="flex p-menuitem-link" @click="optionButtonAction(label)">
            <span v-if="item.label.includes('Hide') && !hideEnabled" class="pi pi-fw pi-eye-slash mr-2"/>
            <span v-else-if="item.label.includes('Hide') && hideEnabled" class="pi pi-fw pi-eye mr-2"/>
            <span v-else v-bind="props.icon"/>
            <span v-bind="props.label">{{ hideOrShowLabel(label) }}</span>
          </a>
        </template>
      </Menu>
    </a>

    <router-link v-if="item.to && !item.items && item.visible !== false" @click="itemClick($event, item, index)"
                 :class="[item.class, 'device-channel']" :exact-active-class="hideEnabled ? '' : 'active-route'" exact
                 tabindex="0" :to="hideEnabled ? '' : item.to">
      <div v-if="item.color" class="color-wrapper pi pi-fw layout-menuitem-icon" @click.stop.prevent>
        <el-color-picker v-model="color" color-format="hex" :predefine="settingsStore.predefinedColorOptions"
                         @change="setNewColor" :disabled="hideEnabled"/>
      </div>
      <svg-icon v-else class="layout-menuitem-icon" :style="item.iconStyle" type="mdi" :path="item.icon ?? ''"
                size="16"/>
      <span class="layout-menuitem-text" :class="{'disabled-text': hideEnabled}">
        {{ item.label }}
      </span>
      <i class="pi pi-fw pi-angle-down layout-submenu-toggler" v-if="item.items"></i>
      <span v-if="item.temp" class="layout-menuitem-text ml-auto" :class="{'disabled-text': hideEnabled}">
        {{ deviceItemsValues(item.deviceUID, item.name).temp }}
        <span>°&nbsp;&nbsp;&nbsp;</span>
      </span>
      <span v-else-if="(item.duty != null && !item.rpm && item.rpm !== 0)"
            class="layout-menuitem-text ml-auto text-right" :class="{'disabled-text': hideEnabled}">
        {{ deviceItemsValues(item.deviceUID, item.name).duty }}
        <span style="font-size: 0.7rem">%&nbsp;&nbsp;&nbsp;&nbsp;</span>
      </span>
      <span v-else-if="(item.duty == null && item.rpm != null)" class="layout-menuitem-text ml-auto text-right"
            :class="{'disabled-text': hideEnabled}">
        {{ deviceItemsValues(item.deviceUID, item.name).rpm }}
        <span style="font-size: 0.7rem">rpm</span>
      </span>
      <span v-else-if="(item.duty != null && item.rpm != null)" class="layout-menuitem-text ml-auto text-right"
            :class="{'disabled-text': hideEnabled}">
        {{ deviceItemsValues(item.deviceUID, item.name).duty }}
        <span style="font-size: 0.7rem">%&nbsp;&nbsp;&nbsp;&nbsp;</span>
        <br/>
        {{ deviceItemsValues(item.deviceUID, item.name).rpm }}
        <span style="font-size: 0.7rem">rpm</span>
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
          <a class="flex p-menuitem-link" @click="optionButtonAction(item.label)">
            <span v-if="item.label === 'Hide' && hideEnabled" class="pi pi-fw pi-eye mr-2"/>
            <span v-else-if="item.label === 'Hide' && !hideEnabled" class="pi pi-fw pi-eye-slash mr-2"/>
            <span v-else v-bind="props.icon"/>
            <span v-bind="props.label">{{ hideOrShowLabel(label) }}</span>
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
.color-wrapper :deep(.el-color-picker__trigger) {
  border: 0 !important;
  padding: 0 !important;
  height: 14px !important;
  width: 14px !important;
}

.color-wrapper :deep(.el-color-picker__mask) {
  border: 0 !important;
  padding: 0 !important;
  height: 14px !important;
  width: 14px !important;
  top: 0;
  left: 0;
  background-color: rgba(0, 0, 0, .7);
}

.color-wrapper :deep(.el-color-picker__color) {
  border: 0 !important;
  //border-radius: 10px !important;
}

.color-wrapper :deep(.el-color-picker__color-inner) {
  border-radius: 4px !important;
}

.color-wrapper :deep(.el-color-picker .el-color-picker__icon) {
  display: none;
}

.disabled-text {
  opacity: 0.2;
}

.layout-menuitem-text {
  //max-width: 8em; // fallback if needed
  max-width: 18ch;
}
</style>

<style>
.el-color-picker__panel {
  padding: 14px;
  border-radius: 12px;
  background-color: var(--surface-card);
}

.el-color-picker__panel.el-popper {
  border-color: var(--surface-border);
}

.el-button {
  border-color: var(--surface-border);
  background-color: var(--cc-bg-two);
}

el-button:focus, .el-button:hover {
  color: var(--cc-text-active);
  border-color: var(--surface-border);
  background-color: var(--cc-bg-three);
}

.el-button.is-text:not(.is-disabled):focus, .el-button.is-text:not(.is-disabled):hover {
  background-color: var(--surface-card);
}

.el-input__wrapper {
  background-color: var(--cc-bg-three);
  box-shadow: none;
}

.el-input__inner {
  color: var(--cc-text-foreground);
}

.el-input__wrapper:hover, .el-input__wrapper:active, .el-input__wrapper:focus {
  box-shadow: var(--cc-context-color)
}
</style>
