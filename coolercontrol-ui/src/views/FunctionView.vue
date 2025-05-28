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
import { FunctionType, getFunctionTypeDisplayName } from '@/models/Profile'
import Button from 'primevue/button'
import { type UID } from '@/models/Device.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { computed, nextTick, onMounted, ref, type Ref, watch } from 'vue'
import { $enum } from 'ts-enum-util'
import { useToast } from 'primevue/usetoast'
import InputNumber from 'primevue/inputnumber'
import { mdiContentSaveOutline } from '@mdi/js'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'radix-vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import Listbox, { ListboxChangeEvent } from 'primevue/listbox'
import { ElSwitch } from 'element-plus'
import 'element-plus/es/components/switch/style/css'
import { onBeforeRouteLeave, onBeforeRouteUpdate } from 'vue-router'
import { useConfirm } from 'primevue/useconfirm'
import { useI18n } from 'vue-i18n'

interface Props {
    functionUID: UID
}

const props = defineProps<Props>()
const settingsStore = useSettingsStore()
const deviceStore = useDeviceStore()
const toast = useToast()
const confirm = useConfirm()
const { t } = useI18n()

let contextIsDirty: Ref<boolean> = ref(false)

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

const selectedType: Ref<FunctionType> = ref(currentFunction.value.f_type)
const chosenDutyMinimum: Ref<number> = ref(currentFunction.value.duty_minimum)
const chosenDutyMaximum: Ref<number> = ref(currentFunction.value.duty_maximum)
const chosenWindowSize: Ref<number> = ref(startingWindowSize)
const chosenDelay: Ref<number> = ref(startingDelay)
const chosenDeviance: Ref<number> = ref(startingDeviance)
const chosenOnlyDownward: Ref<boolean> = ref(startingOnlyDownward)
const functionTypeOptions = computed(() => {
    return [...$enum(FunctionType).values()].map((type) => ({
        value: type,
        label: getFunctionTypeDisplayName(type),
    }))
})

const saveFunctionState = async () => {
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
    const successful = await settingsStore.updateFunction(currentFunction.value.uid)
    if (successful) {
        contextIsDirty.value = false
        toast.add({
            severity: 'success',
            summary: t('common.success'),
            detail: t('views.functions.saveFunction'),
            life: 3000,
        })
    } else {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: t('views.functions.functionError'),
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
    selectedType.value = event.value.value // Get the value field
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

const checkForUnsavedChanges = (_to: any, _from: any, next: any): void => {
    if (!contextIsDirty.value) {
        next()
        return
    }
    confirm.require({
        message: t('views.functions.unsavedChanges'),
        header: t('views.functions.unsavedChangesHeader'),
        icon: 'pi pi-exclamation-triangle',
        defaultFocus: 'accept',
        rejectLabel: t('common.stay'),
        acceptLabel: t('common.discard'),
        accept: () => {
            next()
            contextIsDirty.value = false
        },
        reject: () => next(false),
    })
}

onMounted(async () => {
    addScrollEventListeners()
    // re-add some scroll event listeners for elements that are rendered on Type change
    watch(selectedType, () => {
        nextTick(addScrollEventListeners)
    })
    watch(
        [
            selectedType,
            chosenDutyMinimum,
            chosenDutyMaximum,
            chosenWindowSize,
            chosenDeviance,
            chosenDelay,
            chosenOnlyDownward,
        ],
        () => {
            contextIsDirty.value = true
        },
    )
    onBeforeRouteUpdate(checkForUnsavedChanges)
    onBeforeRouteLeave(checkForUnsavedChanges)
})
</script>

<template>
    <div class="flex border-b-4 border-border-one items-center justify-between">
        <div class="flex pl-4 py-2 text-2xl overflow-hidden">
            <span class="font-bold overflow-hidden overflow-ellipsis">{{
                currentFunction.name
            }}</span>
        </div>
        <div class="flex flex-wrap gap-x-1 justify-end">
            <div class="p-2">
                <Button
                    class="bg-accent/80 hover:!bg-accent w-32 h-[2.375rem]"
                    :class="{ 'animate-pulse-fast': contextIsDirty }"
                    :label="t('common.save')"
                    v-tooltip.bottom="t('views.functions.saveFunction')"
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
                    {{ t('views.functions.functionType') }}
                </small>
                <Listbox
                    :model-value="selectedType"
                    :options="functionTypeOptions"
                    class="w-full"
                    checkmark
                    placeholder="Type"
                    option-label="label"
                    list-style="max-height: 100%"
                    v-tooltip.right="t('views.functions.functionTypeTooltip')"
                    @change="changeFunctionType"
                />
            </div>
            <table class="mt-4 bg-bg-two rounded-lg">
                <tbody>
                    <tr v-tooltip.right="t('views.functions.minimumAdjustmentTooltip')">
                        <td
                            class="py-4 px-4 w-48 text-right items-center border-border-one border-r-2 border-b-2"
                        >
                            {{ t('views.functions.minimumAdjustment') }}
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
                            class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-t-2"
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
                            class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-t-2"
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
                            class="py-4 px-2 w-48 text-center items-center border-border-one border-l-2 border-t-2"
                        >
                            <InputNumber
                                v-model="chosenDelay"
                                class="delay-input"
                                show-buttons
                                :suffix="` ${t('common.secondAbbr')}`"
                                :min="0"
                                :max="30"
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
