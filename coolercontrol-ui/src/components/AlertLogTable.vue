<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import {
    mdiChevronDown,
    mdiChevronLeft,
    mdiChevronRight,
    mdiChevronUp,
    mdiPageFirst,
    mdiPageLast,
} from '@mdi/js'
import { AlertLog, getAlertStateClass, getAlertStateDisplayName } from '@/models/Alert.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import UiButton from '@/shell/ui/UiButton.vue'
import UiInput from '@/shell/ui/UiInput.vue'
import UiSelect from '@/shell/ui/UiSelect.vue'
import UiTable from '@/shell/ui/UiTable.vue'

// Alert log listing: newest first by default, searchable, paged. With an
// alertUID it shows only that alert's logs; without one it shows all logs
// and rows navigate to their alert.
const props = defineProps<{ alertUID?: string }>()

const settingsStore = useSettingsStore()
const router = useRouter()
const { t } = useI18n()

const singleAlert = computed(() => props.alertUID != null)
const openAlert = (alertUID: string) => {
    if (singleAlert.value) return
    router.push({ name: 'monitoring-alert', params: { alertUID } })
}

// Announced rows carry the live state colour; quiet per-source rows stay muted.
const logStateClass = (log: AlertLog): string =>
    log.quiet ? 'text-text-color-secondary' : getAlertStateClass(log.state)

const logSearch = ref('')
const logSortAsc = ref(false)
const logPage = ref(1)
const logRows = ref('20')
const logRowsOptions = ['20', '50', '100'].map((n) => ({ label: n, value: n }))
const filteredLogs = computed(() => {
    const needle = logSearch.value.trim().toLowerCase()
    let logs = props.alertUID
        ? settingsStore.alertLogs.filter((log) => log.uid === props.alertUID)
        : [...settingsStore.alertLogs]
    if (needle) {
        logs = logs.filter(
            (log) =>
                log.name.toLowerCase().includes(needle) ||
                log.message.toLowerCase().includes(needle),
        )
    }
    logs.sort((a, b) =>
        logSortAsc.value
            ? new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime()
            : new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime(),
    )
    return logs
})
const logPageCount = computed(() =>
    Math.max(1, Math.ceil(filteredLogs.value.length / Number(logRows.value))),
)
const pagedLogs = computed(() => {
    const rows = Number(logRows.value)
    const page = Math.min(logPage.value, logPageCount.value)
    return filteredLogs.value.slice((page - 1) * rows, page * rows)
})
watch([logSearch, logRows], () => {
    logPage.value = 1
})

// The message column takes all spare width, which alone would wrap a name at every word.
// A name keeps to one line until it reaches this share of the text width the two columns
// have together. Qt's Chrome 90 has no container queries, so the widths are measured.
const NAME_WIDTH_SHARE = 0.4
const table = ref<InstanceType<typeof UiTable>>()
const timestampHead = ref<HTMLElement>()
const stateHead = ref<HTMLElement>()
const nameWidthMax = ref('none')
let resizeObserver: ResizeObserver | null = null
onMounted(() => {
    const frame: HTMLElement | undefined = table.value?.$el
    const timestamp = timestampHead.value
    const state = stateHead.value
    if (singleAlert.value || frame == null || timestamp == null || state == null) return
    resizeObserver = new ResizeObserver(() => {
        const style = getComputedStyle(state)
        const cellPadding = parseFloat(style.paddingLeft) + parseFloat(style.paddingRight)
        const textWidth =
            frame.clientWidth - timestamp.offsetWidth - state.offsetWidth - 2 * cellPadding
        nameWidthMax.value = `${Math.max(0, Math.floor(textWidth * NAME_WIDTH_SHARE))}px`
    })
    for (const element of [frame, timestamp, state]) resizeObserver.observe(element)
})
onBeforeUnmount(() => resizeObserver?.disconnect())
</script>

<template>
    <UiTable ref="table" bordered class="w-full" :style="{ '--name-width-max': nameWidthMax }">
        <template #toolbar>
            <div
                class="flex flex-wrap items-center justify-between gap-2 border-b border-border-one bg-bg-two p-2"
            >
                <UiInput v-model="logSearch" :placeholder="t('common.search')" />
                <div class="flex items-center gap-1">
                    <UiButton
                        variant="ghost"
                        size="icon"
                        :disabled="logPage <= 1"
                        @click="logPage = 1"
                    >
                        <svg-icon type="mdi" :path="mdiPageFirst" :size="18" />
                    </UiButton>
                    <UiButton
                        variant="ghost"
                        size="icon"
                        :disabled="logPage <= 1"
                        @click="logPage -= 1"
                    >
                        <svg-icon type="mdi" :path="mdiChevronLeft" :size="18" />
                    </UiButton>
                    <span class="px-2 text-text-color-secondary">
                        {{ Math.min(logPage, logPageCount) }} / {{ logPageCount }}
                    </span>
                    <UiButton
                        variant="ghost"
                        size="icon"
                        :disabled="logPage >= logPageCount"
                        @click="logPage += 1"
                    >
                        <svg-icon type="mdi" :path="mdiChevronRight" :size="18" />
                    </UiButton>
                    <UiButton
                        variant="ghost"
                        size="icon"
                        :disabled="logPage >= logPageCount"
                        @click="logPage = logPageCount"
                    >
                        <svg-icon type="mdi" :path="mdiPageLast" :size="18" />
                    </UiButton>
                    <UiSelect v-model="logRows" :options="logRowsOptions" class="!min-w-24" />
                </div>
            </div>
        </template>
        <template #head>
            <tr>
                <th
                    ref="timestampHead"
                    class="cursor-pointer select-none hover:bg-surface-hover"
                    @click="logSortAsc = !logSortAsc"
                >
                    <span class="inline-flex items-center gap-1">
                        {{ t('common.timestamp') }}
                        <svg-icon
                            type="mdi"
                            :path="logSortAsc ? mdiChevronUp : mdiChevronDown"
                            :size="16"
                        />
                    </span>
                </th>
                <th ref="stateHead">{{ t('common.state') }}</th>
                <th v-if="!singleAlert">{{ t('common.name') }}</th>
                <th class="w-full">{{ t('common.message') }}</th>
            </tr>
        </template>
        <tr
            v-for="(log, index) in pagedLogs"
            :key="`${log.uid}-${log.timestamp}-${index}`"
            :class="{ 'cursor-pointer hover:bg-surface-hover': !singleAlert }"
            @click="openAlert(log.uid)"
        >
            <td class="whitespace-nowrap">
                {{ new Date(log.timestamp).toLocaleString() }}
            </td>
            <td>
                <span class="underline" :class="logStateClass(log)">
                    {{ getAlertStateDisplayName(log.state) }}
                </span>
            </td>
            <td v-if="!singleAlert">
                <!-- The longest word is the floor, as it is for plain cell text. -->
                <span
                    class="block w-max min-w-min max-w-[var(--name-width-max)] text-balance underline"
                >
                    {{ log.name }}
                </span>
            </td>
            <td class="w-full text-ellipsis">{{ log.message }}</td>
        </tr>
    </UiTable>
</template>
