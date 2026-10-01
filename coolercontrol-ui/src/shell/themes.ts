// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

/**
 * Installed color themes.
 *
 * Each theme carries the tokens a theme is allowed to define. The palette
 * hues (pink/green/red/yellow/blue/white) are deliberately NOT themed and stay
 * global. Nothing that carries meaning may use them: a status color comes from
 * `success`/`warning`/`error`/`info`, which follow the theme and hold the contrast
 * floor below. The hues do neither.
 *
 * Themes are applied as inline `--colors-*` variables rather than compiled
 * tailwind themes, so adding one is a single entry here. `surface-hover` and the
 * accent/error foregrounds are derived, not stored.
 *
 * Values follow each project's published palette. Where an upstream status color
 * was too close to its own background to read, it is nudged within its hue until
 * it clears the floor in `themes.spec.ts`; those are marked `nudged from`.
 */

/** The tokens a theme defines. Everything else is derived or global. */
export interface ThemeTokens {
    accent: string
    /** Far end of the brand gradient. Equal to `accent` means no gradient. */
    accentGradientTo: string
    bgOne: string
    bgTwo: string
    borderOne: string
    textColor: string
    textColorSecondary: string
    success: string
    warning: string
    error: string
    info: string
}

export interface InstalledTheme {
    /** Stable id; persisted as the UI's theme mode. */
    id: string
    /** Display name. A proper noun, so it is never translated. */
    name: string
    /** Drives the derived surface-hover tint and the OS color-scheme hint. */
    variant: 'dark' | 'light'
    tokens: ThemeTokens
}

/**
 * Patternfly Project Felt themes
 * Source: https://www.patternfly.org/foundations-and-styles/design-tokens/all-design-tokens
 */
const PROJECT_FELT_THEME_TOKENS: Record<
    'light' | 'lightHighContrast' | 'dark' | 'darkHighContrast',
    ThemeTokens
> = {
    light: {
        accent: '#0066cc', // --pf-t--global--text--color--brand--default
        accentGradientTo: '#0066cc',
        bgOne: '#ffffff', // --pf-t--global--background--color--primary--default
        bgTwo: '#f2f2f2', // --pf-t--global--background--color--secondary--default
        borderOne: '#e0e0e0', // --pf-t--global--border--color--default
        textColor: '#151515', // --pf-t--global--text--color--regular
        textColorSecondary: '#4d4d4d', // --pf-t--global--text--color--subtle
        success: '#3d7317', // --pf-t--global--text--color--status--success--default
        warning: '#dca614', // --pf-t--global--text--color--status--warning--default
        error: '#b1380b', // --pf-t--global--text--color--status--danger--default
        info: '#5e40be', // --pf-t--global--text--color--status--info--default
    },
    lightHighContrast: {
        accent: '#003366', // --pf-t--global--text--color--brand--default
        accentGradientTo: '#003366',
        bgOne: '#ffffff', // --pf-t--global--background--color--primary--default
        bgTwo: '#f2f2f2', // --pf-t--global--background--color--secondary--default
        borderOne: '#4d4d4d', // --pf-t--global--border--color--default
        textColor: '#151515', // --pf-t--global--text--color--regular
        textColorSecondary: '#383838', // --pf-t--global--text--color--subtle
        success: '#204d00', // --pf-t--global--text--color--status--success--default
        warning: '#73480b', // --pf-t--global--text--color--status--warning--default
        error: '#731f00', // --pf-t--global--text--color--status--danger--default
        info: '#3d2785', // --pf-t--global--text--color--status--info--default
    },
    dark: {
        accent: '#b9dafc', // --pf-t--global--text--color--brand--default
        accentGradientTo: '#b9dafc',
        bgOne: '#292929', // --pf-t--global--background--color--primary--default
        bgTwo: '#151515', // --pf-t--global--background--color--secondary--default
        borderOne: '#4d4d4d', // --pf-t--global--border--color--default
        textColor: '#ffffff', // --pf-t--global--text--color--regular
        textColorSecondary: '#c7c7c7', // --pf-t--global--text--color--subtle
        success: '#87bb62', // --pf-t--global--text--color--status--success--default
        warning: '#ffcc17', // --pf-t--global--text--color--status--warning--default
        error: '#f89b78', // --pf-t--global--text--color--status--danger--default
        info: '#b6a6e9', // --pf-t--global--text--color--status--info--default
    },
    darkHighContrast: {
        accent: '#b9dafc', // --pf-t--global--text--color--brand--default
        accentGradientTo: '#b9dafc',
        bgOne: '#000000', // --pf-t--global--background--color--primary--default
        bgTwo: '#151515', // --pf-t--global--background--color--secondary--default
        borderOne: '#c7c7c7', // --pf-t--global--border--color--default
        textColor: '#ffffff', // --pf-t--global--text--color--regular
        textColorSecondary: '#c7c7c7', // --pf-t--global--text--color--subtle
        success: '#afdc8f', // --pf-t--global--text--color--status--success--default
        warning: '#ffcc17', // --pf-t--global--text--color--status--warning--default
        error: '#fbbea8', // --pf-t--global--text--color--status--danger--default
        info: '#b6a6e9', // --pf-t--global--text--color--status--info--default
    },
}

