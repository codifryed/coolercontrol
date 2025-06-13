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
    display: {
        class: [
            // Display
            'inline',

            // Spacing
            'p-3',

            // Shape
            'rounded-md',

            // Colors
            'text-surface-700 dark:text-white/80',

            // States
            'hover:bg-surface-100 hover:text-surface-700 dark:hover:bg-surface-700/80 dark:hover:text-white/80',

            // Transitions
            'transition',
            'duration-200',

            // Misc
            'cursor-pointer',
        ],
    },
}
