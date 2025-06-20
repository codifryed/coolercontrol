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
    panel: {
        class: 'mb-1',
    },
    header: {
        class: [
            'rounded-md',
            'outline-none',
            'focus-visible:outline-none focus-visible:outline-offset-0 focus-visible:ring focus-visible:ring-primary-400/50 dark:focus-visible:ring-primary-300/50',
        ],
    },
    headerContent: ({ context, instance }) => ({
        class: [
            // Shape
            'rounded-t-md',
            {
                'rounded-br-md rounded-bl-md':
                    !context.active || instance.activeItem?.items === undefined,
                'rounded-br-0 rounded-bl-0':
                    context.active && instance.activeItem?.items !== undefined,
            },

            // Color
            'border border-surface-200 dark:border-surface-700',
            'bg-surface-50 dark:bg-surface-800',
            'text-surface-600 dark:text-surface-0/80',
            { 'text-surface-900': context.active },

            // States
            'hover:bg-surface-100 dark:hover:bg-surface-700',
            'hover:text-surface-900',

            // Transition
            'transition duration-200 ease-in-out',
            'transition-shadow duration-200',
        ],
    }),
    headerLink: {
        class: [
            'relative',

            // Font
            'font-bold',
            'leading-none',

            // Flex & Alignments
            'flex items-center',

            // Spacing
            'p-5',

            // Misc
            'select-none cursor-pointer no-underline',
        ],
    },
    headerLabel: {
        class: 'leading-none',
    },
    headerIcon: {
        class: 'mr-2',
    },
    submenuIcon: {
        class: 'mr-2',
    },
    content: {
        class: [
            // Spacing
            'py-2',

            // Shape
            'border border-t-0',
            'rounded-t-none rounded-br-md rounded-bl-md',

            // Color
            'text-surface-700 dark:text-white/80',
            'bg-surface-0 dark:bg-surface-800',
            'border-surface-200 dark:border-surface-700',
        ],
    },
    rootList: {
        class: ['outline-none', 'm-0 p-0 list-none'],
    },
    itemContent: {
        class: [
            // Shape
            'border-none rounded-none',

            // Color
            'text-surface-700 dark:text-white/80',

            // Transition
            'transition-shadow duration-200',
        ],
    },
    itemLink: ({ context }) => ({
        class: [
            'relative',

            // Font
            'leading-none',

            // Flex & Alignments
            'flex items-center',

            // Spacing
            'py-3 px-5',

            // Color
            'text-surface-700 dark:text-white/80',

            // States
            'hover:bg-surface-100 dark:hover:bg-surface-700/80 hover:text-surface-700 dark:hover:text-white/80',
            {
                'bg-surface-200 text-surface-700 dark:text-white/80 dark:bg-surface-600/90':
                    context.focused,
            },

            // Misc
            'cursor-pointer no-underline',
            'select-none overflow-hidden',
        ],
    }),
    itemIcon: {
        class: 'mr-2',
    },
    submenu: {
        class: 'p-0 pl-4 m-0 list-none',
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
}
