<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2021-2024  Guy Boldon and contributors
  -
  - This program is free software: you can redistribute it and/or modify
  - it under the terms of the GNU General Public License as published by
  - the Free Software Foundation, either version 3 of the License, or
  - (at your option) any later version.
  -
  - This program is distributed in the hope that it will be useful,
  - but WITHOUT ANY WARRANTY; without even the implied warranty of
  - MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  - GNU General Public License for more details.
  -
  - You should have received a copy of the GNU General Public License
  - along with this program.  If not, see <https://www.gnu.org/licenses/>.
  -->

<script setup lang="ts">
import { FunctionType } from '@/models/Profile'
import Button from 'primevue/button'
import InputText from 'primevue/inputtext'
import Dropdown from 'primevue/dropdown'
import { type UID } from '@/models/Device'
import { useSettingsStore } from '@/stores/SettingsStore'
import { computed, inject, nextTick, ref, type Ref } from 'vue'
import { $enum } from 'ts-enum-util'
import { useToast } from 'primevue/usetoast'
import InputNumber from 'primevue/inputnumber'
import SelectButton from 'primevue/selectbutton'
import type { DynamicDialogInstance } from 'primevue/dynamicdialogoptions'

interface Props {
    functionUID: UID
}

const dialogRef: Ref<DynamicDialogInstance> = inject('dialogRef')!
const props: Props = dialogRef.value.data

const settingsStore = useSettingsStore()
const toast = useToast()

const currentFunction = computed(
    () => settingsStore.functions.find((fun) => fun.uid === props.functionUID)!,
)
let startingWindowSize = 8 // 8 is the recommended default
if (
    currentFunction.value.sample_window != null &&
    (currentFunction.value.sample_window > 0 || currentFunction.value.sample_window <= 16)
) {
    startingWindowSize = currentFunction.value.sample_window
}
let startingDelay = currentFunction.value.response_delay ?? 1
let startingDeviance = currentFunction.value.deviance ?? 2
let startingOnlyDownward = currentFunction.value.only_downward ?? false

const givenName: Ref<string> = ref(currentFunction.value.name)
const selectedType: Ref<FunctionType> = ref(currentFunction.value.f_type)
const chosenDutyMinimum: Ref<number> = ref(currentFunction.value.duty_minimum)
const chosenDutyMaximum: Ref<number> = ref(currentFunction.value.duty_maximum)
const chosenWindowSize: Ref<number> = ref(startingWindowSize)
const chosenDelay: Ref<number> = ref(startingDelay)
const chosenDeviance: Ref<number> = ref(startingDeviance)
const chosenOnlyDownward: Ref<boolean> = ref(startingOnlyDownward)
const functionTypes = [...$enum(FunctionType).keys()]
const enabledOptions = [
    { value: true, label: 'Enabled' },
    { value: false, label: 'Disabled' },
]

const saveFunctionState = async () => {
    currentFunction.value.name = givenName.value
    currentFunction.value.f_type = selectedType.value
    currentFunction.value.duty_minimum = chosenDutyMinimum.value
    currentFunction.value.duty_maximum = chosenDutyMaximum.value
    currentFunction.value.sample_window =
        selectedType.value === FunctionType.ExponentialMovingAvg
            ? chosenWindowSize.value
            : undefined
    currentFunction.value.response_delay =
        selectedType.value === FunctionType.Standard ? chosenDelay.value : undefined
    currentFunction.value.deviance =
        selectedType.value === FunctionType.Standard ? chosenDeviance.value : undefined
    currentFunction.value.only_downward =
        selectedType.value === FunctionType.Standard ? chosenOnlyDownward.value : undefined
    const successful = await settingsStore.updateFunction(currentFunction.value.uid)
    if (successful) {
        toast.add({
            severity: 'success',
            summary: 'Success',
            detail: 'Function successfully updated and applied to affected devices',
            life: 3000,
        })
        dialogRef.value.close()
    } else {
        toast.add({
            severity: 'error',
            summary: 'Error',
            detail: 'There was an error attempting to update this Function',
            life: 3000,
        })
    }
}

const applyButton = ref()
nextTick(async () => {
    const delay = () => new Promise((resolve) => setTimeout(resolve, 100))
    await delay()
    applyButton.value.$el.focus()
})
</script>

