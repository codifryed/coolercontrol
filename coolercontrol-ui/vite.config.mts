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

import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
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
// the git branches it is enabled on; any other branch (and builds where the
// branch cannot be detected) leaves it off, so main and release builds stay
// clean. Consumed at runtime via src/features.ts.
const FEATURE_BRANCHES: Record<string, string[]> = {
    coolingWizard: ['cooling-wizard', 'compio-base'],
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
            branches.includes(branch),
        ]),
    )
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
    resolve: {
        alias: {
            '@': fileURLToPath(new URL('./src', import.meta.url)),
        },
    },
    build: {
        minify: 'oxc',
        cssMinify: 'lightningcss',
        assetsInlineLimit: 10_240_000,
        cssCodeSplit: false,
        chunkSizeWarningLimit: 2_500,
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
