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
    root: ({ props }) => ({
        class: [
            // Positioning
            'absolute z-1',
            {
                'left-0 bottom-0 w-full': props.position == 'bottom',
                'left-0 top-0 w-full': props.position == 'top',
                'left-0 top-0 h-full': props.position == 'left',
                'right-0 top-0 h-full': props.position == 'right',
            },

            // Flexbox & Alignment
            'flex justify-center items-center',

            // Interactivity
            'pointer-events-none',
        ],
    }),
    listContainer: {
        class: [
            // Flexbox
            'flex',

            // Shape & Border
            'rounded-md',

            // Color
            'bg-surface-0/10 dark:bg-surface-900/20 border border-surface-0/20',
            'backdrop-blur-sm',

            // Spacing
            'p-2',

            // Misc
            'pointer-events-auto',
        ],
    },
    list: ({ props }) => ({
        class: [
            // Flexbox & Alignment
            'flex items-center justify-center',
            {
                'flex-col': props.position == 'left' || props.position == 'right',
            },

            // List Style
            'm-0 p-0 list-none',

            // Shape
            'outline-none',
        ],
    }),
    item: ({ props, context, instance }) => ({
        class: [
            // Spacing & Shape
            'p-2 rounded-md',

            // Positioning & Hover States
            {
                'origin-bottom': props.position == 'bottom',
                'origin-top': props.position == 'top',
                'origin-left': props.position == 'left',
                'origin-right': props.position == 'right',
            },

            // Transitions & Transform
            'transition-all duration-200 ease-cubic-bezier-will-change-transform transform',
        ],
    }),
    itemLink: {
        class: [
            // Flexbox & Alignment
            'flex flex-col items-center justify-center',

            // Position
            'relative',

            // Size
            'w-16 h-16',

            // Misc
            'cursor-default overflow-hidden',
        ],
    },
}
