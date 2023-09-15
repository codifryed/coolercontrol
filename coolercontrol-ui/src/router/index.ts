import {createRouter, createWebHistory} from 'vue-router'
// @ts-ignore
import AppLayout from '@/layout/AppLayout.vue'

const router = createRouter({
    history: createWebHistory(import.meta.env.BASE_URL),
    routes: [
        {
            path: '/',
            // name: 'home',
            component: AppLayout,
            children: [
                {
                    path: '/',
                    name: 'system-overview',
                    // @ts-ignore
                    component: () => import('@/views/SystemOverView.vue')
                    // component: () => import('@/views/HomeView.vue')
                },
                // {
                //     path: '/',
                //     name: 'dashboard',
                //     component: () => import('@/views/Dashboard.vue')
                // },
            ]
        },
    ]
})

export default router
