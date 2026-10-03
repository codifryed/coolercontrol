<!--
  SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<!-- The search palette. ListboxRoot rather than a Combobox popper: the list is
     always open inside the dialog, and Listbox gives arrow/Home/End/Enter
     navigation across groups without any positioning machinery.

     The index is built when the palette opens, never as a computed. Device
     status ticks every poll interval and a reactive index would rebuild on each
     one, for a list nobody is looking at. -->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiChevronDown, mdiMagnify } from '@mdi/js'
import {
    DialogContent,
    DialogDescription,
    DialogOverlay,
    DialogPortal,
    DialogRoot,
    DialogTitle,
    ListboxContent,
    ListboxFilter,
    ListboxGroup,
    ListboxGroupLabel,
    ListboxItem,
    ListboxRoot,
} from 'reka-ui'
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import i18n from '@/i18n'
import type { UID } from '@/models/Device.ts'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { buildIndex } from '@/shell/search/index.ts'
import { groupResults, searchEntries } from '@/shell/search/match.ts'
import { openedByKeyboard, paletteOpen, shouldRestoreFocus } from '@/shell/search/palette.ts'
import { PAGE_ENTRIES, PAGE_GROUPS } from '@/shell/search/pages.ts'
import { recentIds, rememberRecent } from '@/shell/search/recents.ts'
import { useSearchActions } from '@/shell/search/useSearchActions.ts'
import {
    KIND_LABEL_KEYS,
    type SearchEntry,
    type SearchGroup,
    type SearchKind,
} from '@/shell/search/types.ts'

const { t } = useI18n()
const router = useRouter()
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const { runAction } = useSearchActions()

const query = ref('')
const entries = ref<SearchEntry[]>([])
const expanded = ref(new Set<SearchKind>())

// English alongside the active locale: the docs, the forums and lm-sensors are
// all english, so a localised UI still has to answer to the english term.
const tEn = (key: string): string => i18n.global.t(key, 'en')

const rebuild = (): void => {
    entries.value = buildIndex({
        devices: [...deviceStore.allDevices()],
        deviceLabel: (uid: UID) => settingsStore.allUIDeviceSettings.get(uid)?.name ?? uid,
        channelLabel: (uid: UID, channelName: string) =>
            settingsStore.allUIDeviceSettings.get(uid)?.sensorsAndChannels.get(channelName)?.name ??
            channelName,
        channelUnit: settingsStore.channelUnit,
        profiles: settingsStore.profiles,
        functions: settingsStore.functions,
        modes: settingsStore.modes,
        dashboards: settingsStore.dashboards,
        alerts: settingsStore.alerts,
        pluginIds: deviceStore.plugins.map((plugin) => plugin.id),
        isQtApp: deviceStore.isQtApp(),
        t,
        tEn,
    })
}

// Set when the palette closes because a result was taken, so the close handler
// can tell that apart from a dismissal.
const activated = ref(false)

watch(paletteOpen, (open) => {
    if (!open) return
    query.value = ''
    expanded.value = new Set()
    activated.value = false
    rebuild()
})

const onCloseAutoFocus = (event: Event): void => {
    if (!shouldRestoreFocus(activated.value, openedByKeyboard.value)) event.preventDefault()
}

const byId = computed(() => new Map(entries.value.map((entry) => [entry.id, entry])))

const groups = computed((): SearchGroup[] =>
    groupResults(searchEntries(entries.value, query.value)),
)

// Empty query: what you opened last, else the whole map of the app. The map is
// the point on first run, when there is nothing to recall and the user opened
// search precisely because they could not find something.
const recents = computed((): SearchEntry[] =>
    recentIds()
        .map((id) => byId.value.get(id))
        .filter((entry): entry is SearchEntry => entry != null),
)

// Matched by id rather than by rendered breadcrumb: two sections could
// translate to the same word in some locale, and the column a page belongs to
// is not a translation question.
const browseMap = computed(() =>
    PAGE_GROUPS.map((groupKey) => {
        const ids = new Set(
            PAGE_ENTRIES.filter((page) => page.groupKey === groupKey).map((page) => page.id),
        )
        return { groupKey, entries: entries.value.filter((entry) => ids.has(entry.id)) }
    }).filter((column) => column.entries.length > 0),
)

const showBrowse = computed(() => query.value.trim().length === 0 && recents.value.length === 0)
const showRecents = computed(() => query.value.trim().length === 0 && recents.value.length > 0)

const visible = (group: SearchGroup): SearchEntry[] =>
    expanded.value.has(group.kind) ? [...group.entries, ...group.hidden] : group.entries

const activate = (entry: SearchEntry): void => {
    rememberRecent(entry.id)
    activated.value = true
    paletteOpen.value = false
    if ('action' in entry.target) runAction(entry.target.action)
    else void router.push(entry.target.route)
}

// The "N more" row is a Listbox item so arrow keys still reach it; selecting it
// reveals the rest of its group instead of navigating.
const onSelect = (value: string): void => {
    if (value.startsWith('more:')) {
        const kind = value.slice('more:'.length) as SearchKind
        expanded.value = new Set([...expanded.value, kind])
        return
    }
    const entry = byId.value.get(value)
    if (entry != null) activate(entry)
}

