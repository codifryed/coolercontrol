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
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'radix-vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { computed, onMounted } from 'vue'
import { DaemonStatus, useDaemonState } from '@/stores/DaemonState.ts'
import { mdiCircle, mdiGit, mdiHelpCircleOutline, mdiToolboxOutline } from '@mdi/js'
import Button from 'primevue/button'
import { useI18n } from 'vue-i18n'

const appVersion = import.meta.env.PACKAGE_VERSION
const deviceStore = useDeviceStore()
const daemonState = useDaemonState()
const { t } = useI18n()

const healthCheck = await deviceStore.health()
const convertLogsToHtml = computed((): string => {
    return deviceStore.logs
        .replaceAll('\n', '<br/>')
        .replaceAll('INFO', '<span class="text-success">INFO</span>')
        .replaceAll('ERROR', '<span class="text-error">ERROR</span>')
        .replaceAll('WARN', '<span class="text-warning">WARN</span>')
        .replaceAll('DEBUG', '<span class="text-info">DEBUG</span>')
        .replaceAll('TRACE', '<span class="text-pink">TRACE</span>')
})
const badgeColor = computed((): string => {
    switch (daemonState.status) {
        case DaemonStatus.OK:
            return 'text-success'
        case DaemonStatus.WARN:
            return 'text-warning'
        case DaemonStatus.ERROR:
            return 'text-error'
        default:
            return 'text-error'
    }
})

const downloadLogFileName = 'coolercontrold-current.log'
const downloadLogHref = computed((): string => {
    const blob = new Blob([deviceStore.logs], { type: 'text/plain' })
    return URL.createObjectURL(blob)
})
const downloadLogDatasetURL = computed((): string => {
    // used for a.dataset.downloadurl
    return ['text/plain', downloadLogFileName, downloadLogHref].join(':')
})

onMounted(() => {
    const logOutput = document.getElementById('log-output')!
    // scroll to bottom:
    logOutput.scrollTop = logOutput.scrollHeight
})
</script>

