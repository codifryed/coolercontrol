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
            // Flexbox
            'inline-flex items-center',

            // Spacing
            'px-3 gap-2',

            // Shape
            'rounded-[16px]',

            // Colors
            'text-surface-700 dark:text-white/70',
            'bg-surface-200 dark:bg-surface-700',
        ],
    },
    label: {
        class: 'leading-6 my-1.5 mx-0',
    },
    icon: {
        class: 'leading-6 mr-2',
    },
    image: {
        class: ['w-9 h-9 -ml-3 mr-2', 'rounded-full'],
    },
    removeIcon: {
        class: [
            // Shape
            'rounded-md leading-6',

            // Size
            'w-4 h-4',

            // Transition
            'transition duration-200 ease-in-out',

            // Misc
            'cursor-pointer',
        ],
    },
}
