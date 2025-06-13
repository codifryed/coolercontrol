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
            {
                'flex flex-wrap items-center justify-center gap-2': props.mode === 'basic',
            },
        ],
    }),
    input: {
        class: 'hidden',
    },
    header: {
        class: [
            // Flexbox
            'flex',
            'flex-wrap',

            // Colors
            'bg-surface-50',
            'dark:bg-surface-800',
            'text-surface-700',
            'dark:text-white/80',

            // Spacing
            'p-5',
            'gap-2',

            // Borders
            'border',
            'border-solid',
            'border-surface-200',
            'dark:border-surface-700',
            'border-b-0',

            // Shape
            'rounded-tr-lg',
            'rounded-tl-lg',
        ],
    },
    content: {
        class: [
            // Position
            'relative',

            // Colors
            'bg-surface-0',
            'dark:bg-surface-900',
            'text-surface-700',
            'dark:text-white/80',

            // Spacing
            'p-8',

            // Borders
            'border',
            'border-surface-200',
            'dark:border-surface-700',

            // Shape
            'rounded-b-lg',

            //ProgressBar
            '[&>[data-pc-name=pcprogressbar]]:absolute',
            '[&>[data-pc-name=pcprogressbar]]:w-full',
            '[&>[data-pc-name=pcprogressbar]]:top-0',
            '[&>[data-pc-name=pcprogressbar]]:left-0',
            '[&>[data-pc-name=pcprogressbar]]:h-1',
        ],
    },
    file: {
        class: [
            // Flexbox
            'flex',
            'items-center',
            'flex-wrap',

            // Spacing
            'p-4',
            'mb-2',
            'last:mb-0',

            // Borders
            'border',
            'border-surface-200',
            'dark:border-surface-700',
            'gap-2',

            // Shape
            'rounded',
        ],
    },
    fileThumbnail: 'shrink-0',
    fileName: 'mb-2 break-all',
    fileSize: 'mr-2',
}
