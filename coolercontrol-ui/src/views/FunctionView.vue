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
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { FunctionType } from '@/models/Profile.ts'
import Button from 'primevue/button'
import InputText from 'primevue/inputtext'
import { type UID } from '@/models/Device.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { computed, inject, nextTick, onMounted, ref, type Ref, watch } from 'vue'
import { $enum } from 'ts-enum-util'
import { useToast } from 'primevue/usetoast'
import InputNumber from 'primevue/inputnumber'
import { mdiContentSaveOutline } from '@mdi/js'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'radix-vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import Listbox, { ListboxChangeEvent } from 'primevue/listbox'
import { ElSwitch } from 'element-plus'
import 'element-plus/es/components/switch/style/css'
import { Emitter, EventType } from 'mitt'

interface Props {
    functionUID: UID
}

const props = defineProps<Props>()
const settingsStore = useSettingsStore()
const deviceStore = useDeviceStore()
const toast = useToast()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!

const dutyMin: number = 1
const dutyMax: number = 100
const windowSizeMin: number = 1
const windowSizeMax: number = 16
const devianceMin: number = 0
const devianceMax: number = 100
const delayMin: number = 0
const delayMax: number = 30

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
        emitter.emit('function-rename', currentFunction.value.uid)
    } else {
        toast.add({
            severity: 'error',
            summary: 'Error',
            detail: 'There was an error attempting to update this Function',
            life: 3000,
        })
    }
}

const minDutyScrolled = (event: WheelEvent) => {
    if (event.deltaY < 0) {
        if (chosenDutyMinimum.value < chosenDutyMaximum.value - 1) chosenDutyMinimum.value += 1
    } else {
        if (chosenDutyMinimum.value > dutyMin) chosenDutyMinimum.value -= 1
    }
}
const maxDutyScrolled = (event: WheelEvent) => {
    if (event.deltaY < 0) {
        if (chosenDutyMaximum.value < dutyMax) chosenDutyMaximum.value += 1
    } else {
        if (chosenDutyMaximum.value > chosenDutyMinimum.value + 1) chosenDutyMaximum.value -= 1
    }
}
const windowSizeScrolled = (event: WheelEvent) => {
    if (event.deltaY < 0) {
        if (chosenWindowSize.value < windowSizeMax) chosenWindowSize.value += 1
    } else {
        if (chosenWindowSize.value > windowSizeMin) chosenWindowSize.value -= 1
    }
}
const devianceScrolled = (event: WheelEvent) => {
    if (event.deltaY < 0) {
        if (chosenDeviance.value < devianceMax) chosenDeviance.value += 0.1
    } else {
        if (chosenDeviance.value > devianceMin) chosenDeviance.value -= 0.1
    }
}
const delayScrolled = (event: WheelEvent) => {
    if (event.deltaY < 0) {
        if (chosenDelay.value < delayMax) chosenDelay.value += 1
    } else {
        if (chosenDelay.value > delayMin) chosenDelay.value -= 1
    }
}

const changeFunctionType = (event: ListboxChangeEvent): void => {
    if (event.value === null) {
        return // do not update on unselect
    }
    selectedType.value = event.value
}

// const inputArea = ref()
// nextTick(async () => {
//     const delay = () => new Promise((resolve) => setTimeout(resolve, 100))
//     await delay()
//     inputArea.value.$el.focus()
// })

const addScrollEventListeners = (): void => {
    // @ts-ignore
    document?.querySelector('.min-duty-input')?.addEventListener('wheel', minDutyScrolled)
    // @ts-ignore
    document?.querySelector('.max-duty-input')?.addEventListener('wheel', maxDutyScrolled)
    // @ts-ignore
    document?.querySelector('.window-size-input')?.addEventListener('wheel', windowSizeScrolled)
    // @ts-ignore
    document?.querySelector('.deviance-input')?.addEventListener('wheel', devianceScrolled)
    // @ts-ignore
    document?.querySelector('.delay-input')?.addEventListener('wheel', delayScrolled)
}

onMounted(async () => {
    addScrollEventListeners()
    // re-add some scroll event listeners for elements that are rendered on Type change
    watch(selectedType, () => {
        nextTick(addScrollEventListeners)
    })
})
</script>

