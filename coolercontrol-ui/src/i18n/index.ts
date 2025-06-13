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

import { createI18n } from 'vue-i18n'
import en from './locales/en.ts'
import zh from './locales/zh.ts'
import ja from './locales/ja.ts'
import zhTw from './locales/zh-tw.ts'
import ru from './locales/ru.ts'
import de from './locales/de.ts'
import fr from './locales/fr.ts'
import es from './locales/es.ts'
import ar from './locales/ar.ts'
import pt from './locales/pt.ts'
import hi from './locales/hi.ts'

// Get saved language settings or use English as default
const savedLocale = localStorage.getItem('locale') || 'en'

const i18n = createI18n({
    legacy: false, // Use Composition API
    locale: savedLocale,
    fallbackLocale: 'en',
    messages: {
        en,
        zh,
        'zh-tw': zhTw,
        ja,
        ru,
        de,
        fr,
        es,
        ar,
        pt,
        hi,
    },
    silentTranslationWarn: true,
    silentFallbackWarn: true,
    warnHtmlMessage: false, // Disable warnings for HTML content in messages
    // Add additional options to ensure internationalization works properly
    sync: true,
    globalInjection: true,
})

console.debug('i18n instance created:', {
    currentLanguage: i18n.global.locale,
    availableMessages: Object.keys(i18n.global.messages),
})

export default i18n
