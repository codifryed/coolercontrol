<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2021-2025  Guy Boldon and contributors
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
import { mdiArrowLeft } from '@mdi/js'
import Button from 'primevue/button'
import { Function, FunctionType, getFunctionTypeDisplayName } from '@/models/Profile.ts'
import { useI18n } from 'vue-i18n'
import { DEFAULT_NAME_STRING_LENGTH, useDeviceStore } from '@/stores/DeviceStore.ts'
import { computed, ref, type Ref } from 'vue'
import InputText from 'primevue/inputtext'
import Select from 'primevue/select'
import { $enum } from 'ts-enum-util'
import InputNumber from 'primevue/inputnumber'
import { ElSwitch } from 'element-plus'
import 'element-plus/es/components/switch/style/css'

interface Props {
    name: string // newProfileName
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'newFunction', fun: Function): void
}>()

const { t } = useI18n()
const deviceStore = useDeviceStore()

const dutyMin: number = 1
const dutyMax: number = 100
const windowSizeMin: number = 1
const windowSizeMax: number = 16
const devianceMin: number = 0
const devianceMax: number = 100
const delayMin: number = 0
const delayMax: number = 30

const newFunction = new Function(
    t('components.wizards.fanControl.newFunctionName', { profileName: props.name }),
    FunctionType.Standard,
)
const currentFunction: Ref<Function> = ref(newFunction)

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

const selectedType: Ref<FunctionType> = ref(newFunction.f_type)
const functionTypeOptions = computed(() => {
    return [...$enum(FunctionType).values()].map((type) => ({
        value: type,
        label: getFunctionTypeDisplayName(type),
    }))
})
const nameInput: Ref<string> = ref(newFunction.name)
const nameInvalid = computed(() => {
    return nameInput.value.length < 1 || nameInput.value.length > DEFAULT_NAME_STRING_LENGTH
})
const chosenDutyMinimum: Ref<number> = ref(currentFunction.value.duty_minimum)
const chosenDutyMaximum: Ref<number> = ref(currentFunction.value.duty_maximum)
const chosenWindowSize: Ref<number> = ref(startingWindowSize)
const chosenDelay: Ref<number> = ref(startingDelay)
const chosenDeviance: Ref<number> = ref(startingDeviance)
const chosenOnlyDownward: Ref<boolean> = ref(startingOnlyDownward)

const nextStep = async (): Promise<void> => {
    if (currentFunction.value.uid === '0') {
        console.error('Changing of the default Function is not allowed.')
        return
    }
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

    emit('newFunction', currentFunction.value)
    emit('nextStep', 13)
}
</script>

