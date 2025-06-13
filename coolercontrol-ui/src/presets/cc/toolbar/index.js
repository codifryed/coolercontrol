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
            // Flex & Alignment
            'flex items-center justify-between flex-wrap',
            'gap-2',

            // Spacing
            'p-5',

            // Shape
            'rounded-md',

            // Color
            'bg-surface-50 dark:bg-surface-800',
            'border border-surface-200 dark:border-surface-700',
        ],
    },
    start: {
        class: 'flex items-center',
    },
    center: {
        class: 'flex items-center',
    },
    end: {
        class: 'flex items-center',
    },
}
