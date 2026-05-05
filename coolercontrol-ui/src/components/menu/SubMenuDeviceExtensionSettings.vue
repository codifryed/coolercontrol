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
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiCogs, mdiInformationSlabCircleOutline } from '@mdi/js'
import { DeviceType, UID } from '@/models/Device.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import Button from 'primevue/button'
import InputNumber from 'primevue/inputnumber'
import Popover from 'primevue/popover'
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useConfirm } from 'primevue/useconfirm'
import { useToast } from 'primevue/usetoast'
import { ElSwitch } from 'element-plus'
import _ from 'lodash'
import { CoolerControlDeviceSettingsDTO } from '@/models/CCSettings.ts'
import { ErrorResponse } from '@/models/ErrorResponse.ts'

const props = defineProps<{
    deviceUID: UID
}>()
const emit = defineEmits<{
    (e: 'open', value: boolean): void
    (e: 'close'): void
}>()

const settingsStore = useSettingsStore()
const deviceStore = useDeviceStore()
const confirm = useConfirm()
const toast = useToast()
const { t } = useI18n()

const popRef = ref()

let isLiquidctl = false
let hasHwmonDriver = false
const useHwmon = ref(false)
let isAmdGpuWithOverdrive = false
const amdOverdriveEnabled = ref(false)
const amdOverdriveEnabling = ref(false)
let isThinkPad = false

for (const device of deviceStore.allDevices()) {
    if (device.uid === props.deviceUID && device.info != null) {
        isLiquidctl = device.type === DeviceType.LIQUIDCTL
        if (isLiquidctl) {
            hasHwmonDriver =
                device.info?.driver_info.locations.find((loc) => loc.includes('hwmon')) != null
        }
        if (device.info.amd_gpu_overdrive != null) {
            isAmdGpuWithOverdrive = true
            amdOverdriveEnabled.value = device.info.amd_gpu_overdrive
        }
        if (device.info.thinkpad_fan_control != null) {
            isThinkPad = true
        }
        break
    }
}
// extensions should always be present as well as the device's settings.
const deviceExtensions = settingsStore.ccDeviceSettings.get(props.deviceUID)?.extensions!
const directAccess = ref(deviceExtensions.direct_access)
const delayMillis = ref(deviceExtensions.delay_millis)

const toggleDirectAccess = _.debounce(() => {
    if (directAccess.value == deviceExtensions.direct_access) return // no change
    confirm.require({
        header: t('layout.settings.restartHeader'),
        message: t('layout.settings.applySettingAndRestart'),
        icon: 'pi pi-exclamation-triangle',
        defaultFocus: 'accept',
        acceptLabel: t('common.yes'),
        rejectLabel: t('common.no'),
        accept: async () => {
            const ccSetting: CoolerControlDeviceSettingsDTO = settingsStore.ccDeviceSettings.get(
                props.deviceUID,
            )!
            ccSetting.extensions.direct_access = directAccess.value
            const result = await deviceStore.daemonClient.saveCCDeviceSettings(
                ccSetting.uid,
                ccSetting,
            )
            // give the system a moment to make sure the setting has been saved ^
            await deviceStore.sleep(50)
            if (result === true) {
                toast.add({
                    severity: 'success',
                    summary: t('layout.settings.success'),
                    detail: t('layout.settings.successDetail'),
                    life: 6000,
                })
                await deviceStore.daemonClient.shutdownDaemon()
                await deviceStore.waitAndReload()
            } else {
                toast.add({
                    severity: 'error',
                    summary: t('common.error'),
                    detail: result.error || t('layout.settings.devices.unknownError'),
                    life: 6000,
                })
            }
        },
        reject: () => {
            // reset
            directAccess.value = deviceExtensions.direct_access
        },
    })
}, 1000)

const toggleUseHwmon = _.debounce(() => {
    if (!useHwmon.value) return // no change
    confirm.require({
        header: t('components.deviceExtensionSettings.disableDevice'),
        message: t('components.deviceExtensionSettings.disableInfo'),
        icon: 'pi pi-exclamation-triangle',
        defaultFocus: 'accept',
        acceptLabel: t('common.yes'),
        rejectLabel: t('common.no'),
        accept: async () => {
            const ccSetting: CoolerControlDeviceSettingsDTO = settingsStore.ccDeviceSettings.get(
                props.deviceUID,
            )!
            const deviceSettings = settingsStore.allUIDeviceSettings.get(props.deviceUID)
            // persist user-defined name if it exists (Helpful when blacklisting)
            ccSetting.name =
                deviceSettings?.name != null && deviceSettings.name
                    ? deviceSettings.name
                    : ccSetting.name
            ccSetting.disable = true
            const result = await deviceStore.daemonClient.saveCCDeviceSettings(
                ccSetting.uid,
                ccSetting,
            )
            // give the system a moment to make sure the setting has been saved ^
            await deviceStore.sleep(50)
            if (result === true) {
                toast.add({
                    severity: 'success',
                    summary: t('layout.settings.success'),
                    detail: t('layout.settings.successDetail'),
                    life: 6000,
                })
                await deviceStore.daemonClient.shutdownDaemon()
                await deviceStore.waitAndReload()
            } else {
                // Reset the toggle so the UI reflects the daemon's rejected state.
                useHwmon.value = !useHwmon.value
                toast.add({
                    severity: 'error',
                    summary: t('common.error'),
                    detail: result.error || t('layout.settings.devices.unknownError'),
                    life: 0,
                })
            }
        },
        reject: () => {
            // reset
            directAccess.value = deviceExtensions.direct_access
        },
    })
}, 1000)

