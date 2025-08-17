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
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import Select from 'primevue/select'
import { computed, ref, Ref } from 'vue'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { Profile, ProfileType } from '@/models/Profile.ts'
import { UID } from '@/models/Device.ts'
import InputNumber from 'primevue/inputnumber'
import Slider from 'primevue/slider'

interface Props {
    name: string
    memberIds: Array<UID>
    offsetProfile: Array<[number, number]>
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'memberIds', memberIds: Array<UID>): void
    (e: 'offsetProfile', offsetProfile: Array<[number, number]>): void
}>()

const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const offsetMemberProfileOptions: Ref<Array<Profile>> = computed(() =>
    settingsStore.profiles.filter(
        (profile) => profile.p_type === ProfileType.Graph || profile.p_type === ProfileType.Mix,
    ),
)
const chosenOverlayMemberProfile: Ref<Profile | undefined> = ref(
    props.memberIds.length != 1
        ? undefined
        : settingsStore.profiles.find((profile) => profile.uid === props.memberIds[0]),
)
const chosenOverlayOffsetType: Ref<string> = ref(
    props.offsetProfile == null || props.offsetProfile.length < 2 ? 'static' : 'graph',
)
const overlayOffsetTypeOptions: Array<{ value: string; label: string }> = [
    {
        value: 'static',
        label: t('views.profiles.offsetTypeStatic'),
    },
    {
        value: 'graph',
        label: t('views.profiles.offsetTypeGraph'),
    },
]
const selectedStaticOffset: Ref<number> = ref(0)
const offsetMin: number = -100
const offsetMax: number = 100
const staticOffsetPrefix = computed(() =>
    selectedStaticOffset.value != null && selectedStaticOffset.value > 0 ? '+' : '',
)
const nextStep = () => {
    if (chosenOverlayMemberProfile.value == null) {
        return
    }
    emit('memberIds', [chosenOverlayMemberProfile.value!.uid])
    if (chosenOverlayOffsetType.value === 'static') {
        emit('offsetProfile', [[50, selectedStaticOffset.value ?? 0]])
    }
    const nextStep = chosenOverlayOffsetType.value === 'graph' ? 101 : 13
    emit('nextStep', nextStep)
}
</script>

<template>
    <div class="flex flex-col justify-between min-w-96 w-[40vw] min-h-max h-[40vh]">
        <div class="flex flex-col gap-y-4">
            <div class="w-full mb-2">
                {{ t('components.wizards.fanControl.newOverlayProfile') }}:
                <span class="font-bold">{{ props.name }}</span>
                <br />
                {{ t('components.wizards.fanControl.withSettings') }}:
            </div>
            <div class="mt-0 flex flex-col">
                <small class="ml-2 mb-1 font-light text-sm">
                    {{ t('views.profiles.baseProfile') }}
                </small>
                <Select
                    v-model="chosenOverlayMemberProfile"
                    :options="offsetMemberProfileOptions"
                    option-label="name"
                    :placeholder="t('views.profiles.baseProfile')"
                    class="w-full mr-3 h-11 bg-bg-one !justify-end"
                    checkmark
                    scroll-height="40rem"
                    dropdown-icon="pi pi-chart-line"
                    :invalid="chosenOverlayMemberProfile == null"
                    v-tooltip.bottom="t('views.profiles.baseProfile')"
                />
            </div>
            <div class="mt-0 flex flex-col">
                <small class="ml-2 mb-1 font-light text-sm">
                    {{ t('views.profiles.offsetType') }}
                </small>
                <Select
                    v-model="chosenOverlayOffsetType"
                    :options="overlayOffsetTypeOptions"
                    option-label="label"
                    option-value="value"
                    :placeholder="t('views.profiles.offsetType')"
                    class="w-full mr-3 h-11 bg-bg-one !justify-end"
                    checkmark
                    dropdown-icon="pi pi-sliders-v"
                    scroll-height="40rem"
                    v-tooltip.bottom="t('views.profiles.offsetType')"
                />
            </div>
            <div class="mt-0 flex flex-col" v-if="chosenOverlayOffsetType === 'static'">
                <small class="ml-2 mb-1 font-light text-sm">
                    {{ t('views.profiles.staticOffset') }}
                </small>
                <InputNumber
                    :placeholder="t('common.offset')"
                    v-model="selectedStaticOffset"
                    mode="decimal"
                    class="duty-input h-11 w-full"
                    :suffix="` ${t('common.percentUnit')}`"
                    :prefix="staticOffsetPrefix"
                    showButtons
                    :min="offsetMin"
                    :max="offsetMax"
                    :use-grouping="false"
                    :step="1"
                    button-layout="horizontal"
                    :input-style="{ width: '8rem', background: 'rgb(var(--colors-bg-one))' }"
                    :disabled="chosenOverlayMemberProfile == null"
                >
                    <template #incrementicon>
                        <span class="pi pi-plus" />
                    </template>
                    <template #decrementicon>
                        <span class="pi pi-minus" />
                    </template>
                </InputNumber>
                <div class="mx-1.5 mt-0">
                    <Slider
                        v-model="selectedStaticOffset"
                        class="!w-full"
                        :step="1"
                        :min="offsetMin"
                        :max="offsetMax"
                        :disabled="chosenOverlayMemberProfile == null"
                    />
                </div>
            </div>
        </div>
        <div class="flex flex-row justify-between mt-4">
            <Button class="w-24 bg-bg-one" label="Back" @click="emit('nextStep', 3)">
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
                :disabled="chosenOverlayMemberProfile == null"
                @click="nextStep"
            />
        </div>
    </div>
</template>

<style scoped lang="scss"></style>
