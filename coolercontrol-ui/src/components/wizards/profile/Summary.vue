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
import {
    Function,
    Profile,
    ProfileMixFunctionType,
    ProfileTempSource,
    ProfileType,
} from '@/models/Profile.ts'
import { UID } from '@/models/Device.ts'
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
    name: string // profileName
    type: ProfileType
    speedFixed?: number
    memberIds: Array<UID>
    mixFunction?: ProfileMixFunctionType
    tempSource?: ProfileTempSource
    speedProfile: Array<[number, number]>
    functionUID: UID
    newFunction?: Function
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

const createNewFunction: boolean = props.newFunction != null
const functionName: string = createNewFunction
    ? props.newFunction!.name
    : (settingsStore.functions.find((fun) => fun.uid === props.functionUID)?.name ?? 'Unknown')

const removeLocallyCreatedFunction = (): void => {
    const functionIndex: number = settingsStore.functions.findIndex(
        (fun) => fun.uid === props.newFunction!.uid,
    )
    if (functionIndex === -1) {
        console.error('Function not found for removal: ' + functionName)
        return
    }
    if (props.newFunction!.uid === '0') {
        return // can't delete default
    }
    settingsStore.functions.splice(functionIndex, 1)
}
const saveProfileAndFunction = async (): Promise<void> => {
    if (createNewFunction) {
        if (props.newFunction == null) {
            console.error("Missing newFunction. This shouldn't happen.")
            return
        }
        settingsStore.functions.push(props.newFunction)
        const functionSuccess = await settingsStore.saveFunction(props.newFunction.uid)
        if (functionSuccess) {
            emitter.emit('function-add-menu', { functionUID: props.newFunction.uid })
        } else {
            removeLocallyCreatedFunction()
            console.error('Function could not be saved. Cannot Save Wizard Entities.')
            return
        }
    }

    const newProfile = new Profile(props.name, props.type)
    newProfile.speed_fixed = props.speedFixed
    newProfile.member_profile_uids = props.memberIds
    newProfile.mix_function_type = props.mixFunction
    newProfile.temp_source = props.tempSource
    newProfile.speed_profile = props.speedProfile
    newProfile.function_uid = createNewFunction ? props.newFunction!.uid : props.functionUID
    settingsStore.profiles.push(newProfile)
    const profileSuccess = await settingsStore.saveProfile(newProfile.uid)
    if (!profileSuccess) {
        console.error('Profile could not be created')
        if (createNewFunction) {
            removeLocallyCreatedFunction()
            await settingsStore.deleteFunction(props.newFunction!.uid)
        }
        return
    }
    emitter.emit('profile-add-menu', { profileUID: newProfile.uid })
    toast.add({
        severity: 'success',
        summary: t('common.success'),
        detail: t('views.profiles.createProfile'),
        life: 3000,
    })
    emit('close')
    await router.push({
        name: 'profiles',
        params: { profileUID: newProfile.uid },
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
                    {{ t('components.wizards.fanControl.aNewProfile') }}:
                    <span class="font-bold">{{ props.name }}</span>
                    <br />
                    <span v-if="props.type === ProfileType.Graph">
                        {{ t('components.wizards.fanControl.andFunction') }}:
                        <span class="font-bold">{{ functionName }}</span>
                        <br />
                    </span>
                    {{ t('components.wizards.profile.willCreated') }}
                </p>
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
                class="bg-accent/80 hover:!bg-accent w-32"
                :label="t('common.apply')"
                v-tooltip.bottom="t('views.speed.applySetting')"
                @click="saveProfileAndFunction"
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
