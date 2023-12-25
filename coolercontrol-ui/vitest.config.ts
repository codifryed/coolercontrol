import {fileURLToPath} from 'node:url'
import {mergeConfig} from 'vite'
import {configDefaults} from 'vitest/config'
import viteConfig from './vite.config'

/** @type {import('vite').UserConfig} */
export default mergeConfig(
    viteConfig,
    {
      test: {
        watch: false,
        environment: 'jsdom',
        exclude: [...configDefaults.exclude, 'e2e/*'],
        root: fileURLToPath(new URL('./', import.meta.url)),
        transformMode: {
          web: [/\.[jt]sx$/],
        },
      }
    }
)