export const INSTALLED_THEMES: InstalledTheme[] = [
    {
        id: 'true-black',
        name: 'True Black',
        variant: 'dark',
        tokens: {
            accent: '#4d8cff',
            accentGradientTo: '#ff21ff', // CoolerControl logo magenta
            bgOne: '#000000',
            bgTwo: '#121212',
            borderOne: '#2e2e2e',
            textColor: '#e6e6e6',
            textColorSecondary: '#9a9a9a',
            success: '#3ddc84',
            warning: '#ffc14d',
            error: '#ff6b6b',
            info: '#4da3ff',
        },
    },
    {
        id: 'dracula',
        name: 'Dracula',
        variant: 'dark',
        tokens: {
            accent: '#bd93f9',
            accentGradientTo: '#ff79c6', // Dracula pink
            bgOne: '#282a36',
            bgTwo: '#343746',
            borderOne: '#44475a',
            textColor: '#f8f8f2',
            textColorSecondary: '#8b9bd4',
            success: '#50fa7b',
            warning: '#f1fa8c',
            error: '#ff5555',
            info: '#8be9fd',
        },
    },
    {
        id: 'gruvbox-dark',
        name: 'Gruvbox Dark',
        variant: 'dark',
        tokens: {
            accent: '#83a598',
            accentGradientTo: '#d3869b', // Gruvbox bright purple
            bgOne: '#282828',
            bgTwo: '#3c3836',
            borderOne: '#504945',
            textColor: '#ebdbb2',
            textColorSecondary: '#a89984',
            success: '#b8bb26',
            warning: '#fabd2f',
            error: '#fb4934',
            info: '#83a598',
        },
    },
    {
        id: 'gruvbox-light',
        name: 'Gruvbox Light',
        variant: 'light',
        tokens: {
            accent: '#076678',
            accentGradientTo: '#8f3f71', // Gruvbox faded purple
            bgOne: '#fbf1c7',
            bgTwo: '#ebdbb2',
            borderOne: '#d5c4a1',
            textColor: '#3c3836',
            textColorSecondary: '#7c6f64',
            success: '#79740e',
            warning: '#8f5f0a', // nudged from #b57614
            error: '#9d0006',
            info: '#076678',
        },
    },
    {
        id: 'one-dark',
        name: 'One Dark',
        variant: 'dark',
        tokens: {
            accent: '#61afef',
            accentGradientTo: '#c678dd', // One Dark purple
            bgOne: '#282c34',
            bgTwo: '#31363f',
            borderOne: '#3e4451',
            textColor: '#abb2bf',
            textColorSecondary: '#8b92a0',
            success: '#98c379',
            warning: '#e5c07b',
            error: '#e06c75',
            info: '#56b6c2',
        },
    },
    {
        id: 'one-light',
        name: 'One Light',
        variant: 'light',
        tokens: {
            accent: '#4078f2',
            accentGradientTo: '#a626a4', // One Light magenta
            bgOne: '#fafafa',
            bgTwo: '#eaeaeb',
            borderOne: '#d3d3d4',
            textColor: '#383a42',
            textColorSecondary: '#696c77',
            success: '#3f7f3e', // nudged from #50a14f
            warning: '#9a6700', // nudged from #c18401
            error: '#ca1243',
            info: '#0184bc',
        },
    },
    {
        id: 'catppuccin-mocha',
        name: 'Catppuccin Mocha',
        variant: 'dark',
        tokens: {
            accent: '#cba6f7',
            accentGradientTo: '#f5c2e7', // Catppuccin Pink
            bgOne: '#1e1e2e',
            bgTwo: '#313244',
            borderOne: '#45475a',
            textColor: '#cdd6f4',
            textColorSecondary: '#a6adc8',
            success: '#a6e3a1',
            warning: '#f9e2af',
            error: '#f38ba8',
            info: '#89b4fa',
        },
    },
    {
        id: 'catppuccin-latte',
        name: 'Catppuccin Latte',
        variant: 'light',
        tokens: {
            accent: '#8839ef',
            accentGradientTo: '#c95aab', // Catppuccin Pink, nudged from #ea76cb
            bgOne: '#eff1f5',
            bgTwo: '#e6e9ef',
            borderOne: '#ccd0da',
            textColor: '#4c4f69',
            textColorSecondary: '#6c6f85',
            success: '#2f7d20', // nudged from #40a02b
            warning: '#9d6510', // nudged from #df8e1d
            error: '#d20f39',
            info: '#1e66f5',
        },
    },
    {
        id: 'tokyo-night',
        name: 'Tokyo Night',
        variant: 'dark',
        tokens: {
            accent: '#7aa2f7',
            accentGradientTo: '#bb9af7', // Tokyo Night purple
            bgOne: '#1a1b26',
            bgTwo: '#24283b',
            borderOne: '#3b4261',
            textColor: '#c0caf5',
            textColorSecondary: '#9aa5ce',
            success: '#9ece6a',
            warning: '#e0af68',
            error: '#f7768e',
            info: '#7dcfff',
        },
    },
    {
        id: 'tokyo-night-day',
        name: 'Tokyo Night Day',
        variant: 'light',
        tokens: {
            accent: '#1e63c8', // nudged from #2e7de9
            accentGradientTo: '#8a46e0', // Tokyo Night purple, nudged from #9854f1
            bgOne: '#e1e2e7',
            bgTwo: '#d4d6e0',
            borderOne: '#a8aecb',
            textColor: '#343b58',
            textColorSecondary: '#5a638c',
            success: '#587539',
            warning: '#8c6c3e',
            error: '#c64343',
            info: '#1e63c8', // nudged from #2e7de9
        },
    },
    {
        id: 'monokai',
        name: 'Monokai',
        variant: 'dark',
        tokens: {
            accent: '#66d9ef',
            accentGradientTo: '#ae81ff', // Monokai purple
            bgOne: '#272822',
            bgTwo: '#32332c',
            borderOne: '#49483e',
            textColor: '#f8f8f2',
            textColorSecondary: '#a59f85',
            success: '#a6e22e',
            warning: '#e6db74',
            error: '#f92672',
            info: '#66d9ef',
        },
    },
    {
        id: 'nord',
        name: 'Nord',
        variant: 'dark',
        tokens: {
            accent: '#88c0d0',
            accentGradientTo: '#b48ead', // Nord aurora purple
            bgOne: '#2e3440',
            bgTwo: '#3b4252',
            borderOne: '#4c566a',
            textColor: '#eceff4',
            textColorSecondary: '#aebacf',
            success: '#a3be8c',
            warning: '#ebcb8b',
            error: '#d08a92', // nudged from #bf616a
            info: '#88c0d0',
        },
    },
    {
        id: 'solarized-dark',
        name: 'Solarized Dark',
        variant: 'dark',
        tokens: {
            accent: '#268bd2',
            accentGradientTo: '#7479ce', // Solarized violet, nudged from #6c71c4
            bgOne: '#002b36',
            bgTwo: '#073642',
            borderOne: '#586e75',
            textColor: '#93a1a1',
            textColorSecondary: '#7b8f8f',
            success: '#859900',
            warning: '#b58900',
            error: '#ea5c58', // nudged from #dc322f
            info: '#2aa198',
        },
    },
    {
        id: 'solarized-light',
        name: 'Solarized Light',
        variant: 'light',
        tokens: {
            accent: '#1c6a9e', // nudged from #268bd2
            accentGradientTo: '#6c71c4', // Solarized violet
            bgOne: '#fdf6e3',
            bgTwo: '#eee8d5',
            borderOne: '#c9c2ab',
            textColor: '#586e75',
            textColorSecondary: '#657b83',
            success: '#6b7c00', // nudged from #859900
            warning: '#96710a', // nudged from #b58900
            error: '#dc322f',
            info: '#218a82', // nudged from #2aa198
        },
    },
    {
        id: 'hackerman',
        name: 'Hackerman',
        variant: 'dark',
        tokens: {
            accent: '#82fb9c',
            accentGradientTo: '#7cf8f7', // Hackerman cyan
            bgOne: '#0b0c16',
            bgTwo: '#141626', // no second surface upstream, lifted toward color0
            borderOne: '#3e4058',
            textColor: '#ddf7ff',
            textColorSecondary: '#829dd4',
            success: '#4fe88f',
            warning: '#f7d94f', // upstream runs green through blue, with no warm hue
            error: '#ff6b81', // as above
            info: '#85e1fb',
        },
    },
    {
        id: 'solitude',
        name: 'Solitude',
        variant: 'dark',
        tokens: {
            accent: '#798186',
            accentGradientTo: '#cacccc', // far end of the upstream active border
            bgOne: '#080a0b',
            bgTwo: '#101315',
            borderOne: '#343d41',
            textColor: '#cacccc',
            textColorSecondary: '#9a9a9a',
            success: '#8fae8f', // upstream is monochrome apart from its red
            warning: '#c9a227', // as above
            error: '#de6145',
            info: '#7f9bb3', // as above
        },
    },
    {
        id: 'project-felt-light',
        name: 'Project Felt Light',
        variant: 'light',
        tokens: PROJECT_FELT_THEME_TOKENS.light,
    },
    {
        id: 'project-felt-light-high-contrast',
        name: 'Project Felt Light (High Contrast)',
        variant: 'light',
        tokens: PROJECT_FELT_THEME_TOKENS.lightHighContrast,
    },
    {
        id: 'project-felt-dark',
        name: 'Project Felt Dark',
        variant: 'dark',
        tokens: PROJECT_FELT_THEME_TOKENS.dark,
    },
    {
        id: 'project-felt-dark-high-contrast',
        name: 'Project Felt Dark (High Contrast)',
        variant: 'dark',
        tokens: PROJECT_FELT_THEME_TOKENS.darkHighContrast,
    },
]

