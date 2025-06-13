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
    root: ({ context }) => ({
        class: [
            'relative flex flex-auto items-center gap-2 p-2 last-of-type:flex-[initial]',
            { 'cursor-default pointer-events-none select-none opacity-60': context.disabled },
            '[&_[data-pc-section=separator]]:has-[~[data-p-active=true]]:bg-primary',
        ],
    }),
    header: ({ props, context }) => ({
        class: [
            'inline-flex items-center border-0 cursor-pointer rounded-md outline-transparent bg-transparent p-0 gap-2',
            'focus:outline-none focus:outline-offset-0 focus-visible:ring-1 ring-inset focus-visible:ring-primary-400 dark:focus-visible:ring-primary-300',
            { '!cursor-default': context.active },
            { 'cursor-auto': props.linear },
        ],
    }),
    number: ({ context }) => ({
        class: [
            // Flexbox
            'flex',
            'items-center',
            'justify-center',

            //Colors
            'border-solid border-2 dark:border-surface-700',

            // Colors (Conditional)
            context.active
                ? 'bg-primary text-primary-contrast border-primary'
                : 'border-surface-200 dark:border-surface-700 text-surface-900 dark:text-surface-0', // Adjust colors as needed

            // Size and Shape
            'min-w-9',
            'h-9',
            'line-height-[2rem]',
            'rounded-full',

            // Text
            'text-lg',

            // Transitions
            'transition',
            'transition-colors',
            'transition-shadow',
            'duration-200',
        ],
    }),
    title: ({ context }) => ({
        class: [
            // Layout
            'block',
            'whitespace-nowrap',
            'overflow-hidden',
            'text-ellipsis',
            'max-w-full',

            // Text
            context.active
                ? 'text-surface-900 dark:text-surface-0'
                : 'text-surface-700 dark:text-surface-0/80',
            'font-bold',

            // Transitions
            'transition',
            'transition-colors',
            'transition-shadow',
            'duration-200',
        ],
    }),
}
