/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2023  Guy Boldon
 * |
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 * |
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 * |
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

import { defineStore } from 'pinia'
import { ref, type Ref } from 'vue'

/**
 * This store offers programmatic access to our color themes.
 */
export const useThemeColorsStore = defineStore('theme-colors', () => {
    const chosenColorScheme: Ref<string> = ref('dark')
    const colors = ref({
        dark: {
            dark_one: '#1b1e23',
            dark_two: '#1e2229',
            dark_three: '#21252d',
            dark_four: '#272c36',
            bg_one: '#272c36',
            bg_two: '#2c313c',
            bg_three: '#343b48',
            icon_color: '#c3ccdf',
            icon_hover: '#dce1ec',
            icon_pressed: '#6c99f4',
            icon_active: '#f5f6f9',
            context_color: '#568af2',
            context_hover: '#6c99f4',
            context_pressed: '#3f6fd1',
            text_title: '#dce1ec',
            text_foreground: '#8a95aa',
            text_description: '#4f5b6e',
            text_active: '#dce1ec',
            white: '#f5f6f9',
            pink: '#ff007f',
            green: '#00ff7f',
            red: '#ff5555',
            yellow: '#f1fa8c',
        },
        light: {
            dark_one: '#1b1e23',
            dark_two: '#1e2229',
            dark_three: '#21252d',
            dark_four: '#272c36',
            bg_one: '#D3E0F7',
            bg_two: '#E2E9F7',
            bg_three: '#EFF1F7',
            icon_color: '#6C7C96',
            icon_hover: '#8CB8FF',
            icon_pressed: '#6c99f4',
            icon_active: '#8CB8FF',
            context_color: '#568af2',
            context_hover: '#6c99f4',
            context_pressed: '#4B5469',
            text_title: '#606C85',
            text_foreground: '#6B7894',
            text_description: '#7887A6',
            text_active: '#8797BA',
            white: '#f5f6f9',
            pink: '#ff007f',
            green: '#00ff7f',
            red: '#ff5555',
            yellow: '#f1fa8c',
        },
    })

    function themeColors() {
        if (chosenColorScheme.value == 'dark') {
            return colors.value.dark
        } else {
            return colors.value.light
        }
    }

    function setDarkColorScheme(): void {
        chosenColorScheme.value = 'dark'
    }

    function setLightColorScheme(): void {
        chosenColorScheme.value = 'light'
    }

    console.debug(`Theme Colors Store created`)
    return { themeColors, setDarkColorScheme, setLightColorScheme }
})
