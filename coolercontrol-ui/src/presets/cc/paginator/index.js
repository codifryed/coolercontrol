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
            'flex items-center justify-center flex-wrap',

            // Spacing
            'px-4 py-2',

            // Shape
            'border-0',

            // Color
            'bg-surface-0 dark:bg-surface-800',
            'text-surface-500 dark:text-white/60',
        ],
    },
    first: ({ context }) => ({
        class: [
            'relative',

            // Flex & Alignment
            'inline-flex items-center justify-center',

            // Shape
            'border-0 rounded-full dark:rounded-md',

            // Size
            'min-w-[3rem] h-12 m-[0.143rem]',
            'leading-none',

            // Color
            'text-surface-500 dark:text-white/60',

            // State
            {
                'hover:bg-surface-50 dark:hover:bg-surface-700/70': !context.disabled,
                'focus:outline-none focus:outline-offset-0 focus:ring focus:ring-primary-400/50 dark:focus:ring-primary-300/50':
                    !context.disabled,
            },

            // Transition
            'transition duration-200',

            // Misc
            'user-none overflow-hidden',
            { 'cursor-default pointer-events-none opacity-60': context.disabled },
        ],
    }),
    prev: ({ context }) => ({
        class: [
            'relative',

            // Flex & Alignment
            'inline-flex items-center justify-center',

            // Shape
            'border-0 rounded-full dark:rounded-md',

            // Size
            'min-w-[3rem] h-12 m-[0.143rem]',
            'leading-none',

            // Color
            'text-surface-500 dark:text-white/60',

            // State
            {
                'hover:bg-surface-50 dark:hover:bg-surface-700/70': !context.disabled,
                'focus:outline-none focus:outline-offset-0 focus:ring focus:ring-primary-400/50 dark:focus:ring-primary-300/50':
                    !context.disabled,
            },

            // Transition
            'transition duration-200',

            // Misc
            'user-none overflow-hidden',
            { 'cursor-default pointer-events-none opacity-60': context.disabled },
        ],
    }),
    next: ({ context }) => ({
        class: [
            'relative',

            // Flex & Alignment
            'inline-flex items-center justify-center',

            // Shape
            'border-0 rounded-full dark:rounded-md',

            // Size
            'min-w-[3rem] h-12 m-[0.143rem]',
            'leading-none',

            // Color
            'text-surface-500 dark:text-white/60',

            // State
            {
                'hover:bg-surface-50 dark:hover:bg-surface-700/70': !context.disabled,
                'focus:outline-none focus:outline-offset-0 focus:ring focus:ring-primary-400/50 dark:focus:ring-primary-300/50':
                    !context.disabled,
            },

            // Transition
            'transition duration-200',

            // Misc
            'user-none overflow-hidden',
            { 'cursor-default pointer-events-none opacity-60': context.disabled },
        ],
    }),
    last: ({ context }) => ({
        class: [
            'relative',

            // Flex & Alignment
            'inline-flex items-center justify-center',

            // Shape
            'border-0 rounded-full dark:rounded-md',

            // Size
            'min-w-[3rem] h-12 m-[0.143rem]',
            'leading-none',

            // Color
            'text-surface-500 dark:text-white/60',

            // State
            {
                'hover:bg-surface-50 dark:hover:bg-surface-700/70': !context.disabled,
                'focus:outline-none focus:outline-offset-0 focus:ring focus:ring-primary-400/50 dark:focus:ring-primary-300/50':
                    !context.disabled,
            },

            // Transition
            'transition duration-200',

            // Misc
            'user-none overflow-hidden',
            { 'cursor-default pointer-events-none opacity-60': context.disabled },
        ],
    }),
    page: ({ context }) => ({
        class: [
            'relative',

            // Flex & Alignment
            'inline-flex items-center justify-center',

            // Shape
            'border-0 rounded-full dark:rounded-md',

            // Size
            'min-w-[3rem] h-12 m-[0.143rem]',
            'leading-none',

            // Color
            'text-surface-500 dark:text-white/80',
            {
                'bg-highlight': context.active,
            },

            // State
            {
                'hover:bg-surface-50 dark:hover:bg-surface-700/70':
                    !context.disabled && !context.active,
                'focus:outline-none focus:outline-offset-0 focus:ring focus:ring-primary-400/50 dark:focus:ring-primary-300/50':
                    !context.disabled,
            },

            // Transition
            'transition duration-200',

            // Misc
            'user-none overflow-hidden',
            { 'cursor-default pointer-events-none opacity-60': context.disabled },
        ],
    }),
    contentStart: 'mr-auto',
    contentEnd: 'ml-auto',
}
