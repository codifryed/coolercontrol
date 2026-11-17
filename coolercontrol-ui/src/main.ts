// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { createApp } from 'vue'
import { createPinia } from 'pinia'
import './style.css'
import 'uplot/dist/uPlot.min.css'
import 'vue-color/style.css'
import 'abortcontroller-polyfill/dist/abortsignal-polyfill-only'

import App from './App.vue'
import router from './router'
import i18n from './i18n'

import { tooltipDirective } from '@/shell/tooltipDirective.ts'
import mitt from 'mitt'

const appVersion = import.meta.env.PACKAGE_VERSION
console.info(`
   ____            _            ____            _             _
  / ___|___   ___ | | ___ _ __ / ___|___  _ __ | |_ _ __ ___ | |
 | |   / _ \\ / _ \\| |/ _ \\ '__| |   / _ \\| '_ \\| __| '__/ _ \\| |
 | |__| (_) | (_) | |  __/ |  | |__| (_) | | | | |_| | | (_) | |
  \\____\\___/ \\___/|_|\\___|_|   \\____\\___/|_| |_|\\__|_|  \\___/|_|  v${appVersion}

 =======================================================================
`)
const app = createApp(App)
app.provide('emitter', mitt())
// Capture whether the app opened at the root URL, before the router's initial
// navigation rewrites the hash. App.vue applies the configured start page only
// for a root launch, not for a direct/deep link like /#/home.
app.provide('startedAtRoot', ['', '#', '#/'].includes(window.location.hash))

app.use(createPinia())
app.use(router)
app.use(i18n)

app.directive('tooltip', tooltipDirective)

app.mount('#app')
