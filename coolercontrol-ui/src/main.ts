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

import { createApp } from 'vue'
import { createPinia } from 'pinia'
import './style.css'
import 'primeicons/primeicons.css'
import 'uplot/dist/uPlot.min.css'
import 'abortcontroller-polyfill/dist/abortsignal-polyfill-only'

import App from './App.vue'
import router from './router'
import i18n from './i18n'

import PrimeVue from 'primevue/config'
import ToastService from 'primevue/toastservice'
import DialogService from 'primevue/dialogservice'
import ConfirmationService from 'primevue/confirmationservice'
import CC from './presets/cc'
import VueFullscreen from 'vue-fullscreen'

import Tooltip from 'primevue/tooltip'
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

app.use(createPinia())
app.use(router)
app.use(i18n)
app.use(PrimeVue, {
    unstyled: true,
    pt: CC,
})
app.use(ToastService)
app.use(DialogService)
app.use(ConfirmationService)
app.use(VueFullscreen)

app.directive('tooltip', Tooltip)

app.mount('#app')
