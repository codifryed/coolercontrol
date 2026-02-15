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
            // Display and Position
            {
                flex: props.fluid,
                'inline-flex': !props.fluid,
            },
            'max-w-full',
            'relative',

            // Misc
            { 'opacity-60 select-none pointer-events-none cursor-default': props.disabled },
        ],
    }),
    pcInputText: ({ props, parent }) => ({
        root: {
            class: [
                // Display
                'flex-auto w-[1%]',

                // Font
                'leading-none',

                // Colors
                'text-text-color',
                'placeholder:text-text-color-secondary',
                'bg-bg-one',
                'border-2 border-border-one',
                { 'border-border-one': !props.invalid },

                // Invalid State
                { 'border-red': props.invalid },

                // Spacing
                'm-0 p-3',

                // Shape
                'appearance-none',
                { 'rounded-lg': !props.showIcon || props.iconDisplay == 'input' },
                { 'rounded-l-lg  flex-1 pr-9': props.showIcon && props.iconDisplay !== 'input' },
                { 'rounded-lg flex-1 pr-9': props.showIcon && props.iconDisplay === 'input' },

                // Transitions
                'transition-colors',
                'duration-200',

                // States
                // { 'hover:border-primary-emphasis': !props.invalid },
                'outline-none',

                // Filled State *for FloatLabel
                { filled: parent.instance?.$name == 'FloatLabel' && props.modelValue !== null },
            ],
        },
    }),
    dropdownIcon: {
        class: ['absolute top-[50%] -mt-2', 'text-text-color-secondary', 'right-[.75rem]'],
    },
    dropdown: {
        class: [
            'relative w-10',

            // Alignments
            'items-center inline-flex text-center align-bottom justify-center',

            // Shape
            'rounded-r-lg',

            // Size
            'px-4 py-3 leading-none',

            // Colors
            'text-primary-inverse',
            'bg-primary',
            'border border-primary',

            // States
            'focus:outline-none focus:outline-offset-0 focus:ring',
            'hover:bg-primary-hover hover:border-primary-hover',
            'focus:ring-primary-400/50',
        ],
    },
    inputIconContainer: 'absolute cursor-pointer top-1/2 right-3 -mt-3',
    inputIcon: 'inline-block text-base',
    panel: ({ props }) => ({
        class: [
            // Display & Position
            {
                absolute: !props.inline,
                'inline-block': props.inline,
            },

            // Size
            { 'w-auto p-2 ': !props.inline },
            { 'min-w-[80vw] w-auto p-2 ': props.touchUI },
            { 'p-2 min-w-full': props.inline },

            // Shape
            'border rounded-lg',
            {
                'shadow-lg': !props.inline,
            },

            // Colors
            'bg-bg-two',
            'border-border-one',

            //misc
            { 'overflow-x-auto': props.inline },
        ],
    }),
    header: {
        class: [
            //Font
            'font-semibold',

            // Flexbox and Alignment
            'flex items-center justify-between',

            // Spacing
            'p-2',
            'm-0',

            // Shape
            'border-b',
            'rounded-t-lg',

            // Colors
            'text-text-color',
            'bg-bg-two',
            'border-border-one',
        ],
    },
    title: {
        class: [
            // Text
            'leading-8',
            'mx-auto my-0',
        ],
    },
    selectMonth: {
        class: [
            // Font
            'text-base leading-[normal]',
            'font-semibold',

            // Colors
            'text-text-color',

            // Transitions
            'transition duration-200',

            // Spacing
            'p-2',
            'm-0 mr-2',

            // States
            'hover:text-accent',

            // Misc
            'cursor-pointer',
        ],
    },
    selectYear: {
        class: [
            // Font
            'text-base leading-[normal]',
            'font-semibold',

            // Colors
            'text-text-color',

            // Transitions
            'transition duration-200',

            // Spacing
            'p-2',
            'm-0',

            // States
            'hover:text-accent',

            // Misc
            'cursor-pointer',
        ],
    },
    table: {
        class: [
            // Font
            'text-base leading-none',
            // Size & Shape
            'border-collapse',
            'w-full',

            // Spacing
            'm-0 my-2',
        ],
    },
    tableHeaderCell: {
        class: [
            // Spacing
            'p-0 md:p-2',
        ],
    },
    weekHeader: {
        class: ['leading-[normal]', 'text-text-color-secondary', 'opacity-60 cursor-default'],
    },
    weekNumber: {
        class: ['text-text-color-secondary', 'opacity-60 cursor-default'],
    },
    weekday: {
        class: [
            // Colors
            'text-text-color-secondary',
        ],
    },
    dayCell: {
        class: [
            // Spacing
            'p-0 md:p-2',
        ],
    },
    weekLabelContainer: {
        class: [
            // Flexbox and Alignment
            'flex items-center justify-center',
            'mx-auto',

            // Shape & Size
            'w-10 h-10',
            'rounded-full',
            'border-transparent border',

            // Colors
            'opacity-60 cursor-default',
        ],
    },
    dayView: 'w-full',
    day: ({ context }) => ({
        class: [
            // Flexbox and Alignment
            'flex items-center justify-center',
            'mx-auto',

            // Shape & Size
            'w-10 h-10',
            'rounded-full',
            'border-transparent border',

            // Colors
            {
                'text-accent/80': context.date.today,
                'text-text-color bg-transparent':
                    !context.selected && !context.disabled && !context.date.today,
                'bg-surface-hover text-accent': context.selected && !context.disabled,
            },

            // States
            'focus:outline-none focus:outline-offset-0 focus:ring focus:ring-primary-400/50',
            {
                'hover:bg-surface-hover': !context.selected && !context.disabled,
                'hover:bg-primary-highlight-hover': context.selected && !context.disabled,
            },
            {
                'opacity-60 cursor-default': context.disabled,
                'cursor-pointer': !context.disabled,
            },
        ],
    }),
    monthView: {
        class: [
            // Spacing
            'my-2',
        ],
    },
    month: ({ context }) => ({
        class: [
            // Flexbox and Alignment
            'inline-flex items-center justify-center',

            // Size
            'w-1/3',
            'p-2',

            // Shape
            'rounded-lg',

            // Colors
            {
                'text-text-color bg-transparent': !context.selected && !context.disabled,
                'bg-highlight': context.selected && !context.disabled,
            },

            // States
            'focus:outline-none focus:outline-offset-0 focus:ring focus:ring-primary-400/50',
            {
                'hover:bg-surface-hover': !context.selected && !context.disabled,
                'hover:bg-primary-highlight-hover': context.selected && !context.disabled,
            },

            // Misc
            'cursor-pointer',
        ],
    }),
    yearView: {
        class: [
            // Spacing
            'my-2',
        ],
    },
    year: ({ context }) => ({
        class: [
            // Flexbox and Alignment
            'inline-flex items-center justify-center',

            // Size
            'w-1/3',
            'p-2',

            // Shape
            'rounded-md',

            // Colors
            {
                'text-text-color bg-transparent': !context.selected && !context.disabled,
                'bg-highlight': context.selected && !context.disabled,
            },

            // States
            'focus:outline-none focus:outline-offset-0 focus:ring focus:ring-primary-400/50',
            {
                'hover:bg-surface-hover': !context.selected && !context.disabled,
                'hover:bg-primary-highlight-hover': context.selected && !context.disabled,
            },

            // Misc
            'cursor-pointer',
        ],
    }),
    timePicker: {
        class: [
            // Flexbox
            'flex',
            'justify-center items-center',

            // Borders
            'border-t-1',
            'border-solid border-border-one',

            // Spacing
            'p-2',
            'text-text-color',
        ],
    },
    separatorContainer: {
        class: [
            // Flexbox and Alignment
            'flex',
            'items-center',
            'flex-col',

            // Spacing
            'px-2',
        ],
    },
    separator: {
        class: [
            // Text
            'text-xl',
        ],
    },
    hourPicker: {
        class: [
            // Flexbox and Alignment
            'flex',
            'items-center',
            'flex-col',

            // Spacing
            'px-2',
        ],
    },
    minutePicker: {
        class: [
            // Flexbox and Alignment
            'flex',
            'items-center',
            'flex-col',

            // Spacing
            'px-2',
        ],
    },
    secondPicker: {
        class: [
            // Flexbox and Alignment
            'flex',
            'items-center',
            'flex-col',

            // Spacing
            'px-2',
        ],
    },
    ampmPicker: {
        class: [
            // Flexbox and Alignment
            'flex',
            'items-center',
            'flex-col',

            // Spacing
            'px-2',
        ],
    },
    calendarContainer: 'flex',
    calendar: 'flex-auto border-l first:border-l-0 border-border-one',
    buttonbar: {
        class: [
            // Flexbox
            'flex justify-between items-center',

            // Spacing
            'py-3 px-0',

            // Shape
            'border-t border-border-one',
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
