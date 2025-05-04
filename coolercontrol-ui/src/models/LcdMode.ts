/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon and contributors
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

import i18n from '@/i18n'

export enum LcdModeType {
    NONE = 'None',
    LC = 'Liquidctl',
    CUSTOM = 'Custom',
}

/**
 * 获取LcdModeType的本地化显示名称
 * @param type LcdModeType枚举值
 * @returns 本地化的显示名称
 */
export function getLcdModeTypeDisplayName(type: LcdModeType): string {
    const { t } = i18n.global
    switch (type) {
        case LcdModeType.NONE:
            return t('models.lcdModeType.none')
        case LcdModeType.LC:
            return t('models.lcdModeType.liquidctl')
        case LcdModeType.CUSTOM:
            return t('models.lcdModeType.custom')
        default:
            return String(type)
    }
}

export class LcdMode {
    constructor(
        readonly name: string,
        readonly frontend_name: string,
        readonly brightness: boolean,
        readonly orientation: boolean,
        readonly image: boolean = false,
        readonly colors_min: number = 0,
        readonly colors_max: number = 0,
        readonly type: LcdModeType = LcdModeType.LC,
    ) {}
}