const THEMES_BY_ID = new Map(INSTALLED_THEMES.map((theme) => [theme.id, theme]))

export const installedTheme = (id: string): InstalledTheme | undefined => THEMES_BY_ID.get(id)

/**
 * Hover tint. A dark theme lifts the surface with white, a light theme deepens
 * it with black, matching what the compiled themes do. Only the tint color is a
 * variable: tailwind bakes the 0.05 alpha into the utility at build time.
 */
export const surfaceHoverFor = (theme: InstalledTheme): string =>
    theme.variant === 'dark' ? '255 255 255' : '0 0 0'

/** `#rrggbb` to the `r g b` triplet the `--colors-*` variables carry. */
export const hexToTriplet = (hex: string): string =>
    [
        Number.parseInt(hex.slice(1, 3), 16),
        Number.parseInt(hex.slice(3, 5), 16),
        Number.parseInt(hex.slice(5, 7), 16),
    ].join(' ')

/**
 * The variable each token drives. Custom themes carry the same keys, so both
 * paths apply a theme through this one map.
 *
 * This order is the theme-code wire order (see `shell/themeCode.ts`): a `cct1`
 * code holds the first six entries. Append new tokens, never insert, or every
 * shared code decodes into the wrong colors. `themeCode.spec.ts` locks it.
 */
