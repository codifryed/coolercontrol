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

import { defineStore } from 'pinia'
import { ref } from 'vue'

/**
 * This store offers programmatic access to our color themes.
 */
export const useThemeColorsStore = defineStore('theme-colors', () => {
    const cssRoot = document.querySelector(':root')
    const getStyle = (varName: string) => getComputedStyle(cssRoot!).getPropertyValue(varName)
    const themeColors = ref({
        dark_one: getStyle('--cc-dark-one'),
        dark_four: getStyle('--cc-dark-four'),
        bg_one: getStyle('--cc-bg-one'),
        bg_two: getStyle('--cc-bg-two'),
        bg_three: getStyle('--cc-bg-three'),
        context_color: getStyle('--cc-context-color'),
        context_hover: getStyle('--cc-context-hover'),
        context_pressed: getStyle('--cc-context-pressed'),
        text_color: getStyle('--text-color'),
        text_color_secondary: getStyle('--text-color-secondary'),
        white: getStyle('--cc-white'),
        pink: getStyle('--cc-ping'),
        green: getStyle('--cc-green'),
        red: getStyle('--cc-red'),
        yellow: getStyle('--cc-yellow'),
        gray_600: getStyle('--gray-600'),
        surface_card: getStyle('--surface-card'),
        accent: getStyle('--cc-accent'),
        primary: getStyle('--primary-color'),
    })
    console.debug(`Theme Colors Store created`)
    return { themeColors }
})
