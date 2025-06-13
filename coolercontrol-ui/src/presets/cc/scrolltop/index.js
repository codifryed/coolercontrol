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
    button: ({ props }) => ({
        root: {
            class: [
                // Flex & Alignment
                'flex items-center justify-center',

                // Positioning
                {
                    '!sticky flex ml-auto': props.target === 'parent',
                    '!fixed': props.target === 'window',
                },
                'bottom-[20px] right-[20px]',
                'h-12 w-12 rounded-full shadow-md',
                'text-white dark:text-surface-900 bg-surface-500 dark:bg-surface-400',
                'hover:bg-surface-600 dark:hover:bg-surface-300',
            ],
        },
    }),
    transition: {
        enterFromClass: 'opacity-0',
        enterActiveClass: 'transition-opacity duration-150',
        leaveActiveClass: 'transition-opacity duration-150',
        leaveToClass: 'opacity-0',
    },
}
