# CoolerControl Internationalization (i18n) Guide

This project uses vue-i18n for internationalization support, currently supporting the following languages:

- English (en) - Default language
- Chinese (Simplified) (zh) - 中文（简体）
- Chinese (Traditional) (zh-tw) - 中文（繁體）
- Japanese (ja) - 日本語
- Russian (ru) - Русский
- German (de) - Deutsch
- French (fr) - Français
- Spanish (es) - Español
- Arabic (ar) - العربية
- Portuguese (pt) - Português
- Hindi (hi) - हिन्दी

## Directory Structure

```
src/i18n/
├── index.ts          # i18n configuration file
├── locales/          # Language files directory
│   ├── en.ts         # English
│   ├── zh.ts         # Chinese (Simplified)
│   ├── zh-tw.ts      # Chinese (Traditional)
│   ├── ja.ts         # Japanese
│   ├── ru.ts         # Russian
│   ├── de.ts         # German
│   ├── fr.ts         # French
│   ├── es.ts         # Spanish
│   ├── ar.ts         # Arabic
│   ├── pt.ts         # Portuguese
│   ├── hi.ts         # Hindi
│   └── index.d.ts    # Type definitions
└── README.md         # This documentation
```

## Usage

### In Templates

```vue
<template>
  <!-- Direct use of t function -->
  <div>{{ t('common.save') }}</div>
  
  <!-- Use in attributes -->
  <input :placeholder="t('common.loading')" />
  
  <!-- Use in directives -->
  <button v-tooltip="t('common.confirm')">Confirm</button>
</template>

<script setup>
import { useI18n } from 'vue-i18n'

// Get t function for translation
const { t } = useI18n()
</script>
```

### In JS/TS

```ts
import { useI18n } from 'vue-i18n'

// In setup function or setup script
const { t, locale } = useI18n()

// Translate text
console.log(t('common.save')) // English: "Save", Chinese: "保存"

// Switch language
function changeLanguage(lang: string) {
  locale.value = lang // 'en', 'zh', 'ja', etc.
  localStorage.setItem('locale', lang) // Save preference
}
```

## Adding New Languages

1. Create a new language file in the `src/i18n/locales/` directory, e.g. `ko.ts` for Korean:

```ts
export default {
  common: {
    save: '저장',
    // ... other translations
  },
  // ... other modules
}
```

2. Import and register the new language in `src/i18n/index.ts`:

```ts
import { createI18n } from 'vue-i18n'
import en from './locales/en.ts'
import zh from './locales/zh.ts'
import zhTw from './locales/zh-tw.ts'
import ja from './locales/ja.ts'
import ru from './locales/ru.ts'
import de from './locales/de.ts'
import fr from './locales/fr.ts'
import es from './locales/es.ts'
import ar from './locales/ar.ts'
import pt from './locales/pt.ts'
import hi from './locales/hi.ts'
import ko from './locales/ko.ts' // Add Korean

const i18n = createI18n({
  legacy: false,
  locale: localStorage.getItem('locale') || 'en',
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
    ko // Add Korean
  },
  silentTranslationWarn: true,
  silentFallbackWarn: true
})

export default i18n
```

3. Update the language switcher component `LanguageSwitcher.vue` options:

```ts
const localeOptions = [
  { name: 'English', code: 'en' },
  { name: '中文（简体）', code: 'zh' },
  { name: '中文（繁體）', code: 'zh-tw' },
  { name: '日本語', code: 'ja' },
  { name: 'Русский', code: 'ru' },
  { name: 'Deutsch', code: 'de' },
  { name: 'Français', code: 'fr' },
  { name: 'Español', code: 'es' },
  { name: 'العربية', code: 'ar' },
  { name: 'Português', code: 'pt' },
  { name: 'हिन्दी', code: 'hi' },
  { name: '한국어', code: 'ko' } 
]
```

## Internationalization Text Structure

Translation text is organized as nested objects, divided into the following main sections:

- `common`: Common text (buttons, tips, status, etc.)
- `layout`: Layout-related text (top bar, settings panel, etc.)
- `views`: Text for various view pages
- `components`: Component-related text

When adding new text, please follow the existing hierarchical structure to maintain good organization.

## Development Notes

1. All user-visible text should use i18n, avoid hardcoded text
2. Use meaningful key names with a hierarchical naming approach (e.g., `module.submodule.name`)
3. Update all language files when adding new features
4. Missing keys in non-English language files will automatically fall back to English 