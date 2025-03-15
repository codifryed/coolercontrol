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
    const getStyle = (varName: string): string =>
        `rgb(${getComputedStyle(cssRoot!).getPropertyValue(varName)})`
    const themeColors = ref({
        accent: getStyle('--colors-accent'),
        bg_one: getStyle('--colors-bg-one'),
        bg_two: getStyle('--colors-bg-two'),
        border: getStyle('--colors-border-one'),
        text_color: getStyle('--colors-text-color'),
        text_color_secondary: getStyle('--colors-text-color-secondary'),
        white: getStyle('--colors-white'),
        pink: getStyle('--colors-pink'),
        green: getStyle('--colors-success'),
        red: getStyle('--colors-error'),
        yellow: getStyle('--colors-warning'),
        info: getStyle('--colors-info'),
    })

    function hexToRgb(hex: string): Array<number> {
        return hex
            .replace(
                /^#?([a-f\d])([a-f\d])([a-f\d])$/i,
                (_m, r, g, b) => '#' + r + r + g + g + b + b,
            )!
            .substring(1)
            .match(/.{2}/g)!
            .map((x) => parseInt(x, 16))
    }
    function convertColorToRGBA(color: string, opacity: number): string {
        if (color.includes('#')) {
            const rgbArray = hexToRgb(color)
            return `rgba(${rgbArray[0]}, ${rgbArray[1]}, ${rgbArray[2]}, ${opacity})`
        } else if (color.includes(',')) {
            return color.replace(')', `, ${opacity})`).replace('rgb', 'rgba')
        } else {
            return color.replace(')', ` / ${opacity})`).replace('rgb', 'rgba')
        }
    }

    console.debug(`Theme Colors Store created`)
    return { themeColors, hexToRgb, convertColorToRGBA }
})
