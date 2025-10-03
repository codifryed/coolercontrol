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
            // Shape
            'rounded-md shadow-lg',
            'border-0 dark:border',

            // Position
            'absolute left-0 top-0 mt-2',
            'z-40 transform origin-center',

            // Color
            // 'bg-surface-0 dark:bg-surface-800',
            // 'text-surface-700 dark:text-surface-0/80',
            // 'dark:border-surface-700',

            // Before: Arrow
            // 'before:absolute before:w-0 before:-top-3 before:h-0 before:border-transparent before:border-solid before:ml-[10px] before:border-x-[10px] before:border-b-[10px] before:border-t-0 before:border-b-surface-300/10 dark:before:border-b-surface-700',
            // 'after:absolute after:w-0 after:-top-[0.54rem] after:left-[4px] after:h-0 after:border-transparent after:border-solid after:ml-[8px] after:border-x-[8px] after:border-b-[8px] after:border-t-0 after:border-b-surface-0 dark:after:border-b-surface-800',
        ],
    },
    content: {
        class: 'ml-8 mt-[-4rem] mb-[-2rem] p-0 items-center flex',
    },
    transition: {
        enterFromClass: 'opacity-0 scale-y-[0.8]',
        enterActiveClass:
            'transition-[transform,opacity] duration-[120ms] ease-[cubic-bezier(0,0,0.2,1)]',
        leaveActiveClass: 'transition-opacity duration-100 ease-linear',
        leaveToClass: 'opacity-0',
    },
}
