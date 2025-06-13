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
    root: 'relative flex',
    content:
        'overflow-x-auto overflow-y-hidden scroll-smooth overscroll-x-contain overscroll-y-auto [&::-webkit-scrollbar]:hidden grow',
    tabList: 'relative flex border-border-one border-none',
    nextButton:
        '!absolute top-0 right-0 z-20 h-full w-10 flex items-center justify-center text-surface-700 bg-surface-0 outline-transparent cursor-pointer shrink-0',
    prevButton:
        '!absolute top-0 left-0 z-20 h-full w-10 flex items-center justify-center text-surface-700 bg-surface-0 outline-transparent cursor-pointer shrink-0',
    activeBar: 'z-10 block absolute h-0 bottom-0',
}
