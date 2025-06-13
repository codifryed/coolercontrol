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
            // Misc
            { 'opacity-60 select-none pointer-events-none cursor-default': props.disabled },
        ],
    }),
    range: {
        class: [
            // Stroke
            // 'stroke-current',

            // Color
            'stroke-bg-two',

            // Fill
            'fill-none',

            // Transition
            'transition duration-100 ease-in',
        ],
    },
    value: {
        class: [
            // Animation
            'animate-dash-frame',

            // Color
            'stroke-accent',

            // Fill
            'fill-none',
        ],
    },
    text: {
        class: [
            // Text Style
            'text-center text-xl',

            // Color
            'fill-text-color',
        ],
    },
}