<template>
    <div class="flex border-b-4 border-border-one items-center justify-between">
        <div class="pl-4 py-2 text-xl">
            {{ givenName }}
        </div>
        <div class="flex justify-end">
            <div class="border-l-2 px-4 py-2 border-border-one flex flex-row">
                <Button
                    class="bg-accent/80 hover:!bg-accent w-32 h-[2.375rem]"
                    label="Save"
                    v-tooltip.bottom="'Save Function'"
                    @click="saveFunctionState"
                >
                    <svg-icon
                        class="outline-0"
                        type="mdi"
                        :path="mdiContentSaveOutline"
                        :size="deviceStore.getREMSize(1.5)"
                    />
                </Button>
            </div>
        </div>
    </div>
    <ScrollAreaRoot style="--scrollbar-size: 10px">
        <ScrollAreaViewport class="p-4 pb-16 h-screen w-full">
            <!--            <small class="mt-8 mb-4 ml-3 font-light text-sm text-text-color-secondary">-->
            <!--                Function Name-->
            <!--            </small>-->
            <!--            <div class="mt-1">-->
            <!--                <InputText-->
            <!--                    ref="inputArea"-->
            <!--                    id="name"-->
            <!--                    v-model="givenName"-->
            <!--                    class="w-96"-->
            <!--                    placeholder="Name"-->
            <!--                    @keydown.enter="saveFunctionState"-->
            <!--                    v-tooltip.right="'Function Name'"-->
            <!--                />-->
            <!--            </div>-->
            <div class="mt-0 mr-4 w-96">
                <small class="ml-3 font-light text-sm text-text-color-secondary">
                    Function Type
                </small>
                <Listbox
                    :model-value="selectedType"
                    :options="functionTypes"
                    class="w-full"
                    checkmark
                    placeholder="Type"
                    list-style="max-height: 100%"
                    v-tooltip.right="
                        'Function Type:\n- Identity: Doesn\'t change the calculated profile value.\n- Standard: Alters the profile value using an algorithm with hysteresis settings.\n- ExponentialMovingAvg: Alters the profile value using an exponential moving average algorithm.'
                    "
                    @change="changeFunctionType"
                />
            </div>
            <table class="mt-4 bg-bg-two rounded-lg">
                <tbody>
                    <tr
                        v-tooltip.right="
                            'Minimum fan speed adjustment: Calculated changes below this value will be ignored.'
                        "
                    >
                        <td
                            class="py-4 px-4 w-48 text-right items-center border-border-one border-r-2 border-b-2"
                        >
                            Minimum Adjustment
                        </td>
                        <td
                            class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-b-2"
                        >
                            <InputNumber
                                v-model="chosenDutyMinimum"
                                class="min-duty-input"
                                show-buttons
                                :min="dutyMin"
                                :max="chosenDutyMaximum - 1"
                                suffix=" %"
                                button-layout="horizontal"
                                :input-style="{ width: '5rem' }"
                            >
                                <template #incrementbuttonicon>
                                    <span class="pi pi-plus" />
                                </template>
                                <template #decrementbuttonicon>
                                    <span class="pi pi-minus" />
                                </template>
                            </InputNumber>
                        </td>
                    </tr>
                    <tr
                        v-tooltip.right="
                            'Maximum fan speed adjustment: Calculated changes above this threshold will be capped.'
                        "
                    >
                        <td
                            class="py-4 px-4 w-48 text-right items-center border-border-one border-r-2 border-t-2"
                        >
                            Maximum Adjustment
                        </td>
                        <td
                            class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                        >
                            <InputNumber
                                v-model="chosenDutyMaximum"
                                class="max-duty-input"
                                show-buttons
                                :min="chosenDutyMinimum + 1"
                                :max="dutyMax"
                                suffix=" %"
                                button-layout="horizontal"
                                :input-style="{ width: '5rem' }"
                            >
                                <template #incrementbuttonicon>
                                    <span class="pi pi-plus" />
                                </template>
                                <template #decrementbuttonicon>
                                    <span class="pi pi-minus" />
                                </template>
                            </InputNumber>
                        </td>
                    </tr>
                    <tr
                        v-if="selectedType === FunctionType.ExponentialMovingAvg"
                        v-tooltip.right="
                            'Adjust the sensitivity of temperature changes by setting the window size.\n' +
                            'A smaller window size responds quickly to changes,\nwhile a larger size provides a smoother average.'
                        "
                    >
                        <td
                            class="py-4 px-4 w-48 text-right items-center border-border-one border-r-2 border-t-2"
                        >
                            Window Size
                        </td>
                        <td
                            class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                        >
                            <InputNumber
                                v-model="chosenWindowSize"
                                class="window-size-input"
                                show-buttons
                                :min="windowSizeMin"
                                :max="windowSizeMax"
                                button-layout="horizontal"
                                :input-style="{ width: '5rem' }"
                            >
                                <template #incrementbuttonicon>
                                    <span class="pi pi-plus" />
                                </template>
                                <template #decrementbuttonicon>
                                    <span class="pi pi-minus" />
                                </template>
                            </InputNumber>
                        </td>
                    </tr>
                    <tr
                        v-if="selectedType === FunctionType.Standard"
                        v-tooltip.right="
                            'Temperature change threshold (°C): adjust fan speed when temperature changes by this amount.'
                        "
                    >
                        <td
                            class="py-4 px-4 w-48 text-right items-center border-border-one border-r-2 border-t-2"
                        >
                            Hysteresis Threshold
                        </td>
                        <td
                            class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                        >
                            <InputNumber
                                v-model="chosenDeviance"
                                class="deviance-input"
                                show-buttons
                                suffix=" °C"
                                :step="0.1"
                                :min="devianceMin"
                                :max="devianceMax"
                                :min-fraction-digits="1"
                                :max-fraction-digits="1"
                                button-layout="horizontal"
                                :input-style="{ width: '5rem' }"
                            >
                                <template #incrementbuttonicon>
                                    <span class="pi pi-plus" />
                                </template>
                                <template #decrementbuttonicon>
                                    <span class="pi pi-minus" />
                                </template>
                            </InputNumber>
                        </td>
                    </tr>
                    <tr
                        v-if="selectedType === FunctionType.Standard"
                        v-tooltip.right="
                            'Time taken to respond to temperature changes (in seconds).'
                        "
                    >
                        <td
                            class="py-4 px-4 w-48 text-right items-center border-border-one border-r-2 border-t-2"
                        >
                            Hysteresis Delay
                        </td>
                        <td
                            class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                        >
                            <InputNumber
                                v-model="chosenDelay"
                                class="delay-input"
                                show-buttons
                                suffix=" s"
                                :min="0"
                                :max="30"
                                button-layout="horizontal"
                                :input-style="{ width: '5rem' }"
                            >
                                <template #incrementbuttonicon>
                                    <span class="pi pi-plus" />
                                </template>
                                <template #decrementbuttonicon>
                                    <span class="pi pi-minus" />
                                </template>
                            </InputNumber>
                        </td>
                    </tr>
                    <tr
                        v-if="selectedType === FunctionType.Standard"
                        v-tooltip.right="'Apply settings only when temperature decreases.'"
                    >
                        <td
                            class="py-4 px-4 w-48 text-right items-center border-border-one border-r-2 border-t-2"
                        >
                            Only Downward
                        </td>
                        <td
                            class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                        >
                            <el-switch v-model="chosenOnlyDownward" size="large" />
                        </td>
                    </tr>
                </tbody>
            </table>
        </ScrollAreaViewport>
        <ScrollAreaScrollbar
            class="flex select-none touch-none p-0.5 bg-transparent transition-colors duration-[120ms] ease-out data-[orientation=vertical]:w-2.5"
            orientation="vertical"
        >
            <ScrollAreaThumb
                class="flex-1 bg-border-one opacity-80 rounded-lg relative before:content-[''] before:absolute before:top-1/2 before:left-1/2 before:-translate-x-1/2 before:-translate-y-1/2 before:w-full before:h-full before:min-w-[44px] before:min-h-[44px]"
            />
        </ScrollAreaScrollbar>
    </ScrollAreaRoot>
</template>

<style scoped lang="scss">
.el-switch {
    --el-switch-on-color: rgb(var(--colors-accent));
    --el-switch-off-color: rgb(var(--colors-bg-one));
    --el-color-white: rgb(var(--colors-bg-two));
    // switch active text color:
    --el-color-primary: rgb(var(--colors-text-color));
    // switch inactive text color:
    --el-text-color-primary: rgb(var(--colors-text-color));
}
</style>
