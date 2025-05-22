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
import { Function } from '@/models/Profile.ts'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { mdiArrowLeft, mdiContentSaveOutline } from '@mdi/js'
import Button from 'primevue/button'
import { Emitter, EventType } from 'mitt'
import { inject } from 'vue'
import { v4 as uuidV4 } from 'uuid'
import { useToast } from 'primevue/usetoast'
import { useRouter } from 'vue-router'

interface Props {
    newFunction: Function
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'nextStep', step: number): void
    (e: 'close'): void
}>()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!

const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const toast = useToast()
const router = useRouter()

const removeLocallyCreatedFunction = (): void => {
    const functionIndex: number = settingsStore.functions.findIndex(
        (fun) => fun.uid === props.newFunction!.uid,
    )
    if (functionIndex === -1) {
        console.error('Function not found for removal: ' + props.newFunction.name)
        return
    }
    if (props.newFunction!.uid === '0') {
        return // can't delete default
    }
    settingsStore.functions.splice(functionIndex, 1)
}
const saveFunction = async (): Promise<void> => {
    settingsStore.functions.push(props.newFunction)
    const functionSuccess = await settingsStore.saveFunction(props.newFunction.uid)
    if (functionSuccess) {
        emitter.emit('function-add-menu', { functionUID: props.newFunction.uid })
    } else {
        removeLocallyCreatedFunction()
        console.error('Function could not be saved.')
        return
    }
    toast.add({
        severity: 'success',
        summary: t('common.success'),
        detail: t('views.functions.createFunction'),
        life: 3000,
    })
    emit('close')
    await router.push({
        name: 'functions',
        params: { functionUID: props.newFunction.uid },
        query: { key: uuidV4() },
    })
}
</script>

<template>
    <div class="flex flex-col justify-between min-w-96 w-[40vw] min-h-max h-[40vh]">
        <div class="flex flex-col gap-y-4">
            <span class="text-xl text-center underline">{{
                t('components.wizards.fanControl.summary')
            }}</span>
            <div class="w-full text-lg">
                <p>
                    {{ t('views.functions.newFunction') }}:
                    <span class="font-bold">{{ props.newFunction.name }}</span>
                    <br />
                    {{ t('components.wizards.profile.willCreated') }}
                </p>
            </div>
        </div>
        <div class="flex flex-row justify-between mt-4">
            <Button class="w-24 bg-bg-one" label="Back" @click="emit('nextStep', 11)">
                <svg-icon
                    class="outline-0"
                    type="mdi"
                    :path="mdiArrowLeft"
                    :size="deviceStore.getREMSize(1.5)"
                />
            </Button>
            <Button
                class="bg-accent/80 hover:!bg-accent w-32"
                :label="t('common.apply')"
                v-tooltip.bottom="t('views.speed.applySetting')"
                @click="saveFunction"
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
</template>

<style scoped lang="scss"></style>