<template>
    <div class="grid">
        <div class="col-fixed" style="width: 18rem">
            <span class="p-float-label mt-4">
                <InputText id="name" v-model="givenName" class="w-full" />
                <label for="name">Name</label>
            </span>
            <div class="p-float-label mt-5">
                <Dropdown
                    v-model="selectedType"
                    inputId="dd-function-type"
                    :options="functionTypes"
                    placeholder="Type"
                    class="w-full"
                    scroll-height="400px"
                />
                <label for="dd-function-type">Type</label>
            </div>
            <div class="p-float-label mt-5">
                <InputNumber
                    v-model="chosenDutyMinimum"
                    showButtons
                    :min="1"
                    :max="chosenDutyMaximum - 1"
                    class="w-full"
                    :input-style="{ width: '58px' }"
                    suffix=" %"
                    v-tooltip.left="{
                        value:
                            'Defines the minimum change step. Note that this can be overridden if the applied ' +
                            'duty hasn\'t changed and the target duty hasn\'t been met within 10 seconds. This enables meeting the ' +
                            'desired fan curve over time while still allowing step control.',
                        showDelay: 300,
                    }"
                />
                <label>Minimum Duty Change</label>
            </div>
            <div class="p-float-label mt-5">
                <InputNumber
                    v-model="chosenDutyMaximum"
                    showButtons
                    :min="chosenDutyMinimum + 1"
                    :max="100"
                    class="w-full"
                    :input-style="{ width: '58px' }"
                    suffix=" %"
                    v-tooltip.left="{
                        value: 'The maximum duty difference to apply. Defines the maximum change step. ',
                        showDelay: 300,
                    }"
                />
                <label>Maximum Duty Change</label>
            </div>
            <div
                v-if="selectedType === FunctionType.ExponentialMovingAvg"
                class="p-float-label mt-5"
            >
                <InputNumber
                    v-model="chosenWindowSize"
                    showButtons
                    :min="1"
                    :max="16"
                    class="w-full"
                    :input-style="{ width: '58px' }"
                    v-tooltip.left="{
                        value:
                            'The window size used to calculate an exponential moving average. ' +
                            'Smaller window sizes adjust more rapidly to temperature changes.',
                        showDelay: 300,
                    }"
                />
                <label>Window Size</label>
            </div>
            <template v-else-if="selectedType === FunctionType.Standard">
                <div class="label-wrapper mt-3" style="font-size: 0.9rem">
                    <label>Hysteresis Controls:</label>
                </div>
                <div class="p-float-label mt-5">
                    <InputNumber
                        v-model="chosenDeviance"
                        showButtons
                        :min="0"
                        :max="100"
                        class="w-full"
                        :input-style="{ width: '58px' }"
                        suffix=" °C"
                        v-tooltip.left="{
                            value: 'How many degrees of temperature change needed before applying a fan speed change.',
                            showDelay: 300,
                        }"
                    />
                    <label>Threshold</label>
                </div>
                <div class="p-float-label mt-5">
                    <InputNumber
                        v-model="chosenDelay"
                        showButtons
                        :min="0"
                        :max="30"
                        class="w-full"
                        :input-style="{ width: '58px' }"
                        suffix=" seconds"
                        v-tooltip.left="{
                            value: 'The response time in seconds to temperature changes.',
                            showDelay: 300,
                        }"
                    />
                    <label>Response Time</label>
                </div>
                <div class="mt-3">
                    <div class="label-wrapper">
                        <label>Only On Way Down</label>
                    </div>
                    <SelectButton
                        v-model="chosenOnlyDownward"
                        :options="enabledOptions"
                        option-label="label"
                        option-value="value"
                        :allow-empty="false"
                        class="w-full mt-2"
                        :pt="{ label: { style: 'width: 4.4rem' } }"
                        v-tooltip.left="{
                            value: 'Whether to apply these settings only when the temperature decreases',
                            showDelay: 300,
                        }"
                    />
                </div>
            </template>
            <div class="align-content-end">
                <div class="mt-6">
                    <Button
                        ref="applyButton"
                        label="Apply"
                        class="w-full"
                        @click="saveFunctionState"
                    >
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
.label-wrapper {
    margin-left: 0.75rem;
    margin-bottom: 0.25rem;
    padding: 0;
    font-size: 0.75rem;
    color: var(--text-color-secondary);
}
</style>
