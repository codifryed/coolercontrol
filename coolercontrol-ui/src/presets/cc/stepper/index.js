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
    root: 'has-[[data-pc-name=stepitem]]:flex has-[[data-pc-name=stepitem]]:flex-col',
    separator:
        'flex-1 w-full h-[2px] bg-surface-200 dark:bg-surface-700 transition-shadow duration-200',
    transition: {
        class: [
            'flex flex-1',
            'bg-surface-0 dark:bg-surface-800',
            'text-surface-900 dark:text-surface-0',
        ],
        enterFromClass: 'max-h-0',
        enterActiveClass:
            'overflow-hidden transition-[max-height] duration-1000 ease-[cubic-bezier(0.42,0,0.58,1)]',
        enterToClass: 'max-h-[1000px]',
        leaveFromClass: 'max-h-[1000px]',
        leaveActiveClass:
            'overflow-hidden transition-[max-height] duration-[450ms] ease-[cubic-bezier(0,1,0,1)]',
        leaveToClass: 'max-h-0',
    },
}