export const THEME_TOKEN_VARS: Record<keyof ThemeTokens, string> = {
    accent: '--colors-accent',
    bgOne: '--colors-bg-one',
    bgTwo: '--colors-bg-two',
    borderOne: '--colors-border-one',
    textColor: '--colors-text-color',
    textColorSecondary: '--colors-text-color-secondary',
    success: '--colors-success',
    warning: '--colors-warning',
    error: '--colors-error',
    info: '--colors-info',
    accentGradientTo: '--colors-accent-gradient-to',
}

export const THEME_TOKEN_KEYS = Object.keys(THEME_TOKEN_VARS) as Array<keyof ThemeTokens>

/** Every `--colors-*` variable an installed theme sets, including the derived hover tint. */
export const themeCssVars = (theme: InstalledTheme): Array<[string, string]> => [
    ...THEME_TOKEN_KEYS.map((token): [string, string] => [
        THEME_TOKEN_VARS[token],
        hexToTriplet(theme.tokens[token]),
    ]),
    ['--colors-surface-hover', surfaceHoverFor(theme)],
]

/** The variables a theme switch has to clear before the next theme is applied. */
export const THEME_CSS_VAR_NAMES: string[] = [
    ...Object.values(THEME_TOKEN_VARS),
    '--colors-surface-hover',
]

/** The id the System theme applies its palette under. Never persisted: the theme
 * mode stays `system`, and the palette behind it is whatever the desktop says today. */
