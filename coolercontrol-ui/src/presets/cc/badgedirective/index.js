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
    root: ({ context }) => ({
        class: [
            // Font
            'font-bold',
            'text-xs leading-[normal]',

            // Alignment
            'flex items-center justify-center',
            'text-center',

            // Position
            'absolute top-0 right-0 transform translate-x-1/2 -translate-y-1/2 origin-top-right',

            // Size
            'm-0',
            {
                'p-0': context.nogutter || context.dot,
                'px-2': !context.nogutter && !context.dot,
                'min-w-[0.5rem] w-2 h-2': context.dot,
                'min-w-[1.5rem] h-6': !context.dot,
            },

            // Shape
            {
                'rounded-full': context.nogutter || context.dot,
                'rounded-[10px]': !context.nogutter && !context.dot,
            },

            // Color
            'text-primary-contrast',
            {
                'bg-primary':
                    !context.info &&
                    !context.success &&
                    !context.warning &&
                    !context.danger &&
                    !context.help &&
                    !context.secondary,
                'bg-surface-500 dark:bg-surface-400': context.secondary,
                'bg-green-500 dark:bg-green-400': context.success,
                'bg-blue-500 dark:bg-blue-400': context.info,
                'bg-orange-500 dark:bg-orange-400': context.warning,
                'bg-purple-500 dark:bg-purple-400': context.help,
                'bg-red-500 dark:bg-red-400': context.danger,
            },
        ],
    }),
}
