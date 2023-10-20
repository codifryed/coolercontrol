import {fileURLToPath, URL} from 'node:url'

import {defineConfig} from 'vite'
import vue from '@vitejs/plugin-vue'
import {viteSingleFile} from "vite-plugin-singlefile"
import svgLoader from 'vite-svg-loader'
import loadVersion from 'vite-plugin-package-version'

// https://vitejs.dev/config/

export default defineConfig({
  plugins: [
    vue(), viteSingleFile(), svgLoader(), loadVersion()
  ],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url))
    }
  },
  build: {
    target: 'modules',
    minify: 'esbuild',
    cssMinify: 'esbuild'
  }
})
