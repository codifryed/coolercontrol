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
    root: ({ context, props, parent }) => ({
        class: [
            // Font
            'leading-[normal]',

            // Spacing
            'm-0 p-3',

            // Colors
            'text-surface-600 dark:text-surface-200',
            'placeholder:text-surface-400 dark:placeholder:text-surface-500',
            'bg-surface-0 dark:bg-surface-900',

            'border',
            { 'border-surface-300 dark:border-surface-600': !props.invalid },

            // Invalid State
            'invalid:focus:ring-red-200',
            'invalid:hover:border-red-500',
            { 'border-red-500 dark:border-red-400': props.invalid },

            // States
            {
                'hover:border-primary': !context.disabled && !props.invalid,
                'focus:outline-none focus:outline-offset-0 focus:ring focus:ring-primary-500/50 dark:focus:ring-primary-400/50':
                    !context.disabled,
                'opacity-60 select-none pointer-events-none cursor-default': context.disabled,
            },

            // Filled State *for FloatLabel
            {
                filled:
                    parent.instance?.$name == 'FloatLabel' &&
                    props.modelValue !== null &&
                    props.modelValue?.length !== 0,
            },

            // Misc
            'rounded-md',
            'appearance-none',
            'transition-colors duration-200',
        ],
    }),
}
