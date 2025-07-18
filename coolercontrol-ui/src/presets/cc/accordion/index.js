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
    // For PrimeVue 3
    accordiontab: {
        root: {
            class: 'mb-1',
        },
        header: ({ props }) => ({
            class: [
                // State
                { 'select-none pointer-events-none cursor-default opacity-60': props?.disabled },
            ],
        }),
        headerAction: ({ context }) => ({
            class: [
                //Font
                'font-bold',
                'leading-none',

                // Alignments
                'flex items-center',
                'relative',

                // Sizing
                'p-5',

                // Shape
                'rounded-t-md',
                {
                    'rounded-br-md rounded-bl-md': !context.active,
                    'rounded-br-0 rounded-bl-0': context.active,
                },

                // Color
                'border border-surface-200 dark:border-surface-700',
                'bg-surface-50 dark:bg-surface-800',
                'text-surface-600 dark:text-surface-0/80',
                { 'text-surface-900': context.active },

                // Transition
                'transition duration-200 ease-in-out',
                'transition-shadow duration-200',

                // States
                'hover:bg-surface-100 dark:hover:bg-surface-700/40',
                'hover:text-surface-900',
                'focus:outline-none focus:outline-offset-0 focus-visible:ring focus-visible:ring-primary-400/50 ring-inset dark:focus-visible:ring-primary-300/50', // Focus

                // Misc
                'cursor-pointer no-underline select-none',
            ],
        }),
        headerIcon: {
            class: 'inline-block mr-2',
        },
        headerTitle: {
            class: 'leading-none',
        },
        content: {
            class: [
                // Spacing
                'p-5',

                //Shape
                'rounded-tl-none rounded-tr-none rounded-br-lg rounded-bl-lg',
                'border-t-0',

                // Color
                'bg-surface-0 dark:bg-surface-800',
                'border border-surface-200 dark:border-surface-700',
                'text-surface-700 dark:text-surface-0/80',
            ],
        },
        transition: {
            enterFromClass: 'max-h-0',
            enterActiveClass:
                'overflow-hidden transition-[max-height] duration-1000 ease-[cubic-bezier(0.42,0,0.58,1)]',
            enterToClass: 'max-h-[1000px]',
            leaveFromClass: 'max-h-[1000px]',
            leaveActiveClass:
                'overflow-hidden transition-[max-height] duration-[450ms] ease-[cubic-bezier(0,1,0,1)]',
            leaveToClass: 'max-h-0',
        },
    },
}
