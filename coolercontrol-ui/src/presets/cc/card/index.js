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
            //Shape
            'rounded-md',
            'shadow-md',

            //Color
            'bg-surface-0 dark:bg-surface-900',
            'text-surface-700 dark:text-surface-0',
        ],
    },
    body: {
        class: 'p-5',
    },
    title: {
        class: 'text-2xl font-bold mb-2',
    },
    subtitle: {
        class: [
            //Font
            'font-normal',

            //Spacing
            'mb-2',

            //Color
            'text-surface-600 dark:text-surface-0/60',
        ],
    },
    content: {
        class: 'py-5', // Vertical padding.
    },
    footer: {
        class: 'pt-5', // Top padding.
    },
}
