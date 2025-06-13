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
            '[&>[data-pc-name=pcbadge]]:absolute',
            '[&>[data-pc-name=pcbadge]]:top-[90%]',
            '[&>[data-pc-name=pcbadge]]:right-[8%]',
            '[&>[data-pc-name=pcbadge]]:translate-x-1/2',
            '[&>[data-pc-name=pcbadge]]:-translate-y-1/2',
            '[&>[data-pc-name=pcbadge]]:m-0',
            '[&>[data-pc-name=pcbadge]]:origin-[100%_0]',
            '[&>[data-pc-name=pcbadge]]:outline',
            '[&>[data-pc-name=pcbadge]]:outline-[2px]',
            '[&>[data-pc-name=pcbadge]]:outline-bg-one',
        ],
    },
}
