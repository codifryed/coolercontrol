<!--
  SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { mdiAlertOutline, mdiCheck, mdiContentCopy, mdiImport } from '@mdi/js'
import {
    computed,
    inject,
    nextTick,
    onMounted,
    onUnmounted,
    reactive,
    type Ref,
    ref,
    watch,
} from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { useDaemonState } from '@/stores/DaemonState.ts'
import { useConfirm } from '@/shell/confirm'
import { useRoute } from 'vue-router'
import { useShortcutsDialog } from '@/composables/useShortcutsDialog.ts'
import UiSelect from '@/shell/ui/UiSelect.vue'
import UiSettingRow from '@/shell/ui/UiSettingRow.vue'
import UiSettingsCard from '@/shell/ui/UiSettingsCard.vue'
import UiSettingGroup from '@/shell/ui/UiSettingGroup.vue'
import UiLockToggle from '@/shell/ui/UiLockToggle.vue'
import UiRestartHint from '@/shell/ui/UiRestartHint.vue'
import UiNumberInput from '@/shell/ui/UiNumberInput.vue'
import UiInput from '@/shell/ui/UiInput.vue'
import UiButton from '@/shell/ui/UiButton.vue'
import { useToast } from '@/shell/toast'
import {
    BUILT_IN_THEME_MODES,
    CustomThemeSettings,
    defaultCustomTheme,
    getInterfaceFontDisplayName,
    getThemeModeDisplayName,
    InterfaceFont,
    StartupPage,
    ThemeMode,
} from '@/models/UISettings.ts'
import {
    INSTALLED_THEMES,
    probeCompiledSwatch,
    surfaceTintFor,
    swatchFor,
    SYSTEM_THEME_ID,
    THEME_TOKEN_KEYS,
    THEME_TOKEN_VARS,
    type ThemeSwatch,
    type ThemeTokens,
} from '@/shell/themes.ts'
import { decodeThemeCode, encodeThemeCode, type ThemeHexTokens } from '@/shell/themeCode.ts'
import type { UiSelectOption } from '@/shell/ui/UiSelect.vue'
import ThemeSwatchStrip from '@/shell/ui/ThemeSwatchStrip.vue'
import { Color } from '@/models/Device.ts'
import { Emitter, EventType } from 'mitt'
import LanguageSwitcher from '@/components/LanguageSwitcher.vue'
import { useI18n } from 'vue-i18n'
import { api as fullscreenApi } from 'vue-fullscreen'
import _ from 'lodash'
import CCColorPicker from '@/components/CCColorPicker.vue'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore.ts'
import UiSwitch from '@/shell/ui/UiSwitch.vue'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const daemonState = useDaemonState()
const colorStore = useThemeColorsStore()
const confirm = useConfirm()
const { openShortcutsDialog } = useShortcutsDialog()
const route = useRoute()
const toast = useToast()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!

interface Props {
    tabNumber?: string
}

const props = defineProps<Props>()

const { t } = useI18n()

// Panel links land on /settings/:tabNumber; scroll the matching card group
// into view. Numeric values are the legacy tab indexes.
const ANCHOR_IDS: Record<string, string> = {
    '0': 'settings-general',
    general: 'settings-general',
    appearance: 'settings-appearance',
    theme: 'settings-theme',
    '1': 'settings-daemon',
    daemon: 'settings-daemon',
    '2': 'settings-desktop',
    desktop: 'settings-desktop',
}
// Rings the row a search result asked for, then clears itself. Same shape as
// the device highlight in ManageSensorsView: instant scroll to centre, a 2.5s
// ring, and an identity re-check so a second search cannot clear the first.
const HIGHLIGHT_MS = 2500
const highlightId = ref<string | null>(null)

// An unknown param is a row anchor, not a card: it goes straight to
// getElementById. Cards and rows scroll the same way, centred and instant, so
// the page does not behave differently depending on what was searched.
const scrollToSection = (tab: string | undefined): void => {
    const id = ANCHOR_IDS[tab ?? ''] ?? tab
    if (id == null || id === '') return
    const isRow = id.startsWith('setting-')
    nextTick(() => {
        document.getElementById(id)?.scrollIntoView({ block: 'center' })
        if (!isRow) return
        highlightId.value = id
        setTimeout(() => {
            if (highlightId.value === id) highlightId.value = null
        }, HIGHLIGHT_MS)
    })
}
watch(
    () => route.params.tabNumber,
    (tab) => scrollToSection(tab as string | undefined),
)
onMounted(() => scrollToSection(props.tabNumber))

const isFullScreen = ref(fullscreenApi.isFullscreen)
if (deviceStore.isQtApp()) {
    // @ts-ignore
    const ipc = window.ipc
    isFullScreen.value = await ipc.getIsFullScreen()
    ipc.fullScreenToggled.connect((fullscreen: boolean) => {
        isFullScreen.value = fullscreen
    })
}
const toggleFullScreen = async (_enable: string | number | boolean): Promise<void> => {
    await fullscreenApi.toggle(null, {
        callback: async (fullscreen: boolean) => {
            isFullScreen.value = fullscreen
            if (deviceStore.isQtApp()) {
                await deviceStore.sleep(50)
                // @ts-ignore
                const ipc = window.ipc
                isFullScreen.value = await ipc.getIsFullScreen()
            }
        },
    })
}

