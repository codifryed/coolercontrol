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
    content: {
        class: [
            // Spacing
            'p-0',

            // Shape
            'border-0',

            // Color
            'text-surface-700 dark:text-white/80',
            'bg-surface-0 dark:bg-surface-800',
        ],
    },
    header: {
        class: [
            'font-bold',

            // Spacing
            'p-4',

            // Color
            'text-surface-800 dark:text-white/80',
            'bg-surface-50 dark:bg-surface-800',
            'border-surface-200 dark:border-surface-700 border-y',
        ],
    },
}
