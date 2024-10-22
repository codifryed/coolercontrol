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

/** @type {import('tailwindcss').Config} */
const primeui = require('tailwindcss-primeui')
const themer = require('tailwindcss-themer')
const colors = require('tailwindcss/colors')

module.exports = {
    content: ['./index.html', './src/**/*.{vue,js,ts,jsx,tsx}'],
    darkMode: 'selector',
    plugins: [
        primeui,
        themer({
            defaultTheme: {
                extend: {
                    colors: {
                        // setting a 'default' color for all themes seems not to work
                        // accent: "#568af2",
                    },
                    'primary-50': 'rgb(var(--primary-50))',
                    'primary-100': 'rgb(var(--primary-100))',
                    'primary-200': 'rgb(var(--primary-200))',
                    'primary-300': 'rgb(var(--primary-300))',
                    'primary-400': 'rgb(var(--primary-400))',
                    'primary-500': 'rgb(var(--primary-500))',
                    'primary-600': 'rgb(var(--primary-600))',
                    'primary-700': 'rgb(var(--primary-700))',
                    'primary-800': 'rgb(var(--primary-800))',
                    'primary-900': 'rgb(var(--primary-900))',
                    'primary-950': 'rgb(var(--primary-950))',
                    'surface-0': 'rgb(var(--surface-0))',
                    'surface-50': 'rgb(var(--surface-50))',
                    'surface-100': 'rgb(var(--surface-100))',
                    'surface-200': 'rgb(var(--surface-200))',
                    'surface-300': 'rgb(var(--surface-300))',
                    'surface-400': 'rgb(var(--surface-400))',
                    'surface-500': 'rgb(var(--surface-500))',
                    'surface-600': 'rgb(var(--surface-600))',
                    'surface-700': 'rgb(var(--surface-700))',
                    'surface-800': 'rgb(var(--surface-800))',
                    'surface-900': 'rgb(var(--surface-900))',
                    'surface-950': 'rgb(var(--surface-950))',
                },
            },
            themes: [
                // If two themes are applied at the same time, order here takes precedence
                {
                    name: 'system-theme',
                    selectors: ['.system-theme'],
                    mediaQuery: '@media (forced-colors: active)',
                    extend: {
                        colors: {
                            primary: 'blue',
                            accent: 'AccentColor',
                            'accent-text': 'AccentColorText',
                            'active-text': 'ActiveText',
                            'button-border': 'ButtonBorder',
                            'button-face': 'ButtonFace',
                            'button-text': 'ButtonText',
                            canvas: 'Canvas',
                            'canvas-text': 'CanvasText',
                            field: 'Field',
                            'field-text': 'FieldText',
                            'gray-text': 'GrayText',
                            highlight: 'Highlight',
                            'highlight-text': 'HighlightText',
                            'link-text': 'LinkText',
                            mark: 'Mark',
                            'mark-text': 'MarkText',
                            'visited-text': 'VisitedText',
                        },
                    },
                },
                {
                    name: 'high-contrast-dark',
                    selectors: ['.high-contrast-dark'],
                    mediaQuery: '@media (prefers-color-scheme: dark) and (prefers-contrast: more)',
                    extend: {
                        colors: {
                            primary: 'blue',
                        },
                    },
                },
                {
                    name: 'high-contrast-light',
                    selectors: ['.high-contrast-light'],
                    mediaQuery: '@media (prefers-color-scheme: light) and (prefers-contrast: more)',
                    extend: {
                        colors: {
                            primary: 'blue',
                        },
                    },
                },
                {
                    name: 'dark-theme',
                    selectors: ['.dark-theme'],
                    mediaQuery: '@media (prefers-color-scheme: dark)',
                    extend: {
                        colors: {
                            // Going to try to minimize the number of colors:
                            accent: '#568af2',
                            'bg-one': '#1b1e23',
                            'bg-two': '#2c313c',
                            'border-one': '#343b48',
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
                            //--------------------------------
                            'dark-one': '#1b1e23',
                            'dark-four': '#272c36',
                            // 'bg-one': '#272c36',
                            'bg-three': '#343b48',
                            'context-color': '#568af2',
                            'context-hover': '#6c99f4',
                            'context-pressed': '#3f6fd1',
                            'surface-a': '#0c1a36',
                            'surface-c': '#10294c',
                            'surface-e': '#0c1a36',
                            'surface-f': '#0c1a36',
                            'primary-color': 'var(--cc-context-color)',
                            'primary-color-text': '#1c2127',
                            'primary-color-text-background': '#3875f0',
                            'surface-0': '#040d19',
                            'surface-50': '#1d2530',
                            'surface-100': '#363d47',
                            'surface-200': '#4f565e',
                            'surface-300': '#686e75',
                            'surface-400': '#82868c',
                            'surface-500': '#9b9ea3',
                            'surface-600': '#b4b6ba',
                            'surface-700': '#cdcfd1',
                            'surface-800': '#e6e7e8',
                            'surface-900': '#ffffff',
                            'gray-50': '#e6e7e8',
                            'gray-100': '#cdcfd1',
                            'gray-200': '#b4b6ba',
                            'gray-300': '#9b9ea3',
                            'gray-400': '#82868c',
                            'gray-500': '#686e75',
                            'gray-600': '#4f565e',
                            'gray-700': '#363d47',
                            'gray-800': '#1d2530',
                            'gray-900': '#040d19',
                            'surface-ground': '#272c36',
                            'surface-section': '#040d19',
                            'surface-card': '#2c313c',
                            'surface-overlay': '#1b1e23',
                            'surface-border': '#343b48',
                            // 'focus-ring': 0 0 0 0.2rem rgba(165, 180, 252, 0.5)',
                            maskbg: 'rgba(0, 0, 0, 0.4)',
                            'highlight-bg': 'rgba(165, 180, 252, 0.16)',
                            'highlight-text-color': 'rgba(255, 255, 255, 0.87)',
                            'border-blue': '#6c99f4',
                            'box-shadow-1': '#a5b4fc',
                            'box-shadow-2': '#dbe2ea',
                            'input-background': '#233752',
                            'input-background-2': '#477ff1',
                            'split-button-color': '#cbd5e1',
                            'password-weak': '#eb9a9c',
                            'password-medium': '#ffcf91',
                            'password-strong': '#93deac',
                            'rating-cancel': '#f48fb1',
                            'button-enable-background': '#95a9c2',
                            'datatable-striped-background': '#0d1a2c',
                        },
                    },
                },
                {
                    name: 'light-theme',
                    selectors: ['.light-theme'],
                    mediaQuery: '@media (prefers-color-scheme: light)',
                    extend: {
                        colors: {
                            primary: 'blue',
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
            green: '#00ff7f',
            red: '#ff5555',
            yellow: '#f1fa8c',
            blue: '#568af2',
            gray: colors.zinc,
            purple: colors.purple,
        },
        // extend: {
        //     colors: {},
        // },
    },
}
