import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import svgLoader from 'vite-svg-loader'
import loadVersion from 'vite-plugin-package-version'

// https://vitejs.dev/config/

export default defineConfig({
    base: '/',
    plugins: [vue(), svgLoader(), loadVersion()],
    resolve: {
        alias: {
            '@': fileURLToPath(new URL('./src', import.meta.url)),
        },
    },
    build: {
        minify: 'esbuild',
        cssMinify: 'esbuild',
        assetsInlineLimit: 10_240_000,
        cssCodeSplit: false,
        chunkSizeWarningLimit: 2_000,
    },
})
