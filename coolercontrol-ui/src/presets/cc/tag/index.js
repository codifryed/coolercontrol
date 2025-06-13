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
            //Font
            'text-xs font-bold',

            //Alignments
            'inline-flex items-center justify-center',

            //Spacing
            'px-2 py-1',

            //Shape
            {
                'rounded-md': !props.rounded,
                'rounded-full': props.rounded,
            },

            //Colors
            'text-primary-contrast',
            {
                'bg-primary dark:bg-primary':
                    props.severity == null || props.severity === 'primary',
                'text-surface-700 dark:text-surface-300 bg-surface-100 dark:bg-surface-500/20':
                    props.severity === 'secondary',
                'bg-green-500 dark:bg-green-400': props.severity === 'success',
                'bg-blue-500 dark:bg-blue-400': props.severity === 'info',
                'bg-orange-500 dark:bg-orange-400': props.severity === 'warn',
                'bg-red-500 dark:bg-red-400': props.severity === 'danger',
                'text-surface-0 dark:text-surface-900 bg-surface-900 dark:bg-surface-0':
                    props.severity === 'contrast',
            },
        ],
    }),
    value: {
        class: 'leading-normal',
    },
    icon: {
        class: 'mr-1 text-sm',
    },
}