const updateDelayMillis = _.debounce(() => {
    if (delayMillis.value == null || delayMillis.value == deviceExtensions.delay_millis) return
    confirm.require({
        header: t('layout.settings.restartHeader'),
        message: t('layout.settings.applySettingAndRestart'),
        icon: 'pi pi-exclamation-triangle',
        defaultFocus: 'accept',
        acceptLabel: t('common.yes'),
        rejectLabel: t('common.no'),
        accept: async () => {
            const ccSetting: CoolerControlDeviceSettingsDTO = settingsStore.ccDeviceSettings.get(
                props.deviceUID,
            )!
            ccSetting.extensions.delay_millis = delayMillis.value ?? 0
            const result = await deviceStore.daemonClient.saveCCDeviceSettings(
                ccSetting.uid,
                ccSetting,
            )
            await deviceStore.sleep(50)
            if (result === true) {
                toast.add({
                    severity: 'success',
                    summary: t('layout.settings.success'),
                    detail: t('layout.settings.successDetail'),
                    life: 6000,
                })
                await deviceStore.daemonClient.shutdownDaemon()
                await deviceStore.waitAndReload()
            } else {
                toast.add({
                    severity: 'error',
                    summary: t('common.error'),
                    detail: result.error || t('layout.settings.devices.unknownError'),
                    life: 6000,
                })
            }
        },
        reject: () => {
            delayMillis.value = deviceExtensions.delay_millis
        },
    })
}, 3000)

const enableAmdOverdrive = async () => {
    amdOverdriveEnabling.value = true
    const result = await deviceStore.daemonClient.amdGpuOverdriveEnable()
    amdOverdriveEnabling.value = false
    if (result instanceof ErrorResponse) {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: result.error ?? t('layout.settings.devices.unknownError'),
            life: 6000,
        })
    } else {
        toast.add({
            severity: 'success',
            summary: t('components.deviceExtensionSettings.overdriveSuccess'),
            detail: result,
            life: 10000,
        })
    }
}

const applyThinkPadFanControl = (value: boolean | string | number) => {
    settingsStore.applyThinkPadFanControl(Boolean(value))
}

const popoverOpen = (event: any): void => {
    popRef.value.toggle(event)
}

const popoverClose = (): void => {
    emit('open', false)
    emit('close')
}
</script>

