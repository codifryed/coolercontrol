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
    root: ({ props, context }) => ({
        class: [
            'relative',

            // Alignment
            'flex items-center justify-center',
            'px-3 py-2',
            'rounded-lg border',

            //Color
            {
                'bg-bg-one': !props.modelValue,
                'border-border-one': !props.modelValue && !props.invalid,
                'text-text-color': !props.modelValue,
                'bg-accent border-border-one text-text-color': props.modelValue,
            },

            // States
            {
                'hover:bg-surface-hover': !props.disabled && !props.modelValue,
                'focus:outline-none focus:outline-offset-0 focus-visible:ring-1 focus-visible:ring-primary-500':
                    !props.disabled,
            },

            // Invalid State
            { 'border-red': props.invalid },

            // Before
            'before:absolute before:left-1 before:top-1 before:w-[calc(100%-0.5rem)] before:h-[calc(100%-0.5rem)] before:rounded-[4px] before:z-0',

            // Transitions
            'transition-all duration-200',

            // Misc
            {
                'cursor-pointer': !props.disabled,
                'opacity-60 select-none pointer-events-none cursor-default': props.disabled,
            },

            // Misc
            'cursor-pointer',
            'select-none',
        ],
    }),
    content: 'relative items-center inline-flex justify-center gap-2',
    label: 'font-bold text-center w-full z-10 relative',
    icon: 'relative z-10 mr-2',
}