export const SYSTEM_THEME_ID = 'system'

/**
 * The desktop's own colors, as the Qt app reads them. A browser tab has none, and
 * neither does a desktop that publishes nothing.
 *
 * `tokens` arrives all or nothing. KDE writes its resolved palette to a file any
 * process can read, so the whole set is available there; everywhere else only an
 * accent and a light/dark preference are, because the rest lives inside GTK.
 */
export interface SystemPalette {
    variant?: 'dark' | 'light'
    accent?: string
    tokens?: ThemeTokens
}

const HEX_COLOR = /^#[0-9a-f]{6}$/

/**
 * Parses what the Qt app sends. Anything malformed is dropped rather than let
 * through to a CSS variable, since this is the one palette the UI does not ship
 * and cannot check at build time. Null when nothing usable survives.
 */
export const parseSystemPalette = (json: string): SystemPalette | null => {
    let raw: unknown
    try {
        raw = JSON.parse(json)
    } catch {
        return null
    }
    if (raw == null || typeof raw !== 'object') return null
    const source = raw as Record<string, unknown>
    const palette: SystemPalette = {}
    if (source.variant === 'dark' || source.variant === 'light') {
        palette.variant = source.variant
    }
    if (typeof source.accent === 'string' && HEX_COLOR.test(source.accent)) {
        palette.accent = source.accent
    }
    if (source.tokens != null && typeof source.tokens === 'object') {
        const candidate = source.tokens as Record<string, unknown>
        // A half-filled palette would draw desktop colors over ours, which reads
        // worse than either on its own, so a single bad token drops the set.
        const complete = THEME_TOKEN_KEYS.every(
            (key) => typeof candidate[key] === 'string' && HEX_COLOR.test(candidate[key] as string),
        )
        if (complete) {
            palette.tokens = Object.fromEntries(
                THEME_TOKEN_KEYS.map((key) => [key, candidate[key]]),
            ) as unknown as ThemeTokens
        }
    }
    return Object.keys(palette).length > 0 ? palette : null
}

/**
 * The hover tint for an arbitrary background, used by custom themes where the
 * variant is not declared: a light background darkens, a dark one lightens.
 * Accepts either `#rrggbb` or the `r g b` form the settings store holds.
 */
export const surfaceTintFor = (background: string): string => {
    const [r, g, b] = background.startsWith('#')
        ? [1, 3, 5].map((at) => Number.parseInt(background.slice(at, at + 2), 16))
        : background.split(/[\s,]+/).map((part) => Number.parseInt(part, 10))
    const channel = (c: number): number => {
        const s = (Number.isFinite(c) ? c : 0) / 255
        return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
    }
    const luminance = 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    return luminance > 0.4 ? '0 0 0' : '255 255 255'
}

/** The four colors a picker preview shows: page, card, accent, text. */
export type ThemeSwatch = [string, string, string, string]

export const swatchFor = (theme: InstalledTheme): ThemeSwatch => [
    theme.tokens.bgOne,
    theme.tokens.bgTwo,
    theme.tokens.accent,
    theme.tokens.textColor,
]

/**
 * The same preview for a compiled theme, read back from its own class rather
 * than duplicated here, so the picker cannot drift from tailwind.config.js.
 */
export const probeCompiledSwatch = (className: string): ThemeSwatch => {
    const probe = document.createElement('div')
    probe.className = className
    probe.style.cssText = 'position:absolute;visibility:hidden;pointer-events:none'
    document.body.appendChild(probe)
    const style = getComputedStyle(probe)
    const read = (name: string): string => `rgb(${style.getPropertyValue(name).trim()})`
    const swatch: ThemeSwatch = [
        read('--colors-bg-one'),
        read('--colors-bg-two'),
        read('--colors-accent'),
        read('--colors-text-color'),
    ]
    probe.remove()
    return swatch
}

export const getCockpitSystemPalette = (): SystemPalette => {
    const prefersDark = !window.matchMedia('(prefers-color-scheme: light)').matches
    const prefersContrast = window.matchMedia('(prefers-contrast: more)').matches
    return {
        variant: prefersDark ? 'dark' : 'light',
        tokens: prefersDark
        ? prefersContrast
            ? PROJECT_FELT_THEME_TOKENS.darkHighContrast
            : PROJECT_FELT_THEME_TOKENS.dark
        : prefersContrast
          ? PROJECT_FELT_THEME_TOKENS.lightHighContrast
          : PROJECT_FELT_THEME_TOKENS.light
    }
}