// Preview swatches. Compiled themes are read back from their own class so the
// picker cannot drift from tailwind.config.js; installed themes carry theirs,
// and Custom shows whatever the user has currently picked.
const COMPILED_THEME_CLASSES: Record<string, string> = {
    [ThemeMode.DARK]: 'dark-theme',
    [ThemeMode.LIGHT]: 'light-theme',
    [ThemeMode.HIGH_CONTRAST_DARK]: 'high-contrast-dark',
    [ThemeMode.HIGH_CONTRAST_LIGHT]: 'high-contrast-light',
}
const builtInSwatch = (mode: string): ThemeSwatch => {
    if (mode === ThemeMode.SYSTEM) {
        // Preview what System will actually apply. Falling back to the media query
        // here would advertise our own colors while the desktop's are what land.
        const palette = settingsStore.systemPalette
        if (palette?.tokens != null) {
            return swatchFor({
                id: SYSTEM_THEME_ID,
                name: 'System',
                variant: palette.variant ?? 'dark',
                tokens: palette.tokens,
            })
        }
        const prefersDark =
            palette?.variant != null
                ? palette.variant === 'dark'
                : !window.matchMedia('(prefers-color-scheme: light)').matches
        const swatch = probeCompiledSwatch(prefersDark ? 'dark-theme' : 'light-theme')
        if (palette?.accent != null) {
            swatch[2] = palette.accent
        }
        return swatch
    }
    return probeCompiledSwatch(COMPILED_THEME_CLASSES[mode] ?? 'dark-theme')
}
const customSwatch = (): ThemeSwatch => [
    `rgb(${settingsStore.customTheme.bgOne})`,
    `rgb(${settingsStore.customTheme.bgTwo})`,
    `rgb(${settingsStore.customTheme.accent})`,
    `rgb(${settingsStore.customTheme.textColor})`,
]
const themeSwatches = computed((): Map<string, ThemeSwatch> => {
    const swatches = new Map<string, ThemeSwatch>()
    for (const mode of BUILT_IN_THEME_MODES) swatches.set(mode, builtInSwatch(mode))
    for (const theme of INSTALLED_THEMES) swatches.set(theme.id, swatchFor(theme))
    swatches.set(ThemeMode.CUSTOM, customSwatch())
    return swatches
})

// Use computed to respond to language changes
const themeModeOptions = computed((): UiSelectOption[] => [
    ...BUILT_IN_THEME_MODES.map((mode) => ({
        value: mode as string,
        label: getThemeModeDisplayName(mode),
        group: t('layout.settings.themeGroups.builtIn'),
    })),
    ...INSTALLED_THEMES.map((theme) => ({
        value: theme.id,
        label: theme.name,
        group: t('layout.settings.themeGroups.installed'),
    })),
    {
        value: ThemeMode.CUSTOM as string,
        label: getThemeModeDisplayName(ThemeMode.CUSTOM),
        group: t('layout.settings.themeGroups.custom'),
    },
])
const changeThemeMode = async (value: string | undefined) => {
    if (value == null) {
        return // do not update on unselect
    }

    // Save the original theme mode
    const previousThemeMode = settingsStore.themeMode

    // Update the theme mode in settings store
    settingsStore.themeMode = value

    // Dynamically apply theme changes without page reload
    settingsStore.applyThemeMode()

    // Display success notification
    toast.add({
        severity: 'success',
        summary: t('common.success'),
        detail: t('layout.settings.themeChangeSuccess'),
        life: 2000,
    })

    // Trigger theme change event to notify other components
    window.dispatchEvent(
        new CustomEvent('theme-changed', {
            detail: {
                previousTheme: previousThemeMode,
                currentTheme: value,
            },
        }),
    )
}
const lineThicknessOptions = ref([
    { optionSize: 1, value: 0.5 },
    { optionSize: 2, value: 1.0 },
    { optionSize: 3, value: 1.5 },
    { optionSize: 4, value: 2.0 },
    { optionSize: 6, value: 3.0 },
])
// Enum values are kept for config compatibility; targets remap to the new
// shell sections (AppInfo -> Home, Controls -> Cooling, dashboards -> Monitoring).
const chartLineValue = computed({
    get: () => String(settingsStore.chartLineScale),
    set: (value: string | undefined) => {
        if (value != null) settingsStore.chartLineScale = Number(value)
    },
})
const lineThicknessSelectOptions = computed(() =>
    lineThicknessOptions.value.map((option) => ({
        label: `${option.optionSize}px`,
        value: String(option.value),
        px: option.optionSize,
    })),
)

const startupPageOptions = computed(() => [
    { value: StartupPage.AppInfo, label: t('layout.shell.home') },
    { value: StartupPage.Controls, label: t('layout.shell.cooling') },
    { value: StartupPage.HomeDashboard, label: t('models.startupPage.homeDashboard') },
])
// The custom theme carries the same tokens an installed theme does. One hex
// ref per token, mirrored into the store as `r g b` whenever a picker changes.
const customThemeHex = reactive({ ...defaultCustomTheme }) as Record<keyof ThemeTokens, Color>
for (const key of THEME_TOKEN_KEYS) {
    customThemeHex[key] = colorStore.rgbToHex(settingsStore.customTheme[key])
}

