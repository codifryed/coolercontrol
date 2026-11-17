// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { execFileSync } from 'node:child_process'
import { readdirSync, readFileSync } from 'node:fs'
import { fileURLToPath, URL } from 'node:url'
import { defineConfig, type Plugin } from 'vite'
import vue from '@vitejs/plugin-vue'
import svgLoader from 'vite-svg-loader'
import loadVersion from 'vite-plugin-package-version'
import legacy from '@vitejs/plugin-legacy'
// https://vitejs.dev/config/

// Emits reflect-metadata as a separate asset and loads it via script src
// before the module entry, so decorator metadata APIs are available before
// any bundled code runs. A src tag (not inline) is required by the Qt app's CSP.
function reflectMetadataPlugin(): Plugin {
    const fileName = 'assets/Reflect.js'
    const reflectPath = fileURLToPath(
        new URL('./node_modules/reflect-metadata/Reflect.js', import.meta.url),
    )
    return {
        name: 'reflect-metadata-inject',
        configureServer(server) {
            server.middlewares.use(`/${fileName}`, (_req, res) => {
                res.setHeader('Content-Type', 'application/javascript')
                res.end(readFileSync(reflectPath, 'utf-8'))
            })
        },
        generateBundle() {
            this.emitFile({
                type: 'asset',
                fileName,
                source: readFileSync(reflectPath, 'utf-8'),
            })
        },
        transformIndexHtml: {
            order: 'pre',
            handler() {
                return [
                    {
                        tag: 'script',
                        attrs: { src: `/${fileName}` },
                        injectTo: 'head',
                    },
                ]
            },
        },
    }
}

// Experimental UI features gated to specific branch builds. Each feature lists
// the git branches it is enabled on, or '*' to enable it on every branch. Any
// other branch (and builds where the branch cannot be detected) leaves it off,
// so main and release builds stay clean. Consumed at runtime via src/features.ts.
const FEATURE_BRANCHES: Record<string, string[]> = {
    coolingWizard: ['*'],
}

function currentGitBranch(): string {
    try {
        return execFileSync('git', ['rev-parse', '--abbrev-ref', 'HEAD'], {
            stdio: ['ignore', 'pipe', 'ignore'],
        })
            .toString()
            .trim()
    } catch {
        return 'main'
    }
}

function buildFeatureFlags(): Record<string, boolean> {
    const branch = currentGitBranch()
    return Object.fromEntries(
        Object.entries(FEATURE_BRANCHES).map(([feature, branches]) => [
            feature,
            branches.includes('*') || branches.includes(branch),
        ]),
    )
}

// The dev server proxies the daemon so the UI and its API share one origin, as they do when
// the daemon serves the UI. The daemon refuses cookie-authenticated writes from any other
// origin, and the proxy is how dev passes that check without an exception in the daemon.
// Everything is proxied except what vite serves itself: its own paths and public/, which is
// read here so a new public file needs no edit.
const DAEMON_URL = process.env.CC_DAEMON_URL ?? 'http://localhost:11987'

function escapeRegExp(text: string): string {
    return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

function daemonProxyPattern(): string {
    const publicEntries = readdirSync(fileURLToPath(new URL('./public', import.meta.url)))
    const viteOwned = [
        '$',
        '\\?',
        'index\\.html',
        '@',
        'src/',
        'node_modules/',
        'assets/Reflect\\.js',
        ...publicEntries.map(escapeRegExp),
    ]
    return `^/(?!(?:${viteOwned.join('|')}))`
}

export default defineConfig({
    base: '/',
    define: {
        __FEATURES__: JSON.stringify(buildFeatureFlags()),
    },
    plugins: [
        reflectMetadataPlugin(),
        vue(),
        svgLoader(),
        loadVersion(),
        legacy({
            renderLegacyChunks: false,
            modernTargets: ['chrome >= 90', 'safari >= 12'],
            modernPolyfills: true,
        }),
    ],
    server: {
        proxy: {
            [daemonProxyPattern()]: {
                target: DAEMON_URL,
                // The daemon's self-signed certificate, when CC_DAEMON_URL is https.
                secure: false,
            },
        },
    },
    resolve: {
        alias: {
            '@': fileURLToPath(new URL('./src', import.meta.url)),
        },
    },
    build: {
        minify: 'oxc',
        cssMinify: 'lightningcss',
        // Everything is inlined so the app loads in as few requests as possible, but
        // not the fonts: base64 costs ~33% and welds them to the CSS bundle, so every
        // release would re-send all of them. Emitted as files they are content-hashed,
        // fetched once, and reused until the bytes actually change.
        assetsInlineLimit: (filePath: string) => !filePath.endsWith('.woff2'),
        cssCodeSplit: false,
        chunkSizeWarningLimit: 2_500,
        rollupOptions: {
            // reka-ui bundles its own @vueuse/core whose built output places
            // /* #__PURE__ */ comments where Rolldown flags them as invalid.
            // It is a third-party artifact we cannot fix, so silence only that
            // warning for that dependency; our own annotations still warn.
            onLog(level, log, handler) {
                if (
                    level === 'warn' &&
                    log.code === 'INVALID_ANNOTATION' &&
                    (log.id ?? log.message ?? '').includes('@vueuse/core')
                ) {
                    return
                }
                handler(level, log)
            },
        },
    },
    css: {
        postcss: './postcss.config.js',
        preprocessorOptions: {
            css: {
                extract: true,
            },
            scss: {
                api: 'modern-compiler',
                // This is temporary and lots of changes are happening for CC 2.0
                // silenceDeprecations: ['global-builtin', 'import'],
            },
        },
    },
})
