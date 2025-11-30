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
import Popover from 'primevue/popover'
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useConfirm } from 'primevue/useconfirm'
import { useToast } from 'primevue/usetoast'
import { ElSwitch } from 'element-plus'
import _ from 'lodash'
import { CoolerControlDeviceSettingsDTO } from '@/models/CCSettings.ts'

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

let hasHwmonDriver = false
const useHwmon = ref(false)

for (const device of deviceStore.allDevices()) {
    if (
        device.uid === props.deviceUID &&
        device.type == DeviceType.LIQUIDCTL &&
        device.info != null
    ) {
        hasHwmonDriver =
            device.info?.driver_info.locations.find((loc) => loc.includes('hwmon')) != null
        break
    }
}
// extensions should always be present as well as the device's settings.
const deviceExtensions = settingsStore.ccDeviceSettings.get(props.deviceUID)?.extensions!
const directAccess = ref(deviceExtensions.direct_access)

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
            const successful = await deviceStore.daemonClient.saveCCDeviceSettings(
                ccSetting.uid,
                ccSetting,
            )
            // give the system a moment to make sure the setting has been saved ^
            await deviceStore.sleep(50)
            if (successful) {
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
                    detail: t('layout.settings.devices.unknownError'),
                    life: 4000,
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
            const successful = await deviceStore.daemonClient.saveCCDeviceSettings(
                ccSetting.uid,
                ccSetting,
            )
            // give the system a moment to make sure the setting has been saved ^
            await deviceStore.sleep(50)
            if (successful) {
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
                    detail: t('layout.settings.devices.unknownError'),
                    life: 4000,
                })
            }
        },
        reject: () => {
            // reset
            directAccess.value = deviceExtensions.direct_access
        },
    })
}, 1000)

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
                    <tr>
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
                    <tr>
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
