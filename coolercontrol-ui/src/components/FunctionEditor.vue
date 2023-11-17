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
import {FunctionType} from "@/models/Profile"
import Button from 'primevue/button'
import InputText from 'primevue/inputtext'
import Dropdown from 'primevue/dropdown'
import {type UID} from "@/models/Device"
import {useSettingsStore} from "@/stores/SettingsStore"
import {computed, ref, type Ref} from "vue"
import {$enum} from "ts-enum-util"
import {useToast} from "primevue/usetoast"
import InputNumber from "primevue/inputnumber"

interface Props {
  functionUID: UID
}

const props = defineProps<Props>()

const settingsStore = useSettingsStore()
const toast = useToast()

const currentFunction = computed(() => settingsStore.functions.find((fun) => fun.uid === props.functionUID)!)
let startingWindowSize = 8 // 8 is the recommended default
if (currentFunction.value.sample_window != null && (currentFunction.value.sample_window > 0 || currentFunction.value.sample_window <= 16)) {
  startingWindowSize = currentFunction.value.sample_window
}

const givenName: Ref<string> = ref(currentFunction.value.name);
const selectedType: Ref<FunctionType> = ref(currentFunction.value.f_type)
const chosenWindowSize: Ref<number> = ref(startingWindowSize)
const functionTypes = [...$enum(FunctionType).keys()]
    // todo: remove for Hysteresis:
    .filter(t => t === FunctionType.Identity || t === FunctionType.ExponentialMovingAvg) // only allow these for now

const saveFunctionState = async () => {
  currentFunction.value.name = givenName.value
  currentFunction.value.f_type = selectedType.value
  currentFunction.value.sample_window = selectedType.value === FunctionType.ExponentialMovingAvg
      ? chosenWindowSize.value
      : undefined

  const successful = await settingsStore.updateFunction(currentFunction.value.uid)
  if (successful) {
    toast.add({
      severity: 'success',
      summary: 'Success',
      detail: 'Function successfully updated and applied to affected devices',
      life: 3000
    })
  } else {
    toast.add({severity: 'error', summary: 'Error', detail: 'There was an error attempting to update this Function', life: 3000})
  }
}
</script>

<template>
  <div class="grid">
    <div class="col-fixed" style="width: 16rem">
      <span class="p-float-label mt-4">
        <InputText id="name" v-model="givenName" class="w-full"/>
        <label for="name">Name</label>
      </span>
      <div class="p-float-label mt-5">
        <Dropdown v-model="selectedType" inputId="dd-function-type" :options="functionTypes"
                  placeholder="Type" class="w-full" scroll-height="flex"/>
        <label for="dd-function-type">Type</label>
      </div>
      <div v-if="selectedType === FunctionType.ExponentialMovingAvg" class="p-float-label mt-5">
        <InputNumber v-model="chosenWindowSize" showButtons :min="1" :max="16" class="w-full" :input-style="{width: '58px'}"
                     v-tooltip.left="{value: 'The window size used to calculate an exponential moving average. ' +
                      'Smaller window sizes adjust more rapidly to temperature changes.', showDelay: 300}"/>
        <label>Window Size</label>
      </div>
      <div class="align-content-end">
        <div class="mt-6">
          <Button label="Apply" class="w-full" @click="saveFunctionState">
            <span class="p-button-label">Apply</span>
          </Button>
        </div>
      </div>
    </div>
    <div class="col">
      <!--todo: perhaps fill in some kind of graph preview to see the kind of changes/differences visually-->
    </div>
  </div>
</template>

<style scoped lang="scss">

</style>