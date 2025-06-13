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
    root: {
        class: [
            'block relative',

            // Base Label Appearance
            '[&>*:last-child]:text-text-color-secondary',
            '[&>*:last-child]:absolute',
            '[&>*:last-child]:top-1/2',
            '[&>*:last-child]:-translate-y-1/2',
            '[&>*:last-child]:left-3',
            '[&>*:last-child]:pointer-events-none',
            '[&>*:last-child]:transition-all',
            '[&>*:last-child]:duration-200',
            '[&>*:last-child]:ease',

            // Focus Label Appearance
            '[&>*:last-child]:has-[:focus]:-top-3',
            '[&>*:last-child]:has-[:focus]:text-sm',

            // Filled Input Label Appearance
            '[&>*:last-child]:has-[.filled]:-top-3',
            '[&>*:last-child]:has-[.filled]:text-sm',
        ],
    },
}
