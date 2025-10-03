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
import { ElSwitch } from 'element-plus'
import 'element-plus/es/components/switch/style/css'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { mdiAlertOutline, mdiCogs, mdiInformationSlabCircleOutline } from '@mdi/js'
import { PopoverContent, PopoverRoot, PopoverTrigger } from 'radix-vue'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { computed, nextTick, ref, Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { UID } from '@/models/Device.ts'
import { ChannelExtensionNames } from '@/models/SpeedOptions.ts'
import { Profile, ProfileType } from '@/models/Profile.ts'
import { CCChannelSettings, ChannelExtensions } from '@/models/CCSettings.ts'
import { useToast } from 'primevue/usetoast'

const props = defineProps<{
    deviceUID: UID
    channelName: string
    chosenProfile?: Profile
}>()
const emit = defineEmits<{
    (e: 'change'): void
}>()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const toast = useToast()
const { t } = useI18n()
const isPopupOpen = ref(false)
const hwFanCurve: Ref<boolean> = ref(false)
const currentChannelExtension: Ref<ChannelExtensionNames | undefined> = ref()

for (const device of deviceStore.allDevices()) {
    if (device.uid === props.deviceUID && device.info != null) {
        const channelInfo = device.info.channels.get(props.channelName)
        if (channelInfo?.speed_options?.extension != null) {
            const channelExtensionSettings = settingsStore.ccDeviceSettings
                .get(props.deviceUID)
                ?.channel_settings.get(props.channelName)?.extension
            switch (channelInfo.speed_options.extension) {
                case ChannelExtensionNames.AmdRdnaGpu:
                    currentChannelExtension.value = ChannelExtensionNames.AmdRdnaGpu
                    if (channelExtensionSettings?.hw_fan_curve_enabled != null) {
                        hwFanCurve.value = channelExtensionSettings.hw_fan_curve_enabled
                    }
                    break
                case ChannelExtensionNames.AutoHWCurve:
                    currentChannelExtension.value = ChannelExtensionNames.AutoHWCurve
                    if (channelExtensionSettings?.auto_hw_curve_enabled != null) {
                        hwFanCurve.value = channelExtensionSettings.auto_hw_curve_enabled
                    }
                    break
                default:
                    break
            }
        }
    }
}
const hwFanCurveIsApplicable = computed((): boolean => {
    const p = props.chosenProfile
    const isApplicable =
        !!p &&
        p.p_type === ProfileType.Graph &&
        p.temp_source?.device_uid === props.deviceUID && // if it's an AMDGPU, that it uses the correct temp sensor:
        (currentChannelExtension.value !== ChannelExtensionNames.AmdRdnaGpu ||
            p.temp_source?.temp_name === 'temp1')
    if (!isApplicable) {
        nextTick(() => {
            hwFanCurve.value = false
        })
    }
    return isApplicable
})

const saveChannelExtensionSettings = () => {
    if (currentChannelExtension.value == null || !hwFanCurveIsApplicable.value) return
    let newExtensionSettings: ChannelExtensions | undefined = undefined
    if (currentChannelExtension.value === ChannelExtensionNames.AmdRdnaGpu && hwFanCurve.value) {
        newExtensionSettings = {
            hw_fan_curve_enabled: true,
        }
    } else if (
        currentChannelExtension.value === ChannelExtensionNames.AutoHWCurve &&
        hwFanCurve.value
    ) {
        newExtensionSettings = {
            auto_hw_curve_enabled: true,
        }
    }
    const deviceSettings = settingsStore.ccDeviceSettings.get(props.deviceUID)!
    if (newExtensionSettings === undefined) {
        const ccChannelSettings = deviceSettings.channel_settings.get(props.channelName)
        if (ccChannelSettings == null) {
            // no settings exist, no need to remove the extension settings
            return
        }
        const changed = ccChannelSettings.extension != null
        ccChannelSettings.extension = undefined
        if (changed) {
            const successful = deviceStore.daemonClient.saveCCDeviceSettings(
                props.deviceUID,
                deviceSettings,
            )
            if (!successful) {
                toast.add({
                    severity: 'error',
                    summary: t('common.error'),
                    detail: t('components.channelExtensionSettings.saveError'),
                    life: 3000,
                })
            }
        }
        return
    }
    // get or create new settings to apply
    let ccChannelSettings =
        deviceSettings.channel_settings.get(props.channelName) ?? new CCChannelSettings()
    ccChannelSettings.extension = newExtensionSettings
    deviceSettings.channel_settings.set(props.channelName, ccChannelSettings)
    const successful = deviceStore.daemonClient.saveCCDeviceSettings(
        props.deviceUID,
        deviceSettings,
    )
    if (!successful) {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: t('components.channelExtensionSettings.saveError'),
            life: 3000,
        })
    }
}
defineExpose({
    saveChannelExtensionSettings,
})
</script>

<template>
    <div
        v-tooltip.bottom="{
            value: t('components.channelExtensionSettings.title'),
            disabled: isPopupOpen,
        }"
    >
        <popover-root @update:open="(open) => (isPopupOpen = open)">
            <popover-trigger
                class="h-[2.375rem] rounded-lg border-2 border-border-one !py-1.5 !px-2.5 text-text-color outline-0 text-center justify-center items-center flex !m-0 hover:bg-surface-hover"
            >
                <svg-icon
                    class="outline-0 mt-[-2px]"
                    type="mdi"
                    :path="mdiCogs"
                    :size="deviceStore.getREMSize(1.35)"
                />
            </popover-trigger>
            <popover-content side="bottom" class="z-10">
                <div
                    class="w-full bg-bg-two border border-border-one p-2 rounded-lg text-text-color pb-4"
                >
                    <table>
                        <thead>
                            <tr>
                                <th colspan="6" class="pb-2.5 pt-1">
                                    {{ t('components.channelExtensionSettings.title') }}
                                </th>
                            </tr>
                        </thead>
                        <tbody>
                            <tr>
                                <td class="w-24 text-end pl-4">
                                    <div class="flex flex-row leading-none items-center">
                                        <div
                                            v-tooltip.bottom="
                                                t(
                                                    'components.channelExtensionSettings.firmwareControlledProfileDesc',
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
                                        {{
                                            t(
                                                'components.channelExtensionSettings.firmwareControlledProfile',
                                            )
                                        }}
                                        <div
                                            class="ml-2 w-2"
                                            v-tooltip.bottom="
                                                t(
                                                    'components.channelExtensionSettings.firmwareControlDisabled',
                                                )
                                            "
                                        >
                                            <svg-icon
                                                v-if="!hwFanCurveIsApplicable"
                                                type="mdi"
                                                :path="mdiAlertOutline"
                                                :size="deviceStore.getREMSize(1.25)"
                                                style="color: rgb(var(--colors-red))"
                                            />
                                        </div>
                                    </div>
                                </td>
                                <td class="w-24 px-2 text-center">
                                    <el-switch
                                        v-model="hwFanCurve"
                                        size="large"
                                        :disabled="!hwFanCurveIsApplicable"
                                        @change="emit('change')"
                                    />
                                </td>
                            </tr>
                        </tbody>
                    </table>
                </div>
            </popover-content>
        </popover-root>
    </div>
</template>

<style scoped lang="scss">
.el-switch {
    --el-switch-on-color: rgb(var(--colors-accent));
    --el-switch-off-color: rgb(var(--colors-bg-one));
    --el-color-white: rgb(var(--colors-bg-two));
}
</style>
