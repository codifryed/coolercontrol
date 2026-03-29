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
    mdiArrowCollapseVertical,
    mdiArrowExpandVertical,
    mdiCircle,
    mdiGit,
    mdiHelpCircleOutline,
    mdiToolboxOutline,
} from '@mdi/js'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'radix-vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { DaemonStatus, useDaemonState } from '@/stores/DaemonState.ts'
import Button from 'primevue/button'
import InputNumber from 'primevue/inputnumber'
import { $enum } from 'ts-enum-util'
import { useI18n } from 'vue-i18n'
import { useToast } from 'primevue/usetoast'

const appVersion = import.meta.env.PACKAGE_VERSION
const deviceStore = useDeviceStore()
const daemonState = useDaemonState()
const { t } = useI18n()

const healthCheck = await deviceStore.health()
const convertLogsToHtml = computed((): string => {
    return deviceStore.logs
        .replaceAll('&', '&amp;')
        .replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;')
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
const getDaemonStatusTranslationKey = (daemonStatus: DaemonStatus) =>
    $enum.visitValue(daemonStatus).with<string>({
        [DaemonStatus.OK]: () => {
            return 'ok'
        },
        [DaemonStatus.WARN]: () => {
            return 'hasWarnings'
        },
        [DaemonStatus.ERROR]: () => {
            return 'hasErrors'
        },
    })

const expandLogs = ref(false)
const onExpandClick = async () => {
    expandLogs.value = !expandLogs.value
    await nextTick()
    scrollToBottom()
}
const logContainer = ref<HTMLElement | null>(null)
const isUserScrolledUp = ref(false)

const checkIfScrolledToBottom = () => {
    if (!logContainer.value) return
    const { scrollTop, scrollHeight, clientHeight } = logContainer.value
    // Consider "at bottom" if within 5px of the bottom
    isUserScrolledUp.value = scrollHeight - scrollTop - clientHeight > 5
}

const scrollToBottom = () => {
    if (logContainer.value) {
        logContainer.value.scrollTop = logContainer.value.scrollHeight
    }
}

watch(
    () => deviceStore.logs,
    async () => {
        if (!isUserScrolledUp.value) {
            await nextTick()
            scrollToBottom()
        }
    },
)
const downloadLogFileName = 'coolercontrold-current.log'
const downloadLogHref = computed((): string => {
    const blob = new Blob([deviceStore.logs], { type: 'text/plain' })
    return URL.createObjectURL(blob)
})
const downloadLogDatasetURL = computed((): string => {
    // used for a.dataset.downloadurl
    return ['text/plain', downloadLogFileName, downloadLogHref].join(':')
})

onMounted(async () => {
    scrollToBottom()
    await pollStatus()
    if (cpuActive.value || gpuActive.value) {
        startPolling()
    }
})

// Stress Test
const toast = useToast()
const cpuThreads = ref<number | undefined>(undefined)
const cpuDuration = ref<number>(60)
const gpuDuration = ref<number>(60)
const cpuActive = ref(false)
const gpuActive = ref(false)
const cpuLoading = ref(false)
const gpuLoading = ref(false)
let statusPollInterval: ReturnType<typeof setInterval> | null = null

const pollStatus = async () => {
    const status = await deviceStore.daemonClient.stressTestStatus()
    cpuActive.value = status.cpu_active
    gpuActive.value = status.gpu_active
    if (!status.cpu_active && !status.gpu_active && statusPollInterval) {
        clearInterval(statusPollInterval)
        statusPollInterval = null
    }
}

const startPolling = () => {
    if (!statusPollInterval) {
        statusPollInterval = setInterval(pollStatus, 2000)
    }
}

const startCpuStress = async () => {
    cpuLoading.value = true
    const err = await deviceStore.daemonClient.startCpuStress(cpuThreads.value, cpuDuration.value)
    cpuLoading.value = false
    if (err) {
        toast.add({ severity: 'error', summary: 'CPU Stress', detail: err.error, life: 5000 })
    } else {
        cpuActive.value = true
        startPolling()
    }
}

const stopCpuStress = async () => {
    cpuLoading.value = true
    await deviceStore.daemonClient.stopCpuStress()
    cpuLoading.value = false
    cpuActive.value = false
}

const startGpuStress = async () => {
    gpuLoading.value = true
    const err = await deviceStore.daemonClient.startGpuStress(gpuDuration.value)
    gpuLoading.value = false
    if (err) {
        toast.add({ severity: 'error', summary: 'GPU Stress', detail: err.error, life: 5000 })
    } else {
        gpuActive.value = true
        startPolling()
    }
}

const stopGpuStress = async () => {
    gpuLoading.value = true
    await deviceStore.daemonClient.stopGpuStress()
    gpuLoading.value = false
    gpuActive.value = false
}

const stopAllStress = async () => {
    cpuLoading.value = true
    gpuLoading.value = true
    await deviceStore.daemonClient.stopAllStress()
    cpuLoading.value = false
    gpuLoading.value = false
    cpuActive.value = false
    gpuActive.value = false
}

onBeforeUnmount(() => {
    if (statusPollInterval) {
        clearInterval(statusPollInterval)
    }
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
            <div class="mt-8 grid gap-8 xl:grid-cols-[auto_auto] xl:w-fit">
                <div
                    class="bg-bg-two border border-border-one p-4 rounded-lg text-text-color w-[28rem]"
                >
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
                                            daemonState.connected
                                                ? t(
                                                      `daemon.status.${getDaemonStatusTranslationKey(daemonState.status)}`,
                                                  )
                                                : t('views.appInfo.disconnected')
                                        }}
                                    </div>
                                </td>
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
                                    {{ daemonState.connected ? healthCheck.details.uptime : '-' }}
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
                                <td class="table-data">{{ healthCheck.details.memory_mb }} MB</td>
                            </tr>
                            <tr>
                                <td class="table-data font-bold text-lg text-end">
                                    {{ t('views.appInfo.liquidctl') }}
                                </td>
                                <td class="table-data">
                                    {{
                                        daemonState.connected
                                            ? healthCheck.details.liquidctl_connected
                                                ? t('views.appInfo.connected')
                                                : t('views.appInfo.disconnected')
                                            : '-'
                                    }}
                                </td>
                            </tr>
                        </tbody>
                    </table>
                </div>
                <div
                    class="bg-bg-two border border-border-one p-4 rounded-lg text-text-color min-w-[28rem] w-max"
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
                        <a target="_blank" :href="healthCheck.links.repository" class="text-accent">
                            <div class="flex flex-row items-center">
                                <svg-icon
                                    type="mdi"
                                    class="mr-2"
                                    :path="mdiGit"
                                    :size="deviceStore.getREMSize(2.0)"
                                />
                                {{ t('views.appInfo.gitRepository') }}
                            </div> </a
                        >&nbsp;- {{ t('views.appInfo.gitRepositoryDesc') }}
                    </p>
                    <p class="mt-4 text-wrap flex flex-row items-center">
                        <a target="_blank" href="https://discord.gg/MbcgUFAfhV" class="text-accent">
                            <div class="flex flex-row items-center">
                                <span class="mr-2 pi pi-discord text-[2.0rem]" />
                                Discord
                            </div> </a
                        >&nbsp;- {{ t('views.appInfo.discordDesc') }}
                    </p>
                </div>
                <!-- Stress Test -->
                <div class="xl:col-span-2">
                    <div class="bg-bg-two border border-border-one p-4 rounded-lg text-text-color">
                        <div class="flex flex-row justify-between items-center mb-4">
                            <div class="flex flex-row items-center gap-2">
                                <span class="font-semibold text-xl text-text-color">{{
                                    t('views.appInfo.stressTest')
                                }}</span>
                                <svg-icon
                                    v-tooltip.right="t('views.appInfo.stressTestTooltip')"
                                    type="mdi"
                                    class="text-text-color-secondary cursor-help"
                                    :path="mdiHelpCircleOutline"
                                    :size="deviceStore.getREMSize(1.25)"
                                />
                            </div>
                            <Button
                                :label="t('views.appInfo.stopAll')"
                                class="bg-red-600/80 hover:!bg-red-600 h-[2.375rem]"
                                :disabled="!cpuActive && !gpuActive"
                                @click="stopAllStress"
                            />
                        </div>
                        <!-- CPU Stress -->
                        <div class="flex flex-row items-center gap-4 mb-3">
                            <span class="font-bold text-lg w-28">{{
                                t('views.appInfo.cpuStress')
                            }}</span>
                            <div class="flex items-center gap-1">
                                <label class="text-sm text-text-color-secondary">{{
                                    t('views.appInfo.threadCount')
                                }}</label>
                                <InputNumber
                                    v-model="cpuThreads"
                                    :min="1"
                                    :max="512"
                                    :placeholder="t('views.appInfo.allCores')"
                                    class="w-24"
                                    :disabled="cpuActive"
                                    input-class="!p-1.5 !text-sm w-full"
                                />
                            </div>
                            <div class="flex items-center gap-1">
                                <label class="text-sm text-text-color-secondary">{{
                                    t('views.appInfo.duration')
                                }}</label>
                                <InputNumber
                                    v-model="cpuDuration"
                                    :min="1"
                                    :max="600"
                                    class="w-20"
                                    :disabled="cpuActive"
                                    input-class="!p-1.5 !text-sm w-full"
                                />
                            </div>
                            <Button
                                v-if="!cpuActive"
                                :label="t('views.appInfo.start')"
                                class="bg-accent/80 hover:!bg-accent h-[2.375rem]"
                                :loading="cpuLoading"
                                @click="startCpuStress"
                            />
                            <Button
                                v-else
                                :label="t('views.appInfo.stop')"
                                class="bg-red-600/80 hover:!bg-red-600 h-[2.375rem]"
                                :loading="cpuLoading"
                                @click="stopCpuStress"
                            />
                            <svg-icon
                                type="mdi"
                                :class="cpuActive ? 'text-success' : 'text-text-color-secondary'"
                                :path="mdiCircle"
                                :size="deviceStore.getREMSize(0.75)"
                            />
                            <span class="text-sm">{{
                                cpuActive ? t('views.appInfo.active') : t('views.appInfo.inactive')
                            }}</span>
                        </div>
                        <!-- GPU Stress -->
                        <div class="flex flex-row items-center gap-4">
                            <span class="font-bold text-lg w-28">{{
                                t('views.appInfo.gpuStress')
                            }}</span>
                            <div class="flex items-center gap-1">
                                <label class="text-sm text-text-color-secondary">{{
                                    t('views.appInfo.duration')
                                }}</label>
                                <InputNumber
                                    v-model="gpuDuration"
                                    :min="1"
                                    :max="600"
                                    class="w-20"
                                    :disabled="gpuActive"
                                    input-class="!p-1.5 !text-sm w-full"
                                />
                            </div>
                            <Button
                                v-if="!gpuActive"
                                :label="t('views.appInfo.start')"
                                class="bg-accent/80 hover:!bg-accent h-[2.375rem]"
                                :loading="gpuLoading"
                                @click="startGpuStress"
                            />
                            <Button
                                v-else
                                :label="t('views.appInfo.stop')"
                                class="bg-red-600/80 hover:!bg-red-600 h-[2.375rem]"
                                :loading="gpuLoading"
                                @click="stopGpuStress"
                            />
                            <svg-icon
                                type="mdi"
                                :class="gpuActive ? 'text-success' : 'text-text-color-secondary'"
                                :path="mdiCircle"
                                :size="deviceStore.getREMSize(0.75)"
                            />
                            <span class="text-sm">{{
                                gpuActive ? t('views.appInfo.active') : t('views.appInfo.inactive')
                            }}</span>
                        </div>
                    </div>
                </div>
            </div>
            <div class="mt-8 mb-8">
                <div
                    class="flex flex-col bg-bg-two border border-border-one p-4 rounded-lg text-text-color min-w-[28rem] 2xl:w-[70vw]"
                >
                    <div class="flex flex-row justify-between items-baseline">
                        <div class="flex flex-row">
                            <span class="mb-4 font-semibold text-xl text-text-color">{{
                                t('views.appInfo.logsAndDiagnostics')
                            }}</span>
                            <Button
                                class="ml-4 !rounded-lg border-none w-8 h-8 !p-0 text-text-color-secondary hover:text-text-color hover:bg-surface-hover outline-none"
                                @click="onExpandClick"
                            >
                                <svg-icon
                                    type="mdi"
                                    :path="
                                        expandLogs
                                            ? mdiArrowCollapseVertical
                                            : mdiArrowExpandVertical
                                    "
                                    :size="deviceStore.getREMSize(1.5)"
                                />
                            </Button>
                        </div>
                        <a
                            :href="downloadLogHref"
                            :download="downloadLogFileName"
                            :data-downloadurl="downloadLogDatasetURL"
                            class="text-accent outline-0 mb-2"
                        >
                            {{ t('views.appInfo.downloadCurrentLog') }}
                        </a>
                    </div>
                    <div
                        ref="logContainer"
                        class="relative text-text-color-secondary bg-black/5 border border-border-one rounded-sm p-2 overflow-auto"
                        :class="expandLogs ? 'min-h-[32rem]' : 'h-[32rem]'"
                        @scroll="checkIfScrolledToBottom"
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
