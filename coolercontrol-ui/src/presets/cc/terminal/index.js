/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon and contributors
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

export default {
    root: {
        class: [
            // Spacing
            'p-5',

            // Shape
            'rounded-md',

            // Color
            'bg-surface-900 text-white',
            'border border-surface-700',

            // Sizing & Overflow
            'h-72 overflow-auto',
        ],
    },
    container: {
        class: [
            // Flexbox
            'flex items-center',
        ],
    },
    prompt: {
        class: [
            // Color
            'text-surface-400',
        ],
    },
    response: {
        class: [
            // Color
            'text-primary-400',
        ],
    },
    command: {
        class: [
            // Color
            'text-primary-400',
        ],
    },
    commandtext: {
        class: [
            // Flexbox
            'flex-1 shrink grow-0',

            // Shape
            'border-0',

            // Spacing
            'p-0',

            // Color
            'bg-transparent text-inherit',

            // Outline
            'outline-none',
        ],
    },
}
