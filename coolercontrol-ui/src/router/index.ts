import {createRouter, createWebHistory} from 'vue-router'
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
          path: '/profiles',
          name: 'profiles',
          component: () => import('@/views/ProfileView.vue'),
        },
        {
          path: '/functions',
          name: 'functions',
          component: () => import('@/views/FunctionView.vue'),
        },
      ]
    },
  ]
})

export default router
