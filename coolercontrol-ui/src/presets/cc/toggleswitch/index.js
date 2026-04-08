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
            'inline-block relative',
            'w-12 h-7',
            'rounded-2xl',
            {
                'opacity-60 select-none pointer-events-none cursor-default': props.disabled,
            },
        ],
    }),
    slider: ({ props }) => ({
        class: [
            // Position
            'absolute top-0 left-0 right-0 bottom-0',
            { 'before:transform before:translate-x-5': props.modelValue == props.trueValue },

            // Shape
            'rounded-2xl',

            // Before (handle):
            'before:absolute before:top-1/2 before:left-1',
            'before:-mt-2.5',
            'before:h-5 before:w-5',
            'before:rounded-full',
            'before:duration-200',
            'before:bg-bg-two',

            // Colors
            'border',
            {
                'bg-bg-two': !(props.modelValue == props.trueValue),
                'bg-accent': props.modelValue == props.trueValue,
            },

            { 'border-border-one': !props.invalid },

            // Invalid State
            { 'border-red': props.invalid },

            // States
            {
                'peer-hover:bg-border-one':
                    !(props.modelValue == props.trueValue) && !props.disabled,
            },
            {
                'peer-hover:bg-accent/80': props.modelValue == props.trueValue && !props.disabled,
            },
            'peer-focus-visible:ring peer-focus-visible:ring-accent/50',

            // Transition
            'transition-colors duration-200',

            // Misc
            'cursor-pointer',
        ],
    }),
    input: {
        class: [
            'peer',

            // Size
            'w-full ',
            'h-full',

            // Position
            'absolute',
            'top-0 left-0',
            'z-10',

            // Spacing
            'p-0',
            'm-0',

            // Shape
            'opacity-0',
            'rounded-[2.5rem]',
            'outline-none',

            // Misc
            'appearance-none',
            'cursor-pointer',
        ],
    },
}
