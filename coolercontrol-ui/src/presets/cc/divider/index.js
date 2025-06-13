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
            // Flex and Position
            'flex relative',
            { 'justify-center': props.layout === 'vertical' },
            { 'items-center': props.layout === 'vertical' },
            {
                'justify-start': props?.align === 'left' && props.layout === 'horizontal',
                'justify-center': props?.align === 'center' && props.layout === 'horizontal',
                'justify-end': props?.align === 'right' && props.layout === 'horizontal',
                'items-center': props?.align === 'top' && props.layout === 'vertical',
                'items-start': props?.align === 'center' && props.layout === 'vertical',
                'items-end': props?.align === 'bottom' && props.layout === 'vertical',
            },

            // Spacing
            {
                'my-5 mx-0 py-0 px-5': props.layout === 'horizontal',
                'mx-4 md:mx-5 py-5': props.layout === 'vertical',
            },

            // Size
            {
                'w-full': props.layout === 'horizontal',
                'min-h-full': props.layout === 'vertical',
            },

            // Before: Line
            'before:block',

            // Position
            {
                'before:absolute before:left-0 before:top-1/2': props.layout === 'horizontal',
                'before:absolute before:left-1/2 before:top-0 before:transform before:-translate-x-1/2':
                    props.layout === 'vertical',
            },

            // Size
            {
                'before:w-full': props.layout === 'horizontal',
                'before:min-h-full': props.layout === 'vertical',
            },

            // Shape
            {
                'before:border-solid': props.type === 'solid',
                'before:border-dotted': props.type === 'dotted',
                'before:border-dashed': props.type === 'dashed',
            },

            // Color
            {
                'before:border-t before:border-border-one': props.layout === 'horizontal',
                'before:border-l before:border-border-one': props.layout === 'vertical',
            },
        ],
    }),
    content: {
        class: [
            // Space and Position
            'px-1 z-10',

            // Color
            'bg-bg-one',
        ],
    },
}
