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
    root: 'flex [&_[data-pc-name=pclist]]:h-full',
    sourceControls: {
        class: [
            // Flexbox & Alignment
            'flex lg:flex-col justify-center gap-2',

            // Spacing
            'p-5',
        ],
    },
    sourceListContainer: {
        class: [
            // Flexbox
            'grow shrink basis-2/4',

            // Shape
            'rounded-md',

            // Color
            'bg-surface-0 dark:bg-surface-900',
            'border border-surface-200 dark:border-surface-700',
            'outline-none',
        ],
    },
    transferControls: {
        class: 'flex lg:flex-col justify-center gap-2 p-5',
    },
    targetListContainer: {
        class: [
            // Flexbox
            'grow shrink basis-2/4',

            // Shape
            'rounded-md',

            // Color
            'bg-surface-0 dark:bg-surface-900',
            'border border-surface-200 dark:border-surface-700',
            'outline-none',
        ],
    },
    targetControls: {
        class: 'flex lg:flex-col justify-center gap-2 p-5',
    },
    transition: {
        enterFromClass: '!transition-none',
        enterActiveClass: '!transition-none',
        leaveActiveClass: '!transition-none',
        leaveToClass: '!transition-none',
    },
}
