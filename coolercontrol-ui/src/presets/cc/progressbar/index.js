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
            // Position and Overflow
            'overflow-hidden',
            'relative',

            // Shape and Size
            'border-0',
            'h-6',
            'rounded-md',

            // Colors
            'bg-surface-100 dark:bg-surface-700',
        ],
    },
    value: ({ props }) => ({
        class: [
            // Flexbox & Overflow & Position
            {
                'absolute flex items-center justify-center overflow-hidden':
                    props.mode !== 'indeterminate',
            },

            // Colors
            'bg-primary',

            // Spacing & Sizing
            'm-0',
            { 'h-full w-0': props.mode !== 'indeterminate' },

            // Shape
            'border-0',

            // Transitions
            {
                'transition-width duration-1000 ease-in-out': props.mode !== 'indeterminate',
                'progressbar-value-animate': props.mode == 'indeterminate',
            },

            // Before & After (indeterminate)
            {
                'before:absolute before:top-0 before:left-0 before:bottom-0 before:bg-inherit ':
                    props.mode == 'indeterminate',
                'after:absolute after:top-0 after:left-0 after:bottom-0 after:bg-inherit after:delay-1000':
                    props.mode == 'indeterminate',
            },
        ],
    }),
    label: {
        class: [
            // Flexbox
            'inline-flex',

            // Font and Text
            'text-white dark:text-surface-900',
            'leading-6',
        ],
    },
}
