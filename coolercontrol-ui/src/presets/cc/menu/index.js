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
            // Sizing and Shape
            'min-w-[6rem]',
            'rounded-lg',
            'leading-none',
            // Spacing
            'p-1',
            // Colors
            'bg-bg-two',
            'text-text-color-secondary',
            'border border-border-one',
        ],
    },
    list: {
        class: [
            // Spacings and Shape
            'list-none',
            'm-0',
            'p-0',
            'outline-none',
        ],
    },
    itemContent: ({ context }) => ({
        class: [
            //Shape
            'rounded-lg',
            'p-1.5',
            // Colors
            'text-text-color-secondary',
            // {
            //     'bg-border-one': context.focused,
            // },
            // Transitions
            'transition-shadow',
            'duration-200',
            // States
            'hover:bg-surface-hover',
            'hover:text-text-color',
        ],
    }),
    itemLink: {
        class: [
            'relative',
            // Flexbox

            'flex',
            'items-center',

            // Spacing
            'p-1.5',
            // 'py-3',
            // 'px-5',

            // Color
            // 'text-surface-700 dark:text-white/80',

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
            // 'text-surface-600 dark:text-white/70'
        ],
    },
    itemLabel: {
        class: ['leading-none'],
    },
    submenuLabel: {
        class: [
            // Font
            'font-bold',
            // Spacing
            'm-0',
            // 'py-3 px-5',
            // Shape
            'rounded-tl-none',
            'rounded-tr-none',
            // Colors
            // 'bg-surface-0 dark:bg-surface-700',
            // 'text-surface-700 dark:text-white'
        ],
    },
    transition: {
        enterFromClass: 'opacity-0 scale-y-[0.8]',
        enterActiveClass:
            'transition-[transform,opacity] duration-[120ms] ease-[cubic-bezier(0,0,0.2,1)]',
        leaveActiveClass: 'transition-opacity duration-100 ease-linear',
        leaveToClass: 'opacity-0',
    },
}
