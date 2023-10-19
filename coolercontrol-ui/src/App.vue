<script setup lang="ts">
import 'reflect-metadata'
import {RouterView} from 'vue-router'
import ProgressSpinner from "primevue/progressspinner"
import {onMounted, ref} from "vue"
import {useDeviceStore} from "@/stores/DeviceStore"
import {useSettingsStore} from "@/stores/SettingsStore"
import Button from 'primevue/button'
import Dialog from 'primevue/dialog'
import Toast from "primevue/toast";

const loading = ref(true)
const initSuccessful = ref(true)
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const reloadPage = () => window.location.reload()

/**
 * Startup procedure for the application.
 */
onMounted(async () => {
  const sleep = (ms: number) => new Promise(r => setTimeout(r, ms))
  initSuccessful.value = await deviceStore.initializeDevices()
  if (!initSuccessful.value) {
    return
  }
  await settingsStore.initializeSettings(deviceStore.allDevices())
  await sleep(200) // give the engine a moment to catch up for a smoother start
  loading.value = false

  const delay = () => new Promise(resolve => setTimeout(resolve, 200))
  let timeStarted = Date.now()
  while (true) { // this will be automatically paused by the browser when going inactive/sleep
    if (Date.now() - timeStarted >= 1000) {
      timeStarted = Date.now()
      await deviceStore.updateStatus()
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
  <RouterView v-else/>
  <Toast/>
  <Dialog :visible="!initSuccessful" header="CoolerControl Connection Error" :style="{ width: '50vw' }">
    <p>
      A connection to the CoolerControl Daemon could not be established. <br/>
      Please make sure that the systemd service is running and available on port 11987.
    </p>
    <p>
      Check the <a href="https://gitlab.com/coolercontrol/coolercontrol/" style="color: var(--cc-context-color)">
      project page</a> for installation instructions.
    </p>
    <p>
      Some helpful commands:
    </p>
    <p>
      <code>
        sudo systemctl coolercontrold enable<br/>
        sudo systemctl coolercontrold restart<br/>
        sudo systemctl coolercontrold status<br/>
      </code>
    </p>
    <template #footer>
      <Button label="Retry" icon="pi pi-refresh" @click="reloadPage"/>
    </template>
  </Dialog>
</template>

<style>
@font-face {
  font-family: 'rounded';
  font-style: normal;
  font-weight: normal;
  src: local('Rounded Elegance Regular'), url('/Rounded_Elegance.woff') format('woff');
}

#app {
  /* Foreground, Background */
  scrollbar-color: var(--cc-context-pressed) var(--cc-bg-two);
}

::-webkit-scrollbar {
  width: 8px;
}

/* Track */
::-webkit-scrollbar-track { /* Background */
  -webkit-box-shadow: inset 0 0 4px rgba(0, 0, 0, 0.3);
  border-radius: 6px;
  background: var(--cc-bg-two);
}

/* Handle */
::-webkit-scrollbar-thumb { /* Foreground */
  border-radius: 6px;
  -webkit-box-shadow: inset 0 0 4px rgba(0, 0, 0, .3);
  background: var(--cc-context-pressed);
}

/* Handle on hover */
::-webkit-scrollbar-thumb:hover { /* Foreground Hover */
  background: var(--cc-context-color);
}
</style>
