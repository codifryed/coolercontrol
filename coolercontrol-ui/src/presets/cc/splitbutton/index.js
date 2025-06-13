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
            // Flexbox and Position
            'inline-flex',
            'relative',

            // Shape
            'rounded-md',
            { 'shadow-lg': props.raised },

            '[&>[data-pc-name=pcbutton]]:rounded-tr-none',
            '[&>[data-pc-name=pcbutton]]:rounded-br-none',
            '[&>[data-pc-name=pcdropdown]]:rounded-tl-none',
            '[&>[data-pc-name=pcdropdown]]:rounded-bl-none',
            '[&>[data-pc-name=pcmenu]]:min-w-full',
        ],
    }),
}
