<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiContentCopy } from '@mdi/js'
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { DaemonStatus, useDaemonState } from '@/stores/DaemonState.ts'
import { useToast } from '@/shell/toast'
import {
    daemonStartedAt,
    DEBUG_LOGGING_SETTING_ROUTE,
    journalCommand,
    LOGS_DOCS_URL,
} from '@/shell/debugLogging.ts'
import UiButton from '@/shell/ui/UiButton.vue'
import UiToggleGroup from '@/shell/ui/UiToggleGroup.vue'

const deviceStore = useDeviceStore()
const daemonState = useDaemonState()
const route = useRoute()
const toast = useToast()
const { t } = useI18n({ useScope: 'global' })

// Debug lines never reach this page, so while debug is on it says where they went.
const debugCommand = computed((): string => {
    const health = daemonState.healthCheck
    return journalCommand(daemonStartedAt(health.current_timestamp, health.details.uptime))
})
const copyDebugCommand = async (): Promise<void> => {
    try {
        await navigator.clipboard.writeText(debugCommand.value)
        toast.add({
            severity: 'success',
            summary: t('layout.shell.logsPage.commandCopied'),
            life: 1500,
        })
    } catch {
        // The clipboard can refuse (insecure context, permissions); the command
        // stays visible and selectable.
    }
}

// Level filter; ?level=warn|error pre-selects (the status row deep-links to
// warnings when the daemon status is degraded, and to everything when it is not).
type LogLevelFilter = 'all' | 'warn' | 'error'
const filterFromQuery = (): LogLevelFilter =>
    route.query.level === 'warn' || route.query.level === 'error'
        ? (route.query.level as LogLevelFilter)
        : 'all'
const levelFilter = ref<LogLevelFilter>(filterFromQuery())
// Both status targets share this route, so re-entering it only changes the
// query and the view is not re-created.
watch(
    () => route.query.level,
    () => (levelFilter.value = filterFromQuery()),
)
const levelOptions = computed(() => [
    { value: 'all', label: t('layout.shell.homePage.logsAll') },
    { value: 'warn', label: t('layout.shell.homePage.logsWarnings') },
    { value: 'error', label: t('layout.shell.homePage.logsErrors') },
])
const visibleLines = computed(() => {
    if (levelFilter.value === 'all') return deviceStore.logLines
    if (levelFilter.value === 'error') {
        return deviceStore.logLines.filter((line) => line.raw.includes('ERROR'))
    }
    return deviceStore.logLines.filter(
        (line) => line.raw.includes('ERROR') || line.raw.includes('WARN'),
    )
})

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
    () => deviceStore.logLines,
    async () => {
        if (!isUserScrolledUp.value) {
            await nextTick()
            scrollToBottom()
        }
    },
)

const downloadLogFileName = 'coolercontrold-current.log'
const downloadLogHref = computed((): string => {
    const raw = deviceStore.logLines.map((line) => line.raw).join('\n')
    const blob = new Blob([raw], { type: 'text/plain' })
    return URL.createObjectURL(blob)
})

onMounted(() => {
    scrollToBottom()
})
</script>

<template>
    <div class="flex h-full flex-col p-4">
        <div class="flex items-center justify-between pb-3">
            <h1 class="text-xl font-semibold text-text-color">
                {{ t('views.appInfo.logsAndDiagnostics') }}
            </h1>
            <span class="flex items-center gap-3">
                <UiToggleGroup v-model="levelFilter" :options="levelOptions" />
                <UiButton
                    variant="outline"
                    :disabled="daemonState.status === DaemonStatus.OK"
                    @click="daemonState.acknowledgeLogIssues()"
                >
                    {{ t('views.appInfo.acknowledgeIssues') }}
                </UiButton>
                <a :href="downloadLogHref" :download="downloadLogFileName">
                    <UiButton variant="outline">
                        {{ t('views.appInfo.downloadCurrentLog') }}
                    </UiButton>
                </a>
            </span>
        </div>
        <div
            v-if="daemonState.debugLogging"
            id="debug-logging-notice"
            class="mb-3 flex flex-col gap-2 rounded-lg border-l-4 border-warning bg-warning/10 p-3 text-sm text-text-color"
        >
            <span class="font-semibold">{{ t('layout.shell.logsPage.debugTitle') }}</span>
            <template v-if="daemonState.healthCheck.details.log_to_journal">
                <span class="text-text-color-secondary">
                    {{ t('layout.shell.logsPage.debugJournal') }}
                </span>
                <div
                    class="flex items-center gap-2 rounded border border-border-one bg-bg-two px-2 py-1"
                >
                    <code class="flex-1 select-all break-all font-mono text-sm">{{
                        debugCommand
                    }}</code>
                    <UiButton
                        variant="ghost"
                        size="icon"
                        :aria-label="t('layout.shell.logsPage.copyCommand')"
                        v-tooltip.top="t('layout.shell.logsPage.copyCommand')"
                        @click="copyDebugCommand"
                    >
                        <svg-icon type="mdi" :path="mdiContentCopy" :size="18" />
                    </UiButton>
                </div>
            </template>
            <span v-else class="text-text-color-secondary">
                {{ t('layout.shell.logsPage.debugNoJournal') }}
            </span>
            <span class="flex flex-wrap gap-x-4 gap-y-1">
                <a
                    :href="LOGS_DOCS_URL"
                    target="_blank"
                    class="text-accent underline-offset-2 hover:underline"
                >
                    {{ t('layout.shell.logsPage.debugDocs') }}
                </a>
                <RouterLink
                    :to="DEBUG_LOGGING_SETTING_ROUTE"
                    class="text-accent underline-offset-2 hover:underline"
                >
                    {{ t('layout.shell.logsPage.debugTurnOff') }}
                </RouterLink>
            </span>
        </div>
        <div
            ref="logContainer"
            class="min-h-0 flex-1 overflow-y-auto rounded-lg border border-border-one bg-bg-two p-3 font-mono text-sm text-text-color"
            @scroll="checkIfScrolledToBottom"
        >
            <!-- Lines are escaped and highlighted once on arrival (logLines.ts). -->
            <div
                v-for="(line, index) in visibleLines"
                :key="index"
                class="whitespace-pre-wrap break-all"
                v-html="line.html"
            />
            <div
                v-if="visibleLines.length === 0"
                class="py-8 text-center text-text-color-secondary"
            >
                {{ t('layout.shell.homePage.logsNoMatches') }}
            </div>
        </div>
    </div>
</template>
