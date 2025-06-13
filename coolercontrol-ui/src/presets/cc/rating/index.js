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

            // Flex & Alignment
            'flex items-center',
            'gap-2',

            // Misc
            {
                'opacity-60 select-none pointer-events-none cursor-default': props.disabled,
            },
        ],
    }),
    option: ({ props, context }) => ({
        class: [
            // Flex & Alignment
            'inline-flex items-center',

            // State
            {
                'outline-none ring ring-primary-500/50 dark:ring-primary-400/50': context.focused,
            },

            // Misc
            {
                'cursor-pointer': !props.readonly,
                'cursor-default': props.readonly,
            },
        ],
    }),
    offIcon: ({ props }) => ({
        class: [
            // Size
            'w-5 h-5',

            // Color
            'text-surface-700 dark:text-surface-0/80',

            // State
            { 'hover:text-primary-500 dark:hover:text-primary-400': !props.readonly },

            // Transition
            'transition duration-200 ease-in',
        ],
    }),
    onIcon: ({ props }) => ({
        class: [
            // Size
            'w-5 h-5',

            // Color
            'text-primary',

            // State
            { 'hover:text-primary-600 dark:hover:text-primary-300': !props.readonly },

            // Transition
            'transition duration-200 ease-in',
        ],
    }),
}