<template>
    <div class="flex flex-col justify-between min-w-96 w-[40vw] min-h-max h-[40vh]">
        <div class="flex flex-col gap-y-4">
            <div class="w-full">
                {{ t('components.wizards.fanControl.chooseFunctionNameType') }}:
            </div>
            <div class="mt-0 flex flex-col">
                <InputText
                    v-model="nameInput"
                    :placeholder="t('common.name')"
                    ref="inputArea"
                    id="property-name"
                    class="w-full h-11"
                    :invalid="nameInvalid"
                    :input-style="{ background: 'rgb(var(--colors-bg-one))' }"
                />
            </div>
            <div class="mt-0 flex flex-col">
                <small class="ml-2 mb-1 font-light text-sm">
                    {{ t('views.functions.functionType') }}
                </small>
                <Select
                    v-model="selectedType"
                    :options="functionTypeOptions"
                    option-label="label"
                    option-value="value"
                    :placeholder="t('views.functions.functionType')"
                    class="w-full h-11 mr-3 bg-bg-one !justify-end"
                    dropdown-icon="pi pi-chart-line"
                    scroll-height="400px"
                    checkmark
                />
            </div>
            <p>
                <span v-html="t('views.functions.functionTypeTooltip')" />
            </p>
            <div class="pr-1 w-full border-border-one border-2 rounded-lg">
                <table class="m-0.5 w-full bg-bg-two">
                    <tbody>
                        <tr
                            class="w-full"
                            v-tooltip.right="t('views.functions.minimumAdjustmentTooltip')"
                        >
                            <td
                                class="py-4 px-4 w-48 text-right items-center border-border-one border-r-2 border-b-2"
                            >
                                {{ t('views.functions.minimumAdjustment') }}
                            </td>
                            <td
                                class="py-4 px-2 text-center items-center border-border-one border-l-2 border-b-2"
                            >
                                <InputNumber
                                    v-model="chosenDutyMinimum"
                                    class="min-duty-input"
                                    show-buttons
                                    :min="dutyMin"
                                    :max="chosenDutyMaximum - 1"
                                    :suffix="` ${t('common.percentUnit')}`"
                                    button-layout="horizontal"
                                    :input-style="{ width: '5rem' }"
                                >
                                    <template #incrementicon>
                                        <span class="pi pi-plus" />
                                    </template>
                                    <template #decrementicon>
                                        <span class="pi pi-minus" />
                                    </template>
                                </InputNumber>
                            </td>
                        </tr>
                        <tr v-tooltip.right="t('views.functions.maximumAdjustmentTooltip')">
                            <td
                                class="py-4 px-4 w-48 text-right items-center border-border-one border-r-2 border-t-2"
                            >
                                {{ t('views.functions.maximumAdjustment') }}
                            </td>
                            <td
                                class="py-4 px-2 text-center items-center border-border-one border-l-2 border-t-2"
                            >
                                <InputNumber
                                    v-model="chosenDutyMaximum"
                                    class="max-duty-input"
                                    show-buttons
                                    :min="chosenDutyMinimum + 1"
                                    :max="dutyMax"
                                    :suffix="` ${t('common.percentUnit')}`"
                                    button-layout="horizontal"
                                    :input-style="{ width: '5rem' }"
                                >
                                    <template #incrementicon>
                                        <span class="pi pi-plus" />
                                    </template>
                                    <template #decrementicon>
                                        <span class="pi pi-minus" />
                                    </template>
                                </InputNumber>
                            </td>
                        </tr>
                        <tr
                            v-if="selectedType === FunctionType.ExponentialMovingAvg"
                            v-tooltip.right="t('views.functions.windowSizeTooltip')"
                        >
                            <td
                                class="py-4 px-4 w-48 text-right items-center border-border-one border-r-2 border-t-2"
                            >
                                {{ t('views.functions.windowSize') }}
                            </td>
                            <td
                                class="py-4 px-2 text-center items-center border-border-one border-l-2 border-t-2"
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
                                    <template #incrementicon>
                                        <span class="pi pi-plus" />
                                    </template>
                                    <template #decrementicon>
                                        <span class="pi pi-minus" />
                                    </template>
                                </InputNumber>
                            </td>
                        </tr>
                        <tr
                            v-if="selectedType === FunctionType.Standard"
                            v-tooltip.right="t('views.functions.hysteresisThresholdTooltip')"
                        >
                            <td
                                class="py-4 px-4 w-48 text-right items-center border-border-one border-r-2 border-t-2"
                            >
                                {{ t('views.functions.hysteresisThreshold') }}
                            </td>
                            <td
                                class="py-4 px-2 text-center items-center border-border-one border-l-2 border-t-2"
                            >
                                <InputNumber
                                    v-model="chosenDeviance"
                                    class="deviance-input"
                                    show-buttons
                                    :suffix="` ${t('common.tempUnit')}`"
                                    :step="0.1"
                                    :min="devianceMin"
                                    :max="devianceMax"
                                    :min-fraction-digits="1"
                                    :max-fraction-digits="1"
                                    button-layout="horizontal"
                                    :input-style="{ width: '5rem' }"
                                >
                                    <template #incrementicon>
                                        <span class="pi pi-plus" />
                                    </template>
                                    <template #decrementicon>
                                        <span class="pi pi-minus" />
                                    </template>
                                </InputNumber>
                            </td>
                        </tr>
                        <tr
                            v-if="selectedType === FunctionType.Standard"
                            v-tooltip.right="t('views.functions.hysteresisDelayTooltip')"
                        >
                            <td
                                class="py-4 px-4 w-48 text-right items-center border-border-one border-r-2 border-t-2"
                            >
                                {{ t('views.functions.hysteresisDelay') }}
                            </td>
                            <td
                                class="py-4 px-2 text-center items-center border-border-one border-l-2 border-t-2"
                            >
                                <InputNumber
                                    v-model="chosenDelay"
                                    class="delay-input"
                                    show-buttons
                                    :suffix="` ${t('common.secondAbbr')}`"
                                    :min="delayMin"
                                    :max="delayMax"
                                    button-layout="horizontal"
                                    :input-style="{ width: '5rem' }"
                                >
                                    <template #incrementicon>
                                        <span class="pi pi-plus" />
                                    </template>
                                    <template #decrementicon>
                                        <span class="pi pi-minus" />
                                    </template>
                                </InputNumber>
                            </td>
                        </tr>
                        <tr
                            v-if="selectedType === FunctionType.Standard"
                            v-tooltip.right="t('views.functions.onlyDownwardTooltip')"
                        >
                            <td
                                class="py-4 px-4 w-48 text-right items-center border-border-one border-r-2 border-t-2"
                            >
                                {{ t('views.functions.onlyDownward') }}
                            </td>
                            <td
                                class="py-4 px-2 text-center items-center border-border-one border-l-2 border-t-2"
                            >
                                <el-switch v-model="chosenOnlyDownward" size="large" />
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
        </div>
        <div class="flex flex-row justify-between mt-4">
            <Button class="w-24 bg-bg-one" label="Back" @click="emit('nextStep', 10)">
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiArrowLeft"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
            <Button
                class="w-24 bg-bg-one"
                :label="t('common.next')"
                :disabled="currentFunction == null || nameInvalid"
                @click="nextStep"
            />
        </div>
    </div>
</template>

<style scoped lang="scss">
.el-switch {
    --el-switch-on-color: rgb(var(--colors-accent));
    --el-switch-off-color: rgb(var(--colors-bg-one));
    --el-color-white: rgb(var(--colors-bg-two));
}
</style>
