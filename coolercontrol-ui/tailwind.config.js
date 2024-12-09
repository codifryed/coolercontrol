/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

const colors = require('tailwindcss/colors')

module.exports = {
    content: ['./index.html', './src/**/*.{vue,js,ts,jsx,tsx}'],
    darkMode: 'selector',
    plugins: [
        require('tailwindcss-primeui'),
        require('tailwindcss-themer')({
            defaultTheme: {
                extend: {
                    // the default theme that appears for a split second on load:
                    colors: {
                        // dark-theme copy
                        accent: '#568af2',
                        'bg-one': '#1b1e23',
                        'bg-two': '#2c313c',
                        'border-one': '#8a95aa40',
                        'surface-hover': 'rgba(255, 255, 255, 0.05)',
                        'text-color': '#dce1ec',
                        'text-color-secondary': '#8a95aa',
                        white: '#f5f6f9',
                        pink: '#ff007f',
                        green: '#00ff7f',
                        success: '#00ff7f',
                        red: '#ff5555',
                        error: '#ff5555',
                        yellow: '#f1fa8c',
                        warning: '#f1fa8c',
                        blue: '#568af2',
                        info: '#568af2',
                    },
                },
            },
            themes: [
                // The last applicable theme here takes precedence.
                {
                    // this theme is not currently used and considered Alpha
                    // It still doesn't properly pull the color palette from the
                    // desktop environment and is inconsistent.
                    name: 'system-theme',
                    selectors: ['.system-theme'],
                    // mediaQuery: '@media (forced-colors: active)',
                    extend: {
                        colors: {
                            accent: 'AccentColor',
                            // accent: 'Highlight',
                            'bg-one': 'Canvas',
                            'bg-two': 'ButtonFace',
                            'border-one': 'ButtonBorder',
                            'surface-hover': 'rgba(255, 255, 255, 0.05)',
                            'text-color': 'CanvasText',
                            'text-color-secondary': 'GreyText',
                            white: '#f5f6f9',
                            pink: '#ff007f',
                            green: '#00ff7f',
                            success: '#00ff7f',
                            red: '#ff5555',
                            error: '#ff5555',
                            yellow: '#f1fa8c',
                            warning: '#f1fa8c',
                            blue: '#568af2',
                            info: '#568af2',
                            // additional possible:
                            // 'accent-text': 'AccentColorText',
                            // 'active-text': 'ActiveText',
                            // 'button-face': 'ButtonFace',
                            // 'button-text': 'ButtonText',
                            // 'canvas-text': 'CanvasText',
                            // field: 'Field',
                            // 'field-text': 'FieldText',
                            // 'gray-text': 'GrayText',
                            // highlight: 'Highlight',
                            // 'highlight-text': 'HighlightText',
                            // 'link-text': 'LinkText',
                            // mark: 'Mark',
                            // 'mark-text': 'MarkText',
                            // 'visited-text': 'VisitedText',
                        },
                    },
                },
                {
                    name: 'high-contrast-dark',
                    selectors: ['.high-contrast-dark'],
                    // mediaQuery: '@media (prefers-color-scheme: dark) and (prefers-contrast: more)',
                    extend: {
                        colors: {
                            accent: '#00ff00',
                            'bg-one': '#000000',
                            'bg-two': '#000000',
                            'border-one': '#ffffffff',
                            'surface-hover': 'rgba(255, 255, 255, 0.25)',
                            'text-color': '#ffffff',
                            'text-color-secondary': '#ffffff',
                            white: '#ffffff',
                            pink: '#ff00ff',
                            green: '#00ff00',
                            success: '#00ff00',
                            red: '#ff0000',
                            error: '#ff0000',
                            yellow: '#ffff00',
                            warning: '#ffff00',
                            blue: '#00a0ff',
                            info: '#00a0ff',
                        },
                    },
                },
                {
                    name: 'high-contrast-light',
                    selectors: ['.high-contrast-light'],
                    // mediaQuery: '@media (prefers-color-scheme: light) and (prefers-contrast: more)',
                    extend: {
                        colors: {
                            accent: '#0000ff',
                            'bg-one': '#ffffff',
                            'bg-two': '#ffffff',
                            'border-one': '#000000ff',
                            'surface-hover': 'rgba(0, 0, 0, 0.25)',
                            'text-color': '#000000',
                            'text-color-secondary': '#000000',
                            white: '#ffffff',
                            pink: '#ff00ff',
                            green: '#00ff00',
                            success: '#00ff00',
                            red: '#ff0000',
                            error: '#ff0000',
                            yellow: '#ffff00',
                            warning: '#ffff00',
                            blue: '#0000ff',
                            info: '#0000ff',
                        },
                    },
                },
                {
                    name: 'light-theme',
                    selectors: ['.light-theme'],
                    // mediaQuery: '@media (prefers-color-scheme: light)',
                    extend: {
                        colors: {
                            accent: '#568af2',
                            'bg-one': '#f5f6f9',
                            'bg-two': '#d3e0f7',
                            'border-one': '#6c757d40',
                            'surface-hover': 'rgba(0, 0, 0, 0.05)',
                            'text-color': '#495057',
                            'text-color-secondary': '#6c757d',
                            white: '#f5f6f9',
                            pink: '#ff007f',
                            green: '#00ff7f',
                            success: '#00ff7f',
                            red: '#ff5555',
                            error: '#ff5555',
                            yellow: '#f1fa8c',
                            warning: '#f1fa8c',
                            blue: '#568af2',
                            info: '#568af2',
                        },
                    },
                },
                {
                    name: 'dark-theme',
                    selectors: ['.dark-theme'],
                    // These don't work as expected for our setup. Left for reference. (SettingsStore)
                    // mediaQuery: '@media (prefers-color-scheme: dark)',
                    extend: {
                        colors: {
                            accent: '#568af2',
                            'bg-one': '#1b1e23',
                            'bg-two': '#2c313c',
                            // 'border-one': '#343b48',
                            'border-one': '#8a95aa40',
                            'surface-hover': 'rgba(255, 255, 255, 0.05)',
                            'text-color': '#dce1ec',
                            'text-color-secondary': '#8a95aa',
                            white: '#f5f6f9',
                            pink: '#ff007f',
                            green: '#00ff7f',
                            success: '#00ff7f',
                            red: '#ff5555',
                            error: '#ff5555',
                            yellow: '#f1fa8c',
                            warning: '#f1fa8c',
                            blue: '#568af2',
                            info: '#568af2',
                        },
                    },
                },
            ],
        }),
    ],
    theme: {
        colors: {
            // The base Tailwind color set
            transparent: 'transparent',
            current: 'currentColor',
            black: colors.black,
            white: '#f5f6f9',
            pink: '#ff007f',
            success: '#00ff7f',
            error: '#ff5555',
            warning: '#f1fa8c',
            info: '#568af2',
            gray: colors.zinc,
            purple: colors.purple,
        },
        // This seems to confict with our setup. Don't use:
        // extend: {
        //     colors: {},
        // },
    },
}