<template>
    <Button
        class="w-full !justify-start !rounded-lg border-none text-text-color-secondary h-12 !p-4 !px-7 hover:text-text-color hover:bg-surface-hover outline-none"
        @click.stop.prevent="popoverOpen"
    >
        <svg-icon
            class="outline-0"
            type="mdi"
            :path="mdiCogs"
            :size="deviceStore.getREMSize(1.5)"
        />
        <span class="ml-1.5">
            {{ t('layout.menu.tooltips.deviceSettings') }}
        </span>
    </Button>
    <Popover ref="popRef" @show="emit('open', true)" @hide="popoverClose">
        <div
            class="ml-6 mt-2 w-full bg-bg-two border border-border-one p-2 rounded-lg text-text-color pb-4"
        >
            <table>
                <thead>
                    <tr>
                        <th colspan="6" class="pb-2.5 pt-1">
                            {{ t('components.deviceExtensionSettings.title') }}
                        </th>
                    </tr>
                </thead>
                <tbody>
                    <tr v-if="isLiquidctl">
                        <td class="w-64 text-end pl-4">
                            <div class="flex flex-row leading-none items-center">
                                <div
                                    v-tooltip.bottom="
                                        t('components.deviceExtensionSettings.directAccessDesc')
                                    "
                                >
                                    <svg-icon
                                        type="mdi"
                                        class="mr-2"
                                        :path="mdiInformationSlabCircleOutline"
                                        :size="deviceStore.getREMSize(1.25)"
                                    />
                                </div>
                                {{ t('components.deviceExtensionSettings.directAccess') }}
                            </div>
                        </td>
                        <td class="w-24 px-2 text-center">
                            <el-switch
                                v-model="directAccess"
                                size="large"
                                :disabled="!hasHwmonDriver"
                                @change="toggleDirectAccess"
                            />
                        </td>
                    </tr>
                    <tr v-if="isLiquidctl">
                        <td class="w-64 text-end pl-4">
                            <div class="flex flex-row leading-none items-center">
                                <div
                                    v-tooltip.bottom="
                                        t('components.deviceExtensionSettings.useHwmonDesc')
                                    "
                                >
                                    <svg-icon
                                        type="mdi"
                                        class="mr-2"
                                        :path="mdiInformationSlabCircleOutline"
                                        :size="deviceStore.getREMSize(1.25)"
                                    />
                                </div>
                                {{ t('components.deviceExtensionSettings.useHwmon') }}
                            </div>
                        </td>
                        <td class="w-24 px-2 text-center">
                            <el-switch
                                v-model="useHwmon"
                                size="large"
                                :disabled="!hasHwmonDriver"
                                @change="toggleUseHwmon"
                            />
                        </td>
                    </tr>
                    <tr v-if="isAmdGpuWithOverdrive" class="pb-2">
                        <td class="w-64 text-end pl-4 pb-2">
                            <div class="flex flex-row leading-none items-center">
                                <div
                                    v-tooltip.bottom="
                                        t('components.deviceExtensionSettings.overdriveDesc')
                                    "
                                >
                                    <svg-icon
                                        type="mdi"
                                        class="mr-2"
                                        :path="mdiInformationSlabCircleOutline"
                                        :size="deviceStore.getREMSize(1.25)"
                                    />
                                </div>
                                {{ t('components.deviceExtensionSettings.overdrive') }}
                            </div>
                        </td>
                        <td class="w-24 px-2 text-center pb-2">
                            <span v-if="amdOverdriveEnabled" class="text-green-400 font-semibold">
                                {{ t('components.deviceExtensionSettings.overdriveActive') }}
                            </span>
                            <Button
                                v-else
                                :label="t('components.deviceExtensionSettings.overdriveEnable')"
                                :loading="amdOverdriveEnabling"
                                class="border border-border-one bg-accent/80 hover:bg-accent text-text-color"
                                severity="warn"
                                size="small"
                                @click="enableAmdOverdrive"
                            />
                        </td>
                    </tr>
                    <tr v-if="isThinkPad">
                        <td class="w-64 text-end pl-4">
                            <div class="flex flex-row leading-none items-center">
                                <div
                                    v-tooltip.bottom="
                                        t(
                                            'components.deviceExtensionSettings.thinkPadFanControlDesc',
                                        )
                                    "
                                >
                                    <svg-icon
                                        type="mdi"
                                        class="mr-2"
                                        :path="mdiInformationSlabCircleOutline"
                                        :size="deviceStore.getREMSize(1.25)"
                                    />
                                </div>
                                {{ t('components.deviceExtensionSettings.thinkPadFanControl') }}
                            </div>
                        </td>
                        <td class="w-24 px-2 text-center">
                            <el-switch
                                v-model="settingsStore.thinkPadFanControlEnabled"
                                size="large"
                                @change="applyThinkPadFanControl"
                            />
                        </td>
                    </tr>
                    <tr v-if="isThinkPad">
                        <td class="w-64 text-end pl-4">
                            <div class="flex flex-row leading-none items-center">
                                <div
                                    v-tooltip.bottom="
                                        t(
                                            'components.deviceExtensionSettings.thinkPadFullSpeedDesc',
                                        )
                                    "
                                >
                                    <svg-icon
                                        type="mdi"
                                        class="mr-2"
                                        :path="mdiInformationSlabCircleOutline"
                                        :size="deviceStore.getREMSize(1.25)"
                                    />
                                </div>
                                {{ t('components.deviceExtensionSettings.thinkPadFullSpeed') }}
                            </div>
                        </td>
                        <td class="w-24 px-2 text-center">
                            <el-switch
                                v-model="settingsStore.ccSettings.thinkpad_full_speed"
                                size="large"
                            />
                        </td>
                    </tr>
                    <tr>
                        <td class="w-64 text-end pl-4">
                            <div class="flex flex-row leading-none items-center">
                                <div
                                    v-tooltip.bottom="
                                        t('components.deviceExtensionSettings.commandDelayDesc')
                                    "
                                >
                                    <svg-icon
                                        type="mdi"
                                        class="mr-2"
                                        :path="mdiInformationSlabCircleOutline"
                                        :size="deviceStore.getREMSize(1.25)"
                                    />
                                </div>
                                {{ t('components.deviceExtensionSettings.commandDelay') }}
                            </div>
                        </td>
                        <td class="w-24 px-2 text-center">
                            <InputNumber
                                v-model="delayMillis"
                                show-buttons
                                :min="0"
                                :max="250"
                                :step="10"
                                suffix=" ms"
                                button-layout="horizontal"
                                :input-style="{ width: '4.5rem' }"
                                @update:model-value="updateDelayMillis"
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
                </tbody>
            </table>
        </div>
    </Popover>
</template>

<style scoped lang="scss">
.el-switch {
    --el-switch-on-color: rgb(var(--colors-accent));
    --el-switch-off-color: rgb(var(--colors-bg-one));
    --el-color-white: rgb(var(--colors-bg-two));
}
</style>
