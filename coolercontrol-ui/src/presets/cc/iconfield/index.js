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
            'relative',

            '[&>[data-pc-name=inputicon]]:absolute',
            '[&>[data-pc-name=inputicon]]:top-1/2',
            '[&>[data-pc-name=inputicon]]:-mt-2',
            '[&>[data-pc-name=inputicon]]:text-text-color-secondary',

            '[&>[data-pc-name=inputicon]:first-child]:left-3',
            '[&>[data-pc-name=inputicon]:last-child]:right-3',

            '[&>[data-pc-name=inputtext]:first-child]:pr-10',
            '[&>[data-pc-name=inputtext]:last-child]:pl-10',

            // filter
            '[&>[data-pc-extend=inputicon]]:absolute',
            '[&>[data-pc-extend=inputicon]]:top-1/2',
            '[&>[data-pc-extend=inputicon]]:-mt-2',
            '[&>[data-pc-extend=inputicon]]:text-text-color-secondary',

            '[&>[data-pc-extend=inputicon]:first-child]:left-3',
            '[&>[data-pc-extend=inputicon]:last-child]:right-3',
        ],
    },
}