const setCustomColor = (key: keyof ThemeTokens, newHexColor: Color): void => {
    customThemeHex[key] = newHexColor
    settingsStore.customTheme[key] = colorStore.hexToRgbThemeString(newHexColor)
    document.documentElement.style.setProperty(
        THEME_TOKEN_VARS[key],
        settingsStore.customTheme[key],
    )
    if (key === 'bgOne') {
        // A custom theme declares no variant, so the hover tint follows the
        // background: a light one darkens instead of washing out.
        document.documentElement.style.setProperty(
            '--colors-surface-hover',
            surfaceTintFor(settingsStore.customTheme.bgOne),
        )
    }
    colorStore.reLoadThemeColors()
}

// Label keys predate the token names, so they are mapped rather than derived.
const CUSTOM_THEME_ROWS: Array<{ key: keyof ThemeTokens; labelKey: string }> = [
    { key: 'accent', labelKey: 'accent' },
    { key: 'accentGradientTo', labelKey: 'accentGradientTo' },
    { key: 'bgOne', labelKey: 'bgOne' },
    { key: 'bgTwo', labelKey: 'bgTwo' },
    { key: 'borderOne', labelKey: 'border' },
    { key: 'textColor', labelKey: 'text' },
    { key: 'textColorSecondary', labelKey: 'textSecondary' },
    { key: 'success', labelKey: 'success' },
    { key: 'warning', labelKey: 'warning' },
    { key: 'error', labelKey: 'error' },
    { key: 'info', labelKey: 'info' },
]

const applyGenericDaemonChange = _.debounce(
    () =>
        confirm.require({
            message: t('layout.settings.applySettingAndRestart'),
            header: t('layout.settings.restartHeader'),
            icon: mdiAlertOutline,
            defaultFocus: 'accept',
            acceptLabel: t('common.yes'),
            rejectLabel: t('common.no'),
            accept: async () => {
                settingsStore.ccSettings.poll_rate = pollRate.value
                // give the system a moment to make sure the pollRate has been saved ^
                await deviceStore.sleep(50)
                toast.add({
                    severity: 'success',
                    summary: t('layout.settings.success'),
                    detail: t('layout.settings.successDetail'),
                    life: 6000,
                })
                await deviceStore.daemonClient.shutdownDaemon()
                await deviceStore.waitAndReload()
            },
        }),
    2000,
)

// Boolean views over non-boolean settings for the kit switch.
const frequencyGhz = computed({
    get: () => settingsStore.frequencyPrecision === 1000,
    set: (value: boolean) => (settingsStore.frequencyPrecision = value ? 1000 : 1),
})
// Applied immediately rather than on restart: the switch is the preview.
const interfaceFont = computed({
    get: () => settingsStore.interfaceFont,
    set: (value: InterfaceFont) => {
        settingsStore.interfaceFont = value
        settingsStore.applyInterfaceFont()
    },
})
const interfaceFontOptions = computed<UiSelectOption[]>(() =>
    [InterfaceFont.BUNDLED, InterfaceFont.SYSTEM].map((font) => ({
        label: getInterfaceFontDisplayName(font),
        value: font,
    })),
)

const liquidctlInit = computed({
    get: () => !settingsStore.ccSettings.no_init,
    set: (value: boolean) => (settingsStore.ccSettings.no_init = !value),
})

// The setting can only raise the level, so debug forced by the env var or flag
// shows as on and locks the switch.
const debugLogging = computed({
    get: () => daemonState.debugLoggingForcedBy != null || settingsStore.ccSettings.debug_logging,
    set: (value: boolean) => (settingsStore.ccSettings.debug_logging = value),
})
const debugLoggingForcedDescription = computed((): string => {
    switch (daemonState.debugLoggingForcedBy) {
        case 'env':
            return t('layout.settings.debugLoggingForcedEnv')
        case 'flag':
            return t('layout.settings.debugLoggingForcedFlag')
        default:
            return ''
    }
})

const pollRate: Ref<number> = ref(settingsStore.ccSettings.poll_rate)
watch(pollRate, () => {
    applyGenericDaemonChange()
})

// Bounded daemon settings: the recommended band, and the hard bounds the lock
// toggle opens up to. Outside the band the input colors itself as a warning.
const STARTUP_DELAY_SAFE = { min: 2, max: 20 }
const STARTUP_DELAY_FULL = { min: 0, max: 120 }
const POLL_RATE_SAFE = { min: 1.0, max: 5.0 }
const POLL_RATE_FULL = { min: 0.5, max: 5.0 }

type Band = { min: number; max: number }
const outsideBand = (value: number, band: Band): boolean => value < band.min || value > band.max

// A saved value already outside its band comes up unlocked, so a reload never
// silently pulls it back in.
const startupDelayUnlocked: Ref<boolean> = ref(
    outsideBand(settingsStore.ccSettings.startup_delay, STARTUP_DELAY_SAFE),
)
const pollRateUnlocked: Ref<boolean> = ref(outsideBand(pollRate.value, POLL_RATE_SAFE))
const startupDelayBounds = computed(() =>
    startupDelayUnlocked.value ? STARTUP_DELAY_FULL : STARTUP_DELAY_SAFE,
)
const pollRateBounds = computed(() => (pollRateUnlocked.value ? POLL_RATE_FULL : POLL_RATE_SAFE))

// Re-locking pulls an out-of-band value back in, so the lock and the value
// never disagree. Poll rate goes through pollRate, whose watcher owns the
// daemon restart prompt; an in-band value is left alone and prompts nothing.
watch(startupDelayUnlocked, (unlocked) => {
    if (unlocked) return
    settingsStore.ccSettings.startup_delay = _.clamp(
        settingsStore.ccSettings.startup_delay,
        STARTUP_DELAY_SAFE.min,
        STARTUP_DELAY_SAFE.max,
    )
})
watch(pollRateUnlocked, (unlocked) => {
    if (unlocked) return
    pollRate.value = _.clamp(pollRate.value, POLL_RATE_SAFE.min, POLL_RATE_SAFE.max)
})