<template>
    <div class="flex h-[3.5rem] border-b-4 border-border-one items-center justify-between">
        <div class="pl-4 py-2 text-2xl font-bold">{{ t('views.appInfo.title') }}</div>
    </div>
    <ScrollAreaRoot style="--scrollbar-size: 10px">
        <ScrollAreaViewport class="p-4 pb-16 h-screen w-full">
            <h3 class="mt-4 text-4xl font-sans subpixel-antialiased">
                CoolerControl
                <a href="https://gitlab.com/coolercontrol/coolercontrol/-/releases" target="_blank">
                    <span class="text-lg font-bold underline">v{{ appVersion }}</span>
                </a>
            </h3>
            <p class="text-sm italic">{{ t('views.appInfo.noWarranty') }}</p>
            <div class="mt-8">
                <div class="flex flex-row">
                    <div class="bg-bg-two border border-border-one p-4 rounded-lg text-text-color">
                        <table class="w-[26rem]">
                            <tbody>
                                <tr>
                                    <td
                                        class="mb-4 p-2 flex justify-end items-center font-semibold text-xl text-text-color"
                                    >
                                        {{ t('views.appInfo.daemonStatus') }}
                                    </td>
                                    <td class="pl-2">
                                        <Button
                                            :label="t('views.appInfo.acknowledgeIssues')"
                                            class="mb-4 bg-accent/80 hover:!bg-accent h-[2.375rem]"
                                            :disabled="daemonState.status === DaemonStatus.OK"
                                            @click="daemonState.acknowledgeLogIssues()"
                                        />
                                    </td>
                                </tr>
                                <tr>
                                    <td class="table-data font-bold text-lg text-end">
                                        {{ t('views.appInfo.status') }}
                                    </td>
                                    <td class="table-data">
                                        <div class="flex flex-row items-center">
                                            <svg-icon
                                                type="mdi"
                                                class="mr-2"
                                                :class="badgeColor"
                                                :path="mdiCircle"
                                                :size="deviceStore.getREMSize(1.25)"
                                            />
                                            {{
                                                t(
                                                    `daemon.status.${daemonState.status.replace(/\s+/g, '').toLowerCase()}`,
                                                )
                                            }}
                                        </div>
                                    </td>
                                </tr>
                                <tr>
                                    <td class="table-data font-bold text-lg text-end">
                                        {{ t('views.appInfo.processStatus') }}
                                    </td>
                                    <td class="table-data">{{ healthCheck.status }}</td>
                                </tr>
                                <tr>
                                    <td class="table-data font-bold text-lg text-end">
                                        {{ t('views.appInfo.host') }}
                                    </td>
                                    <td class="table-data w-44">
                                        {{ healthCheck.system.name }}
                                    </td>
                                </tr>
                                <tr>
                                    <td class="table-data font-bold text-lg text-end">
                                        {{ t('views.appInfo.uptime') }}
                                    </td>
                                    <td class="table-data w-44">
                                        {{ healthCheck.details.uptime }}
                                    </td>
                                </tr>
                                <tr>
                                    <td class="table-data font-bold text-lg text-end">
                                        {{ t('views.appInfo.version') }}
                                    </td>
                                    <td class="table-data">{{ healthCheck.details.version }}</td>
                                </tr>
                                <tr>
                                    <td class="table-data font-bold text-lg text-end">
                                        {{ t('views.appInfo.processId') }}
                                    </td>
                                    <td class="table-data">{{ healthCheck.details.pid }}</td>
                                </tr>
                                <tr>
                                    <td class="table-data font-bold text-lg text-end">
                                        {{ t('views.appInfo.memoryUsage') }}
                                    </td>
                                    <td class="table-data">
                                        {{ healthCheck.details.memory_mb }} MB
                                    </td>
                                </tr>
                                <tr>
                                    <td class="table-data font-bold text-lg text-end">
                                        {{ t('views.appInfo.liquidctl') }}
                                    </td>
                                    <td class="table-data">
                                        {{
                                            healthCheck.details.liquidctl_connected
                                                ? t('views.appInfo.connected')
                                                : t('views.appInfo.disconnected')
                                        }}
                                    </td>
                                </tr>
                            </tbody>
                        </table>
                    </div>
                    <div class="w-full" />
                </div>
            </div>
            <div class="mt-8">
                <div
                    class="bg-bg-two border border-border-one p-4 rounded-lg text-text-color w-[28rem]"
                >
                    <span class="mb-4 font-semibold text-xl text-text-color">{{
                        t('views.appInfo.helpfulLinks')
                    }}</span>
                    <p class="mt-4 text-wrap flex flex-row items-center">
                        <a
                            target="_blank"
                            href="https://docs.coolercontrol.org/getting-started.html#%F0%9F%A7%99-configure"
                            class="text-accent"
                        >
                            <div class="flex flex-row items-center">
                                <svg-icon
                                    type="mdi"
                                    class="mr-2"
                                    :path="mdiHelpCircleOutline"
                                    :size="deviceStore.getREMSize(2.0)"
                                />
                                {{ t('views.appInfo.gettingStarted') }}
                            </div> </a
                        >&nbsp;- {{ t('views.appInfo.helpSettingUp') }}
                    </p>
                    <p class="mt-4 text-wrap flex flex-row items-center">
                        <a
                            target="_blank"
                            href="https://docs.coolercontrol.org/hardware-support.html"
                            class="text-accent"
                        >
                            <div class="flex flex-row items-center">
                                <svg-icon
                                    type="mdi"
                                    class="mr-2"
                                    :path="mdiToolboxOutline"
                                    :size="deviceStore.getREMSize(2.0)"
                                />
                                {{ t('views.appInfo.hardwareSupport') }}
                            </div> </a
                        >&nbsp;- {{ t('views.appInfo.hardwareSupportDesc') }}
                    </p>
                    <p class="mt-4 text-wrap flex flex-row items-center">
                        <a
                            target="_blank"
                            href="https://gitlab.com/coolercontrol/coolercontrol/-/issues/new"
                            class="text-accent"
                        >
                            <div class="flex flex-row items-center">
                                <svg-icon
                                    type="mdi"
                                    class="mr-2"
                                    :path="mdiGit"
                                    :size="deviceStore.getREMSize(2.0)"
                                />
                                {{ t('views.appInfo.openIssue') }}
                            </div> </a
                        >&nbsp;- {{ t('views.appInfo.openIssueDesc') }}
                    </p>
                </div>
            </div>
            <div class="mt-8">
                <div
                    class="bg-bg-two border border-border-one p-4 rounded-lg text-text-color w-[60vw]"
                >
                    <div class="flex flex-row justify-between items-baseline">
                        <span class="mb-4 font-semibold text-xl text-text-color">{{
                            t('views.appInfo.logsAndDiagnostics')
                        }}</span>
                        <a
                            :href="downloadLogHref"
                            :download="downloadLogFileName"
                            :data-downloadurl="downloadLogDatasetURL"
                            class="text-accent outline-0 mb-2 text-sm"
                        >
                            {{ t('views.appInfo.downloadCurrentLog') }}
                        </a>
                    </div>
                    <div
                        class="h-[22rem] relative text-text-color-secondary bg-black/5 border border-border-one rounded-sm p-2 overflow-auto"
                    >
                        <pre
                            id="log-output"
                            class="whitespace-pre-wrap h-full w-full select-text outline-none"
                            v-html="convertLogsToHtml"
                        ></pre>
                    </div>
                </div>
            </div>
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
.table-data {
    padding: 0.5rem;
    border: 1px solid rgb(var(--colors-border-one));
}
</style>
