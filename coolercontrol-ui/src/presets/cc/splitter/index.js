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
    root: ({ context }) => ({
        class: [
            // Colors
            'bg-surface-ground',
            // 'bg-gray-700',
            // 'text-white',
            // 'bg-surface-0',
            // 'dark:bg-surface-900',
            // 'text-surface-700',
            // 'dark:text-surface-0/80',

            // Shape
            // 'rounded-lg',

            // Borders (Conditional)
            // { 'border border-solid border-surface-50 dark:border-surface-700': !context.nested },

            // Nested
            { 'flex grow border-0': context.nested },
        ],
    }),

    gutter: ({ props }) => ({
        class: [
            // Flexbox
            'flex',
            'items-center',
            'justify-center',
            'shrink-0',

            // Colors
            'bg-surface-ground',
            // 'bg-surface-50',
            // 'dark:bg-surface-800',

            // Transitions
            'transition-all',
            'duration-200',

            // Misc
            {
                'cursor-col-resize': props.layout == 'horizontal',
                'cursor-row-resize': props.layout !== 'horizontal',
            },
        ],
    }),
    gutterhandler: ({ props }) => ({
        class: [
            // Colors
            'bg-surface-100',
            'dark:bg-surface-600',

            // Transitions
            'transition-all',
            'duration-200',

            // Sizing (Conditional)
            {
                'h-7': props.layout == 'horizontal',
                'w-7 h-2': props.layout !== 'horizontal',
            },
        ],
    }),
}
