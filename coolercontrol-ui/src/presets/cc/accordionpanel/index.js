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
    root: ({ props, context }) => ({
        class: [
            'flex flex-col border-none',
            {
                '[&>[data-pc-name=accordionheader]]:select-none [&>[data-pc-name=accordionheader]]:pointer-events-none [&>[data-pc-name=accordionheader]]:cursor-default [&>[data-pc-name=accordionheader]]:opacity-60':
                    props?.disabled,
                '[&>[data-pc-name=accordionheader]]:text-surface-600 dark:[&>[data-pc-name=accordionheader]]:text-surface-0 hover:[&>[data-pc-name=accordionheader]]:text-surface-100 dark:hover:[&>[data-pc-name=accordionheader]]:text-surface-0':
                    !props.disabled && context.active,
                '[&>[data-pc-section=toggleicon]]:text-surface-600 dark:[&>[data-pc-section=toggleicon]]:text-surface-0 hover:[&>[data-pc-section=toggleicon]]:text-surface-100 dark:hover:[&>[data-pc-section=toggleicon]]:text-surface-0':
                    !props.disabled && context.active,
                '[&:last-child>[data-pc-name=accordioncontent]>[data-pc-section=content]]:rounded-b-md':
                    !props.disabled && context.active,
                '[&:last-child>[data-pc-name=accordionheader]]:rounded-b-md':
                    !props.disabled && !context.active,
            },
            '[&:nth-child(n+2)>[data-pc-name=accordionheader]]:border-t-0',
            '[&:first-child>[data-pc-name=accordionheader]]:rounded-t-md',
        ],
    }),
}
