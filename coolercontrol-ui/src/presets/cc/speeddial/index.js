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
    root: ({ state }) => ({
        class: [
            'static flex gap-2',
            {
                '[&_[data-pc-name=pcbutton]]:rotate-45': state.d_visible,
                '[&_[data-pc-name=pcbutton]]:rotate-0': !state.d_visible,
            },
        ],
    }),
    list: {
        class: [
            // Spacing
            'm-0 p-0',

            // Layout & Flexbox
            'list-none flex items-center justify-center',

            // Transitions
            'transition delay-200',

            // Z-Index (Positioning)
            'z-20',
        ],
    },
    item: ({ props, context }) => ({
        class: [
            'transform transition-transform duration-200 ease-out transition-opacity duration-800',

            // Conditional Appearance
            context.hidden ? 'opacity-0 scale-0' : 'opacity-100 scale-100',

            // Conditional Spacing
            {
                'my-1 first:mb-2': props.direction == 'up' && props.type == 'linear',
                'my-1 first:mt-2': props.direction == 'down' && props.type == 'linear',
                'mx-1 first:mr-2': props.direction == 'left' && props.type == 'linear',
                'mx-1 first:ml-2': props.direction == 'right' && props.type == 'linear',
            },

            // Conditional Positioning
            { absolute: props.type !== 'linear' },
        ],
    }),
    mask: ({ state }) => ({
        class: [
            // Base Styles
            'absolute left-0 top-0 w-full h-full transition-opacity duration-250 ease-in-out bg-black/40 z-0',

            // Conditional Appearance
            {
                'opacity-0 pointer-events-none': !state.d_visible,
                'opacity-100 transition-opacity duration-400 ease-in-out': state.d_visible,
            },
        ],
    }),
}
