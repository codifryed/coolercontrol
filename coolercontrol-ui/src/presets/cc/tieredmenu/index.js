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
            // Shape
            'rounded-md',

            // Size
            'min-w-[12rem]',
            'py-1',

            // Colors
            'bg-surface-0 dark:bg-surface-700',
            'border border-surface-200 dark:border-surface-700',
        ],
    },
    rootList: {
        class: [
            // Spacings and Shape
            'flex flex-col',
            'list-none',
            'm-0',
            'p-0',
            'outline-none',
        ],
    },
    item: {
        class: [
            // Position
            'relative',
        ],
    },
    itemContent: ({ context }) => ({
        class: [
            //Shape
            'rounded-none',

            //  Colors
            {
                'text-surface-500 dark:text-white/70': !context.focused && !context.active,
                'text-surface-500 dark:text-white/70 bg-surface-200 dark:bg-surface-600/90':
                    context.focused && !context.active,
                'bg-highlight':
                    (context.focused && context.active) ||
                    context.active ||
                    (!context.focused && context.active),
            },

            // Hover States
            {
                'hover:bg-surface-100 dark:hover:bg-surface-600/80': !context.active,
                'hover:bg-highlight-emphasis': context.active,
            },

            // Transitions
            'transition-shadow',
            'duration-200',
        ],
    }),
    itemLink: {
        class: [
            'relative',
            // Flexbox

            'flex',
            'items-center',

            // Spacing
            'py-3',
            'px-5',

            // Color
            'text-surface-700 dark:text-white/80',

            // Misc
            'no-underline',
            'overflow-hidden',
            'cursor-pointer',
            'select-none',
        ],
    },
    itemIcon: {
        class: [
            // Spacing
            'mr-2',

            // Color
            'text-surface-600 dark:text-white/70',
        ],
    },
    itemLabel: {
        class: ['leading-none'],
    },
    submenuIcon: {
        class: [
            // Position
            'ml-auto',
        ],
    },
    submenu: {
        class: [
            'flex flex-col',
            // Size
            'w-full sm:w-48',

            // Spacing
            'py-1',
            'm-0',
            'list-none',

            // Shape
            'shadow-none sm:shadow-md',
            'border-0',

            // Position
            'static sm:absolute',
            'z-10',

            // Color
            'bg-surface-0 dark:bg-surface-700',
        ],
    },
    separator: {
        class: 'border-t border-surface-200 dark:border-surface-600 my-1',
    },
}