const itemClass =
    'flex cursor-pointer flex-col gap-0.5 rounded-lg px-3 py-2 outline-none ' +
    'data-[highlighted]:bg-surface-hover'
const labelClass =
    'px-3 pb-1 pt-3 text-xs font-semibold uppercase tracking-wide ' + 'text-text-color-secondary'
</script>

<template>
    <DialogRoot v-model:open="paletteOpen">
        <DialogPortal>
            <!-- Opacity only, no backdrop-filter: a blur flashed and left
                 artifacts under QtWebEngine on NVIDIA at low GPU power. -->
            <DialogOverlay class="fixed inset-0 z-[1200] bg-black/80" />
            <DialogContent
                class="fixed left-1/2 top-1/2 z-[1210] flex h-[85vh] w-[95vw] max-w-2xl -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-lg border border-border-one bg-bg-two text-text-color shadow-xl outline-none md:h-auto md:max-h-[70vh] md:top-[15%] md:-translate-y-0"
                @close-auto-focus="onCloseAutoFocus"
            >
                <DialogTitle class="sr-only">{{ t('common.search') }}</DialogTitle>
                <DialogDescription class="sr-only">
                    {{ t('layout.shell.search.hint') }}
                </DialogDescription>
                <ListboxRoot
                    highlight-on-hover
                    class="flex min-h-0 flex-1 flex-col"
                    @update:model-value="onSelect(($event as string) ?? '')"
                >
                    <div
                        class="flex shrink-0 items-center gap-2 border-b border-border-one px-4 py-3"
                    >
                        <svg-icon
                            type="mdi"
                            :path="mdiMagnify"
                            :size="deviceStore.getREMSize(1.25)"
                            class="shrink-0 text-text-color-secondary"
                        />
                        <ListboxFilter
                            v-model="query"
                            auto-focus
                            :placeholder="t('layout.shell.search.hint')"
                            class="min-w-0 flex-1 bg-transparent text-base text-text-color outline-none placeholder:text-text-color-secondary"
                        />
                    </div>
                    <ListboxContent class="min-h-0 flex-1 overflow-y-auto p-2">
                        <!-- First run: the whole map of the app, so the palette
                             teaches the layout instead of showing an empty box. -->
                        <template v-if="showBrowse">
                            <div :class="labelClass">{{ t('layout.shell.search.jumpTo') }}</div>
                            <ListboxGroup v-for="column in browseMap" :key="column.groupKey">
                                <ListboxGroupLabel :class="labelClass">
                                    {{ t(column.groupKey) }}
                                </ListboxGroupLabel>
                                <ListboxItem
                                    v-for="entry in column.entries"
                                    :key="entry.id"
                                    :value="entry.id"
                                    :class="itemClass"
                                >
                                    <span class="truncate text-base">{{ entry.label }}</span>
                                </ListboxItem>
                            </ListboxGroup>
                        </template>
                        <template v-else-if="showRecents">
                            <ListboxGroup>
                                <ListboxGroupLabel :class="labelClass">
                                    {{ t('layout.shell.search.recent') }}
                                </ListboxGroupLabel>
                                <ListboxItem
                                    v-for="entry in recents"
                                    :key="entry.id"
                                    :value="entry.id"
                                    :class="itemClass"
                                >
                                    <span class="truncate text-base">{{ entry.label }}</span>
                                    <span class="truncate text-sm text-text-color-secondary">{{
                                        entry.breadcrumb.join(' › ')
                                    }}</span>
                                </ListboxItem>
                            </ListboxGroup>
                        </template>
                        <template v-else-if="groups.length > 0">
                            <ListboxGroup v-for="group in groups" :key="group.kind">
                                <ListboxGroupLabel :class="labelClass">
                                    {{ t(KIND_LABEL_KEYS[group.kind]) }}
                                </ListboxGroupLabel>
                                <ListboxItem
                                    v-for="entry in visible(group)"
                                    :key="entry.id"
                                    :value="entry.id"
                                    :class="itemClass"
                                >
                                    <span class="truncate text-base">{{ entry.label }}</span>
                                    <span
                                        v-if="entry.breadcrumb.length > 0"
                                        class="truncate text-sm text-text-color-secondary"
                                        >{{ entry.breadcrumb.join(' › ') }}</span
                                    >
                                </ListboxItem>
                                <ListboxItem
                                    v-if="group.hidden.length > 0 && !expanded.has(group.kind)"
                                    :value="`more:${group.kind}`"
                                    class="flex cursor-pointer items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm text-text-color-secondary outline-none data-[highlighted]:bg-surface-hover"
                                >
                                    <svg-icon type="mdi" :path="mdiChevronDown" :size="14" />
                                    {{
                                        t('layout.shell.search.more', {
                                            count: group.hidden.length,
                                        })
                                    }}
                                </ListboxItem>
                            </ListboxGroup>
                        </template>
                        <div v-else class="px-3 py-8 text-center text-text-color-secondary">
                            {{ t('layout.shell.search.noResults') }}
                        </div>
                    </ListboxContent>
                </ListboxRoot>
            </DialogContent>
        </DialogPortal>
    </DialogRoot>
</template>
