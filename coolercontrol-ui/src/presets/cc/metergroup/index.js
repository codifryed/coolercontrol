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
            // Flexbox
            'flex gap-4',

            {
                'flex-col': props.orientation == 'horizontal',
                'flex-row': props.orientation == 'vertical',
            },
        ],
    }),
    meters: ({ props }) => ({
        class: [
            // Flexbox
            'flex',

            { 'flex-col': props.orientation === 'vertical' },

            // Sizing
            { 'w-2 h-full': props.orientation === 'vertical' },
            { 'h-2': props.orientation === 'horizontal' },

            // Colors
            'bg-gray-200 dark:bg-gray-700',

            // Border Radius
            'rounded-lg',
        ],
    }),
    meter: ({ props }) => ({
        class: [
            // Shape
            'border-0',

            // Rounded Corners - Horizontal
            {
                'first:rounded-l-lg last:rounded-r-lg': props.orientation === 'horizontal',
            },

            // Rounded Corners - Vertical
            {
                'first:rounded-t-lg last:rounded-b-lg': props.orientation === 'vertical',
            },

            // Colors
            'bg-primary',
        ],
    }),
    labelList: ({ props }) => ({
        class: [
            // Display & Flexbox
            'flex flex-wrap',

            { 'gap-4': props.labelOrientation === 'horizontal' },

            { 'gap-2': props.labelOrientation === 'vertical' },

            { 'flex-col': props.labelOrientation === 'vertical' },

            // Conditional Alignment - Horizontal
            {
                'align-end':
                    props.labelOrientation === 'horizontal' && props.labelPosition === 'end',
                'align-start':
                    props.labelOrientation === 'horizontal' && props.labelPosition === 'start',
            },

            // Conditional Alignment - Vertical
            {
                'justify-start':
                    props.labelOrientation === 'vertical' && props.labelPosition === 'start',
            },

            // List Styling
            'm-0 p-0 list-none',
        ],
    }),
    label: {
        class: [
            // Flexbox
            'inline-flex',
            'items-center',
            'gap-2',
        ],
    },
    labelMarker: {
        class: [
            // Display
            'inline-flex',

            // Background Color
            'bg-primary',

            // Size
            'w-2 h-2',

            // Rounded Shape
            'rounded-full',
        ],
    },
}
