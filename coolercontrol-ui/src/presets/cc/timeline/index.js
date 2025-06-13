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
            'flex grow',
            {
                'flex-col': props.layout === 'vertical',
                'flex-row flex-1': props.layout === 'horizontal',
            },
        ],
    }),
    event: ({ props, context }) => ({
        class: [
            'flex relative min-h-[70px]',
            {
                'flex-row-reverse':
                    props.align === 'right' ||
                    (props.layout === 'vertical' &&
                        props.align === 'alternate' &&
                        context.index % 2 === 1),
                'flex-col flex-1': props.layout === 'horizontal',
                'flex-col-reverse ':
                    props.align === 'bottom' ||
                    (props.layout === 'horizontal' &&
                        props.align === 'alternate' &&
                        context.index % 2 === 1),
            },
        ],
    }),
    eventOpposite: ({ props, context }) => ({
        class: [
            'flex-1',
            {
                'px-4': props.layout === 'vertical',
                'py-4': props.layout === 'horizontal',
            },
            {
                'text-right':
                    props.align === 'left' ||
                    (props.layout === 'vertical' &&
                        props.align === 'alternate' &&
                        context.index % 2 === 0),
                'text-left':
                    props.align === 'right' ||
                    (props.layout === 'vertical' &&
                        props.align === 'alternate' &&
                        context.index % 2 === 1),
            },
        ],
    }),
    eventSeparator: ({ props }) => ({
        class: [
            'flex items-center flex-initial',
            {
                'flex-col': props.layout === 'vertical',
                'flex-row': props.layout === 'horizontal',
            },
        ],
    }),
    eventMarker: {
        class: [
            // Display & Flexbox
            'flex self-baseline',

            // Size
            'w-4 h-4',

            // Appearance
            'rounded-full border-2 border-primary bg-surface-0 dark:bg-surface-900/40',
        ],
    },
    eventConnector: ({ props }) => ({
        class: [
            'grow bg-surface-300 dark:bg-surface-700',
            {
                'w-[2px]': props.layout === 'vertical',
                'w-full h-[2px]': props.layout === 'horizontal',
            },
        ],
    }),
    eventContent: ({ props, context }) => ({
        class: [
            'flex-1',
            {
                'px-4': props.layout === 'vertical',
                'py-4': props.layout === 'horizontal',
            },
            {
                'text-left':
                    props.align === 'left' ||
                    (props.layout === 'vertical' &&
                        props.align === 'alternate' &&
                        context.index % 2 === 0),
                'text-right':
                    props.align === 'right' ||
                    (props.layout === 'vertical' &&
                        props.align === 'alternate' &&
                        context.index % 2 === 1),
            },
            {
                'min-h-0': props.layout === 'vertical' && context.index === context.count - 1,
                'grow-0': props.layout === 'horizontal' && context.index === context.count - 1,
            },
        ],
    }),
}