// This interface is used for exporting/importing custom HEX color themes
// We transition to and from the CustomThemeSettings interface which contains
// a 'R G B' string value.
interface CustomColorTheme extends CustomThemeSettings {}
class CustomColorTheme {
    static fromCustomThemeSettings(customThemeSettings: CustomThemeSettings): CustomColorTheme {
        const customColorTheme = new CustomColorTheme()
        for (const key of THEME_TOKEN_KEYS) {
            customColorTheme[key] = colorStore.rgbToHex(customThemeSettings[key])
        }
        return customColorTheme
    }

    static fromJson(jsonObj: any): CustomColorTheme {
        if (CustomColorTheme.isCustomThemeSettings(jsonObj)) {
            return CustomColorTheme.fromCustomThemeSettings(jsonObj)
        }
        throw new Error('Invalid JSON object for CustomColorTheme')
    }

    static isCustomThemeSettings(jsonObj: any): jsonObj is CustomThemeSettings {
        const valid = (value: any): boolean =>
            typeof value === 'string' && colorStore.isValidHex(value)
        // Exports written before the status colors existed carry only the first
        // six keys; they still import, and the rest fall back to the defaults.
        if (THEME_TOKEN_KEYS.slice(0, 6).some((key) => !valid(jsonObj[key]))) return false
        for (const key of THEME_TOKEN_KEYS) {
            if (jsonObj[key] == null) jsonObj[key] = colorStore.rgbToHex(defaultCustomTheme[key])
        }
        return THEME_TOKEN_KEYS.every((key) => valid(jsonObj[key]))
    }

    applyCustomColorTheme(): void {
        for (const key of THEME_TOKEN_KEYS) {
            setCustomColor(key, this[key])
        }
    }
}
/** The custom theme as hex, which is what the codec speaks. */
const customThemeAsHex = (theme: CustomThemeSettings): ThemeHexTokens =>
    Object.fromEntries(
        THEME_TOKEN_KEYS.map((key) => [key, colorStore.rgbToHex(theme[key])]),
    ) as ThemeHexTokens

/** A decoded code over the defaults, since a cct1 code omits the newer tokens. */
const themeFromCode = (input: string): CustomColorTheme | null => {
    const decoded = decodeThemeCode(input)
    if (decoded == null) return null
    const theme = CustomColorTheme.fromCustomThemeSettings(defaultCustomTheme)
    for (const key of THEME_TOKEN_KEYS) {
        const value = decoded[key]
        if (value != null) theme[key] = value
    }
    return theme
}

const themeCode = computed((): string =>
    encodeThemeCode(customThemeAsHex(settingsStore.customTheme)),
)

const pasteCodeInput: Ref<string> = ref('')
const justCopied: Ref<boolean> = ref(false)

const copyThemeCode = async (): Promise<void> => {
    try {
        await navigator.clipboard.writeText(themeCode.value)
        justCopied.value = true
        setTimeout(() => (justCopied.value = false), 1500)
        toast.add({
            severity: 'success',
            summary: t('common.success'),
            detail: t('layout.settings.customTheme.themeCodeCopied'),
            life: 2000,
        })
    } catch (e) {
        console.error('Clipboard write failed', e)
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: t('layout.settings.customTheme.invalidThemeCode'),
            life: 3000,
        })
    }
}

const applyPastedThemeCode = (): void => {
    const decoded = themeFromCode(pasteCodeInput.value)
    if (decoded == null) {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: t('layout.settings.customTheme.invalidThemeCode'),
            life: 3000,
        })
        return
    }
    decoded.applyCustomColorTheme()
    pasteCodeInput.value = ''
    toast.add({
        severity: 'success',
        summary: t('common.success'),
        detail: t('layout.settings.customTheme.themeApplied'),
        life: 2000,
    })
}

// DEPRECATED: JSON theme files are no longer offered in the UI. Theme codes are
// the way to share a custom theme now. The implementation below stays for one
// release so the removal is a single clean delete, and is referenced once at the
// end of this block only to keep it compiling.
const downloadThemeFileName = 'coolercontrol-color-theme.json'
const downloadThemeHref = computed((): string => {
    const customColorTheme = CustomColorTheme.fromCustomThemeSettings(settingsStore.customTheme)
    const blob = new Blob([JSON.stringify(customColorTheme)], { type: 'application/json' })
    return URL.createObjectURL(blob)
})
const downloadThemeDatasetURL = computed((): string => {
    return ['application/json', downloadThemeFileName, downloadThemeHref].join(':')
})
const createJsonUploader = (): void => {
    const inputFileElement = document.createElement('input')
    inputFileElement.setAttribute('type', 'file')
    inputFileElement.setAttribute('accept', '.json')
    inputFileElement.onchange = function () {
        getUploadedJson(this)
    }
    document.body.appendChild(inputFileElement)
    inputFileElement.click()
    inputFileElement.remove()
}
const getUploadedJson = (fileInput: any) => {
    const files = fileInput.files
    if (files.length <= 0) return
    const file = files[0]
    if (file.size > 1024) {
        console.error('JSON File too large for single Color Theme')
        return
    }
    file.text().then(function (text: string) {
        console.log('text', text)
        const result = JSON.parse(text)
        const customThemeSettings = CustomColorTheme.fromJson(result)
        customThemeSettings.applyCustomColorTheme()
        // const formatted = JSON.stringify(result, null, 2)
        // console.log('result', formatted)
    })
}
// End of the deprecated JSON theme file support. Delete this block together with
// the now-unused customTheme.export/import and tooltips.*ThemeFile strings.
void { downloadThemeHref, downloadThemeDatasetURL, createJsonUploader }

