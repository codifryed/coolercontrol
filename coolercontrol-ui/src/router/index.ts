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
          component: () => import('@/views/HomeView.vue'),
          props: true,
        },
        {
          path: '/devices/:deviceId/speed/:name',
          name: 'device-speed',
          component: () => import('@/views/HomeView.vue'),
          props: true,
        },
        {
          path: '/devices/:deviceId/lighting/:name',
          name: 'device-lighting',
          component: () => import('@/views/HomeView.vue'),
          props: true,
        },
        {
          path: '/devices/:deviceId/lcd/:name',
          name: 'device-lcd',
          component: () => import('@/views/HomeView.vue'),
          props: true,
        },
      ]
    },
  ]
})

export default router
