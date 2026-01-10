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

<script setup lang="ts" xmlns="http://www.w3.org/1999/html">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiCogs, mdiPowerPlugOutline } from '@mdi/js'
import { Ref, ref } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import Card from 'primevue/card'
import { ServiceType } from '@/models/Plugins.ts'
import Button from 'primevue/button'
import { useDialog } from 'primevue/usedialog'
import pluginUi from '@/layout/PluginUi.vue'
import { useI18n } from 'vue-i18n'

const deviceStore = useDeviceStore()
const dialog = useDialog()
const { t } = useI18n()

const pluginsDto = await deviceStore.daemonClient.loadPlugins()

const pluginUiAvailable: Ref<Map<string, boolean>> = ref<Map<string, boolean>>(new Map())
await Promise.all(
    pluginsDto.plugins.map(async (plugin) => {
        pluginUiAvailable.value.set(
            plugin.id,
            await deviceStore.daemonClient.hasPluginUi(plugin.id),
        )
    }),
)

const getSubTitle = (serviceType: ServiceType): string => {
    switch (serviceType) {
        case ServiceType.Device:
            return t('layout.settings.plugins.device')
        case ServiceType.Integration:
            return t('layout.settings.plugins.integration')
        default:
            return 'Unknown Plugin Type'
    }
}

const getPluginVersion = (version?: string): string => {
    if (version) {
        return `v${version}`
    }
    return ''
}

// onClick of settings, open up model with iframe to plugin UI
const openPluginUi = (pluginId: string): void => {
    dialog.open(pluginUi, {
        props: {
            header: `${pluginId} ${t('layout.topbar.settings')}`,
            position: 'center',
            modal: true,
            dismissableMask: true,
        },
        data: {
            pluginId: pluginId,
        },
    })
}
</script>

<template>
    <div class="flex flex-wrap">
        <Card v-for="plugin in pluginsDto.plugins" :key="plugin.id" class="mb-4 mr-4 w-[36rem]">
            <template #title>
                <div class="flex items-center gap-2">
                    <svg-icon
                        type="mdi"
                        :path="mdiPowerPlugOutline"
                        :size="deviceStore.getREMSize(1.5)"
                    />
                    {{ plugin.id }}
                </div>
            </template>
            <template #subtitle
                >{{ getSubTitle(plugin.service_type) }}
                <span v-if="plugin.version">
                    <br />
                    {{ getPluginVersion(plugin.version) }}
                </span>
                <span v-if="plugin.url">
                    <br />
                    <a
                        class="outline-0 underline"
                        :href="plugin.url"
                        target="_blank"
                        rel="noopener noreferrer"
                        >{{ t('layout.settings.plugins.pluginUrl') }}</a
                    >
                </span>
            </template>
            <template #content>
                <p v-if="plugin.description" class="text-wrap italic mb-4">
                    {{ plugin.description }}
                </p>
                <p class="text-wrap">
                    <span class="leading-loose" :class="{ 'font-bold': plugin.privileged }">
                        {{
                            plugin.privileged
                                ? t('layout.settings.plugins.privileged')
                                : t('layout.settings.plugins.restricted')
                        }}
                        <br />
                    </span>
                    <span v-if="plugin.address">
                        <code>{{ plugin.address }}</code>
                    </span>
                </p>
            </template>
            <template #footer v-if="pluginUiAvailable.get(plugin.id)">
                <Button
                    class="bg-accent/80 hover:!bg-accent w-40 h-[2.375rem]"
                    @click="openPluginUi(plugin.id)"
                >
                    <svg-icon
                        class="outline-0"
                        type="mdi"
                        :path="mdiCogs"
                        :size="deviceStore.getREMSize(1.625)"
                    />
                </Button>
            </template>
        </Card>
    </div>
</template>

<style scoped lang="scss"></style>