onMounted(() => {
    // Listen for language change events
    window.addEventListener('language-changed', () => {
        // When language changes, the computed property will automatically update, no need to manually assign value
        // themeModeOptions.value = [...] - this line would cause an error

        // Trigger theme options recalculation
        window.dispatchEvent(new CustomEvent('theme-options-updated'))
    })
})

// Remove event listeners when component is unmounted
onUnmounted(() => {
    window.removeEventListener('language-changed', () => {
        // Cleanup code
    })
})
</script>

<template>
    <div class="flex h-full flex-col overflow-y-auto p-4">
        <h1 class="pb-3 text-xl font-semibold text-text-color">
            {{ t('layout.settings.title') }}
        </h1>
        <div class="columns-1 gap-4 space-y-4 xl:columns-2 min-[1900px]:columns-3">
            <UiSettingsCard
                id="settings-general"
                class="break-inside-avoid"
                :title="t('layout.settings.general')"
            >
                <UiSettingRow
                    id="setting-introduction"
                    :highlighted="highlightId === 'setting-introduction'"
                >
                    <template #label>{{ t('layout.settings.introduction') }}</template>
                    <UiButton class="w-full" @click="emitter.emit('start-tour')">{{
                        t('layout.settings.startTour')
                    }}</UiButton>
                </UiSettingRow>
                <UiSettingRow
                    id="setting-shortcuts"
                    :highlighted="highlightId === 'setting-shortcuts'"
                    :label="t('views.shortcuts.shortcuts')"
                >
                    <UiButton variant="outline" class="w-full" @click="openShortcutsDialog()">{{
                        t('views.shortcuts.shortcuts')
                    }}</UiButton>
                </UiSettingRow>
                <UiSettingRow
                    id="setting-startup-page"
                    :highlighted="highlightId === 'setting-startup-page'"
                    v-tooltip.top="t('layout.settings.tooltips.startupPage')"
                    :label="t('layout.settings.startupPage')"
                >
                    <UiSelect
                        v-model="settingsStore.startupPage"
                        :options="startupPageOptions"
                        class="w-full"
                    />
                </UiSettingRow>
                <UiSettingRow
                    id="setting-language"
                    :highlighted="highlightId === 'setting-language'"
                    v-tooltip.top="t('layout.settings.language')"
                >
                    <template #label>{{ t('layout.settings.language') }}</template>
                    <LanguageSwitcher />
                </UiSettingRow>
                <UiSettingRow
                    id="setting-full-screen"
                    :highlighted="highlightId === 'setting-full-screen'"
                    v-tooltip.top="t('layout.settings.tooltips.fullScreen')"
                    :label="t('layout.settings.fullScreen')"
                >
                    <UiSwitch
                        v-model="isFullScreen"
                        :disabled="!fullscreenApi.isEnabled"
                        @update:model-value="toggleFullScreen"
                    />
                </UiSettingRow>
            </UiSettingsCard>
            <UiSettingsCard
                id="settings-appearance"
                class="break-inside-avoid"
                :title="t('layout.settings.appearance')"
            >
                <UiSettingRow
                    id="setting-theme-style"
                    :highlighted="highlightId === 'setting-theme-style'"
                    v-tooltip.top="t('layout.settings.appearance')"
                >
                    <template #label>{{ t('layout.settings.themeStyle') }}</template>
                    <UiSelect
                        :model-value="settingsStore.themeMode"
                        :options="themeModeOptions"
                        class="w-full"
                        @update:model-value="changeThemeMode"
                    >
                        <template #value="{ option }">
                            <span class="flex items-center gap-2 truncate">
                                <ThemeSwatchStrip :colors="themeSwatches.get(option.value)" />
                                {{ option.label }}
                            </span>
                        </template>
                        <template #option="{ option }">
                            <span class="flex items-center gap-2">
                                <ThemeSwatchStrip :colors="themeSwatches.get(option.value)" />
                                {{ option.label }}
                            </span>
                        </template>
                    </UiSelect>
                </UiSettingRow>
                <UiSettingRow
                    id="setting-dashboard-line-size"
                    :highlighted="highlightId === 'setting-dashboard-line-size'"
                    v-tooltip.top="t('layout.settings.tooltips.lineThickness')"
                    :label="t('layout.settings.dashboardLineSize')"
                >
                    <UiSelect
                        v-model="chartLineValue"
                        :options="lineThicknessSelectOptions"
                        class="w-full"
                    >
                        <template #option="{ option }">
                            <span class="flex w-full items-center gap-3">
                                <span
                                    class="block w-16 rounded bg-text-color"
                                    :style="{
                                        height: `${(option as any).px}px`,
                                    }"
                                />
                                {{ option.label }}
                            </span>
                        </template>
                    </UiSelect>
                </UiSettingRow>
                <UiSettingRow
                    id="setting-time-format"
                    :highlighted="highlightId === 'setting-time-format'"
                    v-tooltip.top="t('layout.settings.tooltips.timeFormat')"
                    :label="t('layout.settings.timeFormat')"
                >
                    <span class="inline-flex items-center justify-center gap-2">
                        <span>{{ t('layout.settings.time12h') }}</span>
                        <UiSwitch v-model="settingsStore.time24" two-sided />
                        <span>{{ t('layout.settings.time24h') }}</span>
                    </span>
                </UiSettingRow>
                <UiSettingRow
                    id="setting-frequency-precision"
                    :highlighted="highlightId === 'setting-frequency-precision'"
                    v-tooltip.top="t('layout.settings.tooltips.frequencyPrecision')"
                    :label="t('layout.settings.frequencyPrecision')"
                >
                    <span class="inline-flex items-center justify-center gap-2">
                        <span>{{ t('common.mhzAbbr') }}</span>
                        <UiSwitch v-model="frequencyGhz" two-sided />
                        <span>{{ t('common.ghzAbbr') }}</span>
                    </span>
                </UiSettingRow>
                <UiSettingRow
                    id="setting-eye-candy"
                    :highlighted="highlightId === 'setting-eye-candy'"
                    v-tooltip.top="t('layout.settings.tooltips.eyeCandy')"
                    :label="t('layout.settings.eyeCandy')"
                >
                    <UiSwitch v-model="settingsStore.eyeCandy" />
                </UiSettingRow>
                <UiSettingRow
                    id="setting-rail-to-collapse"
                    :highlighted="highlightId === 'setting-rail-to-collapse'"
                    v-tooltip.top="t('layout.settings.tooltips.railToCollapse')"
                    :label="t('layout.settings.railToCollapse')"
                >
                    <UiSwitch v-model="settingsStore.hideMenuCollapseIcon" />
                </UiSettingRow>
                <UiSettingRow
                    id="setting-interface-font"
                    :highlighted="highlightId === 'setting-interface-font'"
                    v-tooltip.top="t('layout.settings.tooltips.interfaceFont')"
                    :label="t('layout.settings.interfaceFont')"
                >
                    <UiSelect
                        v-model="interfaceFont"
                        :options="interfaceFontOptions"
                        class="w-44"
                    />
                </UiSettingRow>
            </UiSettingsCard>
            <UiSettingsCard
                id="settings-theme"
                v-if="settingsStore.themeMode === ThemeMode.CUSTOM"
                class="break-inside-avoid"
                :title="t('layout.settings.customTheme.title')"
            >
                <UiSettingRow
                    v-for="row in CUSTOM_THEME_ROWS"
                    :id="`setting-theme-${row.key}`"
                    :key="row.key"
                    :label="t(`layout.settings.customTheme.${row.labelKey}`)"
                >
                    <div class="w-full h-full content-center flex justify-center">
                        <div :class="row.key === 'bgTwo' ? 'rounded-lg bg-bg-one' : ''">
                            <c-c-color-picker
                                v-model="customThemeHex[row.key]"
                                color-format="hex"
                                :default-color="colorStore.rgbToHex(defaultCustomTheme[row.key])"
                                @update:model-value="setCustomColor(row.key, $event)"
                            />
                        </div>
                    </div>
                </UiSettingRow>
                <div v-tooltip.top="t('layout.settings.tooltips.copyThemeCode')" class="px-4 py-3">
                    <div class="flex items-center gap-2 w-full">
                        <div class="pr-4 text-right">
                            {{ t('layout.settings.customTheme.copyCode') }}
                        </div>
                        <UiInput
                            :model-value="themeCode"
                            readonly
                            class="flex-1 font-mono text-xs"
                            @focus="($event.target as HTMLInputElement).select()"
                        />
                        <UiButton size="icon" @click="copyThemeCode">
                            <svg-icon
                                class="shrink-0 outline-0"
                                type="mdi"
                                :path="justCopied ? mdiCheck : mdiContentCopy"
                                :size="deviceStore.getREMSize(1.25)"
                            />
                        </UiButton>
                    </div>
                </div>
                <div v-tooltip.top="t('layout.settings.tooltips.pasteThemeCode')" class="px-4 py-3">
                    <div class="flex items-center gap-2 w-full">
                        <div class="pr-4 text-right">
                            {{ t('layout.settings.customTheme.pasteCode') }}
                        </div>
                        <UiInput
                            v-model="pasteCodeInput"
                            class="flex-1 font-mono text-xs"
                            placeholder="cct1:..."
                            @keyup.enter="applyPastedThemeCode"
                        />
                        <UiButton
                            size="icon"
                            :disabled="pasteCodeInput.trim().length === 0"
                            @click="applyPastedThemeCode"
                        >
                            <svg-icon
                                class="shrink-0 outline-0"
                                type="mdi"
                                :path="mdiImport"
                                :size="deviceStore.getREMSize(1.5)"
                            />
                        </UiButton>
                    </div>
                </div>
            </UiSettingsCard>
            <UiSettingsCard
                id="settings-daemon"
                class="break-inside-avoid"
                :title="t('views.daemon.title')"
            >
                <UiSettingGroup :title="t('layout.settings.groups.startup')">
                    <UiSettingRow
                        id="setting-apply-on-startup"
                        :highlighted="highlightId === 'setting-apply-on-startup'"
                        v-tooltip.top="{
                            escape: false,
                            value: t('layout.settings.tooltips.applySettingsOnStartup'),
                        }"
                        :label="t('layout.settings.applySettingsOnStartup')"
                    >
                        <UiSwitch v-model="settingsStore.ccSettings.apply_on_boot" />
                    </UiSettingRow>
                    <UiSettingRow
                        id="setting-device-startup-delay"
                        :highlighted="highlightId === 'setting-device-startup-delay'"
                        v-tooltip.top="{
                            escape: false,
                            value: t('layout.settings.tooltips.deviceDelayAtStartup'),
                        }"
                        :label="t('layout.settings.deviceDelayAtStartup')"
                    >
                        <div class="flex items-center gap-1">
                            <UiLockToggle
                                v-model="startupDelayUnlocked"
                                v-tooltip.top="
                                    startupDelayUnlocked
                                        ? t('layout.settings.tooltips.lockRange')
                                        : t('layout.settings.tooltips.unlockRange')
                                "
                            />
                            <UiNumberInput
                                v-model="settingsStore.ccSettings.startup_delay"
                                :min="startupDelayBounds.min"
                                :max="startupDelayBounds.max"
                                :safe-min="STARTUP_DELAY_SAFE.min"
                                :safe-max="STARTUP_DELAY_SAFE.max"
                                :suffix="t('common.secondAbbr')"
                            />
                        </div>
                    </UiSettingRow>
                </UiSettingGroup>
                <UiSettingGroup :title="t('layout.settings.groups.performance')">
                    <UiSettingRow
                        id="setting-polling-rate"
                        :highlighted="highlightId === 'setting-polling-rate'"
                        v-tooltip.top="{
                            escape: false,
                            value: t('layout.settings.tooltips.pollingRate'),
                        }"
                        :label="t('layout.settings.pollingRate')"
                    >
                        <div class="flex items-center gap-1">
                            <UiRestartHint />
                            <UiLockToggle
                                v-model="pollRateUnlocked"
                                v-tooltip.top="
                                    pollRateUnlocked
                                        ? t('layout.settings.tooltips.lockRange')
                                        : t('layout.settings.tooltips.unlockRange')
                                "
                            />
                            <UiNumberInput
                                v-model="pollRate"
                                :min="pollRateBounds.min"
                                :max="pollRateBounds.max"
                                :safe-min="POLL_RATE_SAFE.min"
                                :safe-max="POLL_RATE_SAFE.max"
                                :step="0.5"
                                :suffix="t('common.secondAbbr')"
                            />
                        </div>
                    </UiSettingRow>
                    <UiSettingRow
                        id="setting-compress-api-payload"
                        :highlighted="highlightId === 'setting-compress-api-payload'"
                        v-tooltip.top="{
                            escape: false,
                            value: t('layout.settings.tooltips.compressApiPayload'),
                        }"
                        :label="t('layout.settings.compressApiPayload')"
                    >
                        <div class="flex items-center gap-1">
                            <UiRestartHint />
                            <UiSwitch
                                v-model="settingsStore.ccSettings.compress"
                                @click="applyGenericDaemonChange"
                            />
                        </div>
                    </UiSettingRow>
                </UiSettingGroup>
                <UiSettingGroup :title="t('layout.settings.groups.devices')">
                    <UiSettingRow
                        id="setting-sensors-auto-detect"
                        :highlighted="highlightId === 'setting-sensors-auto-detect'"
                        v-tooltip.top="{
                            escape: false,
                            value: t('layout.settings.tooltips.sensorsAutoDetect'),
                        }"
                        :label="t('layout.settings.sensorsAutoDetect')"
                    >
                        <div class="flex items-center gap-1">
                            <UiRestartHint />
                            <UiSwitch
                                v-model="settingsStore.ccSettings.sensors_auto_detect"
                                @update:model-value="applyGenericDaemonChange"
                            />
                        </div>
                    </UiSettingRow>
                    <UiSettingRow
                        id="setting-sensors-config"
                        :highlighted="highlightId === 'setting-sensors-config'"
                        v-tooltip.top="{
                            escape: false,
                            value: t('layout.settings.tooltips.sensorsConfig'),
                        }"
                        :label="t('layout.settings.sensorsConfig')"
                    >
                        <div class="flex items-center gap-1">
                            <UiRestartHint />
                            <UiSwitch
                                v-model="settingsStore.ccSettings.sensors_conf_enabled"
                                @update:model-value="applyGenericDaemonChange"
                            />
                        </div>
                    </UiSettingRow>
                    <UiSettingRow
                        id="setting-device-listener"
                        :highlighted="highlightId === 'setting-device-listener'"
                        v-tooltip.top="{
                            escape: false,
                            value: t('layout.settings.tooltips.deviceListener'),
                        }"
                        :label="t('layout.settings.deviceListener')"
                    >
                        <div class="flex items-center gap-1">
                            <UiRestartHint />
                            <UiSwitch
                                v-model="settingsStore.ccSettings.device_listener_enabled"
                                @update:model-value="applyGenericDaemonChange"
                            />
                        </div>
                    </UiSettingRow>
                    <UiSettingRow
                        id="setting-drive-power-state"
                        :highlighted="highlightId === 'setting-drive-power-state'"
                        v-tooltip.top="{
                            escape: false,
                            value: t('layout.settings.tooltips.drivePowerState'),
                        }"
                        :label="t('layout.settings.drivePowerState')"
                    >
                        <div class="flex items-center gap-1">
                            <UiRestartHint />
                            <UiSwitch
                                v-model="settingsStore.ccSettings.drivetemp_suspend"
                                @update:model-value="applyGenericDaemonChange"
                            />
                        </div>
                    </UiSettingRow>
                </UiSettingGroup>
                <UiSettingGroup :title="t('layout.settings.groups.liquidctl')">
                    <UiSettingRow
                        id="setting-liquidctl-integration"
                        :highlighted="highlightId === 'setting-liquidctl-integration'"
                        v-tooltip.top="{
                            escape: false,
                            value: t('layout.settings.tooltips.liquidctlIntegration'),
                        }"
                        :label="t('layout.settings.liquidctlIntegration')"
                    >
                        <div class="flex items-center gap-1">
                            <UiRestartHint />
                            <UiSwitch
                                v-model="settingsStore.ccSettings.liquidctl_integration"
                                @update:model-value="applyGenericDaemonChange"
                            />
                        </div>
                    </UiSettingRow>
                    <UiSettingRow
                        id="setting-liquidctl-device-init"
                        :highlighted="highlightId === 'setting-liquidctl-device-init'"
                        v-tooltip.top="{
                            escape: false,
                            value: t('layout.settings.tooltips.liquidctlDeviceInit'),
                        }"
                        :label="t('layout.settings.liquidctlDeviceInit')"
                    >
                        <UiSwitch
                            v-model="liquidctlInit"
                            :disabled="!settingsStore.ccSettings.liquidctl_integration"
                        />
                    </UiSettingRow>
                    <UiSettingRow
                        id="setting-hide-duplicate-devices"
                        :highlighted="highlightId === 'setting-hide-duplicate-devices'"
                        v-tooltip.top="{
                            escape: false,
                            value: t('layout.settings.tooltips.hideDuplicateDevices'),
                        }"
                        :label="t('layout.settings.hideDuplicateDevices')"
                    >
                        <div class="flex items-center gap-1">
                            <UiRestartHint />
                            <UiSwitch
                                v-model="settingsStore.ccSettings.hide_duplicate_devices"
                                :disabled="!settingsStore.ccSettings.liquidctl_integration"
                                @update:model-value="applyGenericDaemonChange"
                            />
                        </div>
                    </UiSettingRow>
                </UiSettingGroup>
                <UiSettingGroup :title="t('layout.settings.groups.logging')">
                    <UiSettingRow
                        id="setting-debug-logging"
                        :highlighted="highlightId === 'setting-debug-logging'"
                        v-tooltip.top="{
                            escape: false,
                            value: t('layout.settings.tooltips.debugLogging'),
                        }"
                        :label="t('layout.settings.debugLogging')"
                        :description="debugLoggingForcedDescription"
                    >
                        <div class="flex items-center gap-1">
                            <UiRestartHint />
                            <UiSwitch
                                v-model="debugLogging"
                                :disabled="daemonState.debugLoggingForcedBy != null"
                                @update:model-value="applyGenericDaemonChange"
                            />
                        </div>
                    </UiSettingRow>
                </UiSettingGroup>
            </UiSettingsCard>
            <UiSettingsCard
                id="settings-desktop"
                v-if="deviceStore.isQtApp()"
                class="break-inside-avoid"
                :title="t('layout.settings.desktop')"
            >
                <UiSettingRow
                    id="setting-start-in-tray"
                    :highlighted="highlightId === 'setting-start-in-tray'"
                    v-tooltip.top="{
                        escape: false,
                        value: t('layout.settings.tooltips.startInTray'),
                    }"
                    :label="t('layout.settings.startInTray')"
                >
                    <UiSwitch v-model="settingsStore.startInSystemTray" />
                </UiSettingRow>
                <UiSettingRow
                    id="setting-close-to-tray"
                    :highlighted="highlightId === 'setting-close-to-tray'"
                    v-tooltip.top="{
                        escape: false,
                        value: t('layout.settings.tooltips.closeToTray'),
                    }"
                    :label="t('layout.settings.closeToTray')"
                >
                    <UiSwitch v-model="settingsStore.closeToSystemTray" />
                </UiSettingRow>
                <UiSettingRow
                    id="setting-zoom"
                    :highlighted="highlightId === 'setting-zoom'"
                    v-tooltip.top="{
                        escape: false,
                        value: t('layout.settings.tooltips.zoom'),
                    }"
                    :label="t('layout.settings.zoom')"
                >
                    <UiNumberInput
                        v-model="settingsStore.uiScale"
                        :min="50"
                        :max="400"
                        :step="10"
                        :suffix="t('common.percentUnit')"
                    />
                </UiSettingRow>
                <UiSettingRow
                    id="setting-desktop-startup-delay"
                    :highlighted="highlightId === 'setting-desktop-startup-delay'"
                    v-tooltip.top="{
                        escape: false,
                        value: t('layout.settings.tooltips.desktopStartupDelay'),
                    }"
                    :label="t('layout.settings.desktopStartupDelay')"
                >
                    <UiNumberInput
                        v-model="settingsStore.desktopStartupDelay"
                        :min="0"
                        :max="10"
                        :step="1"
                        :suffix="t('common.secondAbbr')"
                    />
                </UiSettingRow>
            </UiSettingsCard>
        </div>
    </div>
</template>

<style scoped lang="scss"></style>

<style lang="scss">
.el-tree-node__content {
    border-radius: 0.5rem;
}

.el-tree-node__expand-icon {
    font-size: 1rem;
    padding-left: 1px !important;
}
</style>
