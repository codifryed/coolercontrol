<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { UID } from '@/models/Device.ts'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { coolingChannels } from '@/shell/cooling/channels.ts'
import { hardwareNoticeKind } from '@/shell/hardware/findings.ts'
import { useToolWizards } from '@/composables/useToolWizards.ts'
import { features } from '@/features'
import ChannelCard from '@/shell/cooling/ChannelCard.vue'
import HardwareNotice from '@/shell/hardware/HardwareNotice.vue'
import HardwareHelpLine from '@/shell/hardware/HardwareHelpLine.vue'
import UiButton from '@/shell/ui/UiButton.vue'

const { t } = useI18n()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()

const groups = computed(() => coolingChannels(deviceStore.allDevices(), settingsStore.channelUnit))

const channels = computed(() => groups.value.flatMap((group) => group.channels))

// Undefined whenever at least one channel can be driven: a working machine has
// nothing to be told here.
const noticeKind = computed(() =>
    hardwareNoticeKind(
        settingsStore.healthSystemFindings,
        channels.value.length,
        channels.value.filter((channel) => channel.controllable).length,
    ),
)

const title = computed(() => t('layout.shell.cooling'))

const deviceLabel = (deviceUID: UID): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.name ?? deviceUID

const deviceColor = (deviceUID: UID): string =>
    settingsStore.allUIDeviceSettings.get(deviceUID)?.userColor ?? ''

const { openCalibrationWizard, openGenerateWizard } = useToolWizards()
</script>

<template>
    <div class="flex h-full flex-col overflow-y-auto">
        <div class="flex flex-wrap items-center gap-3 px-4 pt-4">
            <h1 class="text-xl font-semibold text-text-color">{{ title }}</h1>
            <span class="text-base text-text-color-secondary">
                {{ t('layout.shell.coolingPage.landingHint') }}
            </span>
            <span class="ml-auto flex items-center gap-2">
                <UiButton
                    v-if="features.coolingWizard"
                    variant="outline"
                    @click="openGenerateWizard()"
                >
                    {{ t('views.appInfo.gettingStartedAutoCreateLink') }}
                </UiButton>
                <UiButton variant="outline" @click="openCalibrationWizard()">
                    {{ t('components.wizards.calibration.title') }}
                </UiButton>
            </span>
        </div>
        <template v-if="groups.length > 0">
            <div v-if="noticeKind" class="px-4 pt-4">
                <HardwareNotice :kind="noticeKind" />
            </div>
            <section v-for="group in groups" :key="group.deviceUID" class="px-4 pt-4">
                <h2
                    class="truncate pb-2 text-sm font-medium uppercase tracking-wide"
                    :class="{ 'text-text-color-secondary': !deviceColor(group.deviceUID) }"
                    :style="
                        deviceColor(group.deviceUID) ? { color: deviceColor(group.deviceUID) } : {}
                    "
                >
                    {{ deviceLabel(group.deviceUID) }}
                </h2>
                <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
                    <ChannelCard
                        v-for="channel in group.channels"
                        :key="`${channel.deviceUID}-${channel.channelName}`"
                        :channel="channel"
                    />
                </div>
            </section>
            <!-- Only when the page lists something. The empty state below is a
                 HardwareNotice, which already carries both of these links. -->
            <div class="px-4 pt-6">
                <HardwareHelpLine />
            </div>
            <div class="pb-6" />
        </template>
        <div v-else-if="noticeKind" class="flex flex-1 items-center justify-center p-4">
            <HardwareNotice :kind="noticeKind" class="w-full max-w-2xl" />
        </div>
    </div>
</template>
