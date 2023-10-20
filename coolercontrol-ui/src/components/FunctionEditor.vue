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
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import {mdiContentSaveMoveOutline} from "@mdi/js"
import {type UID} from "@/models/Device";
import {useSettingsStore} from "@/stores/SettingsStore";
import {computed, ref, type Ref} from "vue";
import {$enum} from "ts-enum-util";

interface Props {
  functionUID: UID
}

const props = defineProps<Props>()

const settingsStore = useSettingsStore()

const currentFunction = computed(() => settingsStore.functions.find((fun) => fun.uid === props.functionUID)!)
const givenName: Ref<string> = ref(currentFunction.value.name)
const selectedType: Ref<FunctionType> = ref(currentFunction.value.f_type)
const functionTypes = [...$enum(FunctionType).keys()]
    .filter(t => t === FunctionType.Identity || t === FunctionType.ExponentialMovingAvg) // only allow these for now

const saveFunctionState = () => {
  currentFunction.value.name = givenName.value
  currentFunction.value.f_type = selectedType.value
  // todo: save other values when appropriate (only save applicable values for specific function types)
  settingsStore.saveFunctions()
}
</script>

<template>
  <div class="grid">
    <div class="col-fixed" style="width: 220px">
      <span class="p-float-label mt-2">
        <InputText id="name" v-model="givenName" class="w-full"/>
        <label for="name">Name</label>
      </span>
      <div class="p-float-label mt-4">
        <Dropdown v-model="selectedType" inputId="dd-function-type" :options="functionTypes"
                  placeholder="Type" class="w-full"/>
        <label for="dd-function-type">Type</label>
      </div>
      <div class="align-content-end">
        <div class="mt-6">
          <Button label="Apply" size="small" rounded @click="saveFunctionState">
            <svg-icon class="p-button-icon p-button-icon-left pi" type="mdi" :path="mdiContentSaveMoveOutline"
                      size="1.35rem"/>
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