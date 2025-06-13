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
            // Font
            'font-bold',

            {
                'text-xs leading-[1.5rem]': props.size === null,
                'text-[0.625rem] leading-[1.25rem]': props.size === 'small',
                'text-lg leading-[2.25rem]': props.size === 'large',
                'text-2xl leading-[3rem]': props.size === 'xlarge',
            },

            // Alignment
            'text-center inline-block',

            // Size
            'p-0',
            {
                'w-2 h-2': props.value === null,
                // 'min-w-[1.5rem] h-[1.5rem]': props.value !== null && props.size === null,
                'w-5 h-5': props.value !== null && props.size === null,
                'min-w-[1.25rem] h-[1.25rem]': props.size === 'small',
                'min-w-[2.25rem] h-[2.25rem]': props.size === 'large',
                'min-w-[3rem] h-[3rem]': props.size === 'xlarge',
            },

            // Shape
            // {
            // 'rounded-full': props.value?.length === 1,
            // 'rounded-[0.71rem]': props.value?.length !== 1,
            // },
            'rounded-full',

            // Color
            'text-bg-one',
            // 'outline-none',
            {
                'bg-accent':
                    props.severity === null ||
                    props.severity === 'primary' ||
                    props.severity === 'secondary',
                'bg-success': props.severity === 'success',
                'bg-info': props.severity === 'info',
                'bg-warning': props.severity === 'warn',
                'bg-error': props.severity === 'error',
                'text-text-color bg-bg-one': props.severity === 'contrast',
            },
        ],
    }),
}
