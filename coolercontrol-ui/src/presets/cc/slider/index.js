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
            'relative',

            // Size
            {
                'h-1 w-60': props.orientation === 'horizontal',
                'w-1 h-56': props.orientation === 'vertical',
            },

            // Shape
            'border-0',
            'rounded-lg',

            // Colors
            'bg-accent',

            // States
            { 'opacity-60 select-none pointer-events-none cursor-default': props.disabled },
        ],
    }),
    range: ({ props }) => ({
        class: [
            // Position
            'block absolute',
            {
                'top-0 left-0': props.orientation === 'horizontal',
                'bottom-0 left-0': props.orientation === 'vertical',
            },

            //Size
            {
                'h-full': props.orientation === 'horizontal',
                'w-full': props.orientation === 'vertical',
            },

            // Colors
            'bg-accent',
            'rounded-lg',
        ],
    }),
    handle: ({ props }) => ({
        class: [
            'block',

            // Size
            'h-[1.143rem]',
            'w-[1.143rem]',
            {
                'top-[50%] mt-[-0.5715rem] ml-[-0.5715rem]': props.orientation === 'horizontal',
                'left-[50%] mb-[-0.5715rem] ml-[-0.5715rem]': props.orientation === 'vertical',
            },

            // Shape
            'rounded-full',
            'border-2',

            // Colors
            'bg-bg-one',
            'border-accent',

            // States
            'hover:bg-accent-emphasis',
            'focus-visible:outline-none focus-visible:outline-offset-0 focus-visible:none',
            'ring-primary-400/50 dark:ring-primary-300/50',

            // Transitions
            'transition duration-200',

            // Misc
            'cursor-grab',
            'touch-action-none',
        ],
    }),
    startHandler: ({ props }) => ({
        class: [
            'block',

            // Size
            'h-[1.143rem]',
            'w-[1.143rem]',
            {
                'top-[50%] mt-[-0.5715rem] ml-[-0.5715rem]': props.orientation === 'horizontal',
                'left-[50%] mb-[-0.5715rem] ml-[-0.4715rem]': props.orientation === 'vertical',
            },

            // Shape
            'rounded-full',
            'border-2',

            // Colors
            'bg-accent',
            'border-border-one',

            // States
            'hover:bg-accent-emphasis',
            'focus-visible:outline-none focus-visible:outline-offset-0 focus-visible:ring',
            'focus-visible:ring-primary-400/50 dark:focus-visible:ring-primary-300/50',

            // Transitions
            'transition duration-200',

            // Misc
            'cursor-grab',
            'touch-action-none',
        ],
    }),
    endHandler: ({ props }) => ({
        class: [
            'block',

            // Size
            'h-[1.143rem]',
            'w-[1.143rem]',
            {
                'top-[50%] mt-[-0.5715rem] ml-[-0.5715rem]': props.orientation === 'horizontal',
                'left-[50%] mb-[-0.5715rem] ml-[-0.4715rem]': props.orientation === 'vertical',
            },

            // Shape
            'rounded-full',
            'border-2',

            // Colors
            'bg-accent',
            'border-border-one',

            // States
            'hover:bg-accent-emphasis',
            'focus-visible:outline-none focus-visible:outline-offset-0 focus-visible:ring',
            'focus-visible:ring-primary-400/50 dark:focus-visible:ring-primary-300/50',

            // Transitions
            'transition duration-200',

            // Misc
            'cursor-grab',
            'touch-action-none',
        ],
    }),
}
