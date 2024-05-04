/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon and contributors
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

import { createRouter, createWebHistory } from 'vue-router'
// @ts-ignore
import AppLayout from '@/layout/AppLayout.vue'

const router = createRouter({
    history: createWebHistory(import.meta.env.BASE_URL),
    routes: [
        {
            path: '/',
            component: AppLayout,
            children: [
                {
                    path: '',
                    name: 'system-overview',
                    component: () => import('@/views/SystemOverView.vue'),
                    props: true,
                },
                {
                    path: '/devices/:deviceId/temp/:name',
                    name: 'device-temp',
                    component: () => import('@/views/TempView.vue'),
                    props: true,
                },
                {
                    path: '/devices/:deviceId/load/:name',
                    name: 'device-load',
                    component: () => import('@/views/LoadView.vue'),
                    props: true,
                },
                {
                    path: '/devices/:deviceId/freq/:name',
                    name: 'device-freq',
                    component: () => import('@/views/FreqView.vue'),
                    props: true,
                },
                {
                    path: '/devices/:deviceId/speed/:name',
                    name: 'device-speed',
                    component: () => import('@/views/SpeedView.vue'),
                    props: true,
                },
                {
                    path: '/devices/:deviceId/lighting/:name',
                    name: 'device-lighting',
                    component: () => import('@/views/LightingView.vue'),
                    props: true,
                },
                {
                    path: '/devices/:deviceId/lcd/:name',
                    name: 'device-lcd',
                    component: () => import('@/views/LcdView.vue'),
                    props: true,
                },
                {
                    path: '/profiles-functions',
                    name: 'profiles-functions',
                    component: () => import('@/views/ProfileFunctionView.vue'),
                },
                {
                    path: '/modes',
                    name: 'modes',
                    component: () => import('@/views/ModeView.vue'),
                },
            ],
        },
    ],
})

export default router
