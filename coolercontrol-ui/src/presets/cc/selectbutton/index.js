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
            'inline-flex select-none align-bottom outline-transparent',
            'border border-border-one rounded-lg [&>button]:rounded-none',
            '[&>button:first-child]:border-r-none [&>button:first-child]:rounded-r-none [&>button:first-child]:rounded-tl-lg [&>button:first-child]:rounded-bl-lg',
            '[&>button:last-child]:border-l-none [&>button:first-child]:rounded-l-none [&>button:last-child]:rounded-tr-lg [&>button:last-child]:rounded-br-lg',

            // Invalid State
            {
                'border-red': props.invalid,
                'border-border-one': !props.invalid,
            },
        ],
    }),
}
