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
            // Spacing and Shape
            'rounded-lg',
            'outline-solid outline-0 outline-l-[6px]',

            // Colors
            {
                'bg-blue/20': props.severity == 'info',
                'bg-green/20': props.severity == 'success',
                'bg-bg-two': props.severity == 'secondary',
                'bg-yellow/20': props.severity == 'warn',
                'bg-red/20': props.severity == 'error',
                'bg-bg-one': props.severity == 'contrast',
            },
            {
                'outline-blue': props.severity == 'info',
                'outline-green': props.severity == 'success',
                'outline-border-one': props.severity == 'secondary',
                'outline-yellow': props.severity == 'warn',
                'outline-red': props.severity == 'error',
                'outline-border-one': props.severity == 'contrast',
            },
            {
                'text-blue': props.severity == 'info',
                'text-green': props.severity == 'success',
                'text-text-color-secondary': props.severity == 'secondary',
                'text-yellow': props.severity == 'warn',
                'text-red': props.severity == 'error',
                'text-text-color': props.severity == 'contrast',
            },
        ],
    }),
    content: {
        class: [
            // Flexbox
            'flex h-full',

            // Spacing
            'py-3 px-5 gap-2',
            // this is from the LCD Image warning about too large a file.
            // 'w-96',
        ],
    },
    icon: {
        class: [
            // Sizing and Spacing
            'w-6 h-6',
            'text-lg leading-none shrink-0',
        ],
    },
    text: {
        class: [
            // Font and Text
            'text-base leading-none',
            'font-medium',
        ],
    },
    closeButton: {
        class: [
            // Flexbox
            'flex items-center justify-center',

            // Size
            'w-8 h-8',

            // Spacing and Misc
            'ml-auto  relative',

            // Shape
            'rounded-full',

            // Colors
            'bg-transparent',

            // Transitions
            'transition duration-200 ease-in-out',

            // States
            'hover:bg-surface-hover',

            // Misc
            'overflow-hidden',
        ],
    },
    transition: {
        enterFromClass: 'opacity-0',
        enterActiveClass: 'transition-opacity duration-300',
        leaveFromClass: 'max-h-40',
        leaveActiveClass: 'overflow-hidden transition-all duration-300 ease-in',
        leaveToClass: 'max-h-0 opacity-0 !m-0',
    },
}
