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

import { createRouter, createWebHashHistory } from 'vue-router'
// @ts-ignore
import AppLayout from '@/layout/AppLayout.vue'

const router = createRouter({
    // For our use case, using the hash history allows users to bookmark links
    // without the daemon needing a catch-all rule. The only downside is that
    // it adds an extra # in the URL, which is bad for SEO, but that is not
    // a concern for us.
    history: createWebHashHistory(import.meta.env.BASE_URL),
    routes: [
        {
            path: '/',
            component: AppLayout,
            children: [
                {
                    path: '',
                    name: 'system-overview',
                    component: () => import('@/views/DashboardView.vue'),
                    props: true,
                },
                {
                    path: 'controls',
                    name: 'system-controls',
                    component: () => import('@/views/ControlsView.vue'),
                    props: true,
                },
                {
                    path: 'app-info',
                    name: 'app-info',
                    component: () => import('@/views/AppInfoView.vue'),
                    props: false,
                },
                {
                    path: '/settings',
                    name: 'settings',
                    component: () => import('@/layout/AppSettings.vue'),
                    props: false,
                },
                {
                    path: '/dashboards/:dashboardUID?',
                    name: 'dashboards',
                    component: () => import('@/views/DashboardView.vue'),
                    props: true,
                },
                {
                    path: '/modes/:modeUID',
                    name: 'modes',
                    component: () => import('@/views/ModeView.vue'),
                    props: true,
                },
                {
                    path: '/profiles/:profileUID',
                    name: 'profiles',
                    component: () => import('@/views/ProfileView.vue'),
                    props: true,
                },
                {
                    path: '/functions/:functionUID',
                    name: 'functions',
                    component: () => import('@/views/FunctionView.vue'),
                    props: true,
                },
                {
                    path: '/alerts/:alertUID?',
                    name: 'alerts',
                    component: () => import('@/views/AlertView.vue'),
                    props: true,
                },
                {
                    path: '/alerts-overview',
                    name: 'alerts-overview',
                    component: () => import('@/views/AlertsOverView.vue'),
                    props: true,
                },
                {
                    path: '/custom-sensors/:customSensorID?',
                    name: 'custom-sensors',
                    component: () => import('@/views/CustomSensorView.vue'),
                    props: true,
                },
                {
                    path: '/dashboards/:deviceUID/:channelName',
                    name: 'single-dashboard',
                    component: () => import('@/views/SingleDashboardView.vue'),
                    props: true,
                },
                {
                    path: '/devices/:deviceUID/speed/:channelName',
                    name: 'device-speed',
                    component: () => import('@/views/SpeedView.vue'),
                    props: true,
                },
                {
                    path: '/devices/:deviceId/lighting/:channelName',
                    name: 'device-lighting',
                    component: () => import('@/views/LightingView.vue'),
                    props: true,
                },
                {
                    path: '/devices/:deviceId/lcd/:channelName',
                    name: 'device-lcd',
                    component: () => import('@/views/LcdView.vue'),
                    props: true,
                },
                {
                    path: '/:pathMatch(.*)', // match any other route
                    name: 'not-found',
                    component: () => import('@/components/NotFound.vue'),
                },
            ],
        },
    ],
})

export default router
