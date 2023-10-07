<script setup lang="ts">
import 'reflect-metadata'
import {RouterView} from 'vue-router'
import ProgressSpinner from "primevue/progressspinner"
import {onMounted, ref} from "vue"
import {useDeviceStore} from "@/stores/DeviceStore"
import {useSettingsStore} from "@/stores/SettingsStore"

let loading = ref(true)
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

/**
 * Startup procedure for the application.
 */
onMounted(async () => {
  // for testing:
  // const sleep = ms => new Promise(r => setTimeout(r, ms));
  // await sleep(3000);
  const initSuccessful = await deviceStore.initializeDevices()
  if (initSuccessful) {
    loading.value = false
  } else {
    // todo: we need to popup a dialog to notify the user about connection issues and give hints
    return
  }
  // todo: handle other startup processes

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
</template>

<style>
</style>
