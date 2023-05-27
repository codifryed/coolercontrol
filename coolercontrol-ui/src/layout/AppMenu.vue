<script setup lang="ts">
import {ref} from 'vue'

import AppMenuItem from './AppMenuItem.vue'

// todo: need to dynamically setup the menu from our app state (devices + channels as sub menus)
const model = ref([
    {
        label: '',
        items: [
            {label: 'System Overview', icon: 'pi pi-fw pi-chart-bar', to: '/'}
        ]
    },
    {
        label: '',
        items: [
            {
                label: 'Submenu 1',
                icon: 'pi pi-fw pi-bookmark',
                items: [
                    {
                        label: 'Submenu 1.1',
                        icon: 'pi pi-fw pi-bookmark',
                        items: [
                            {label: 'Submenu 1.1.1', icon: 'pi pi-fw pi-bookmark'},
                            {label: 'Submenu 1.1.2', icon: 'pi pi-fw pi-bookmark'},
                            {label: 'Submenu 1.1.3', icon: 'pi pi-fw pi-bookmark'}
                        ]
                    },
                    {
                        label: 'Submenu 1.2',
                        icon: 'pi pi-fw pi-bookmark',
                        items: [{label: 'Submenu 1.2.1', icon: 'pi pi-fw pi-bookmark'}]
                    }
                ]
            },
            {
                label: 'Submenu 2',
                icon: 'pi pi-fw pi-bookmark',
                items: [
                    {
                        label: 'Submenu 2.1',
                        icon: 'pi pi-fw pi-bookmark',
                        items: [
                            {label: 'Submenu 2.1.1', icon: 'pi pi-fw pi-bookmark'},
                            {label: 'Submenu 2.1.2', icon: 'pi pi-fw pi-bookmark'}
                        ]
                    },
                    {
                        label: 'Submenu 2.2',
                        icon: 'pi pi-fw pi-bookmark',
                        items: [{label: 'Submenu 2.2.1', icon: 'pi pi-fw pi-bookmark'}]
                    }
                ]
            }
        ]
    },
    {
        label: '',
        items: [
            {
                label: 'Documentation',
                icon: 'pi pi-fw pi-question',
                to: '/documentation'
            },
            {
                label: 'View Source',
                icon: 'pi pi-fw pi-search',
                url: 'https://github.com/primefaces/sakai-vue',
                target: '_blank'
            }
        ]
    }
])
</script>

<template>
    <ul class="layout-menu">
        <template v-for="(item, i) in model" :key="item">
            <app-menu-item v-if="!item.separator" :item="item" :index="i"></app-menu-item>
            <li v-if="item.separator" class="menu-separator"></li>
        </template>
    </ul>
</template>

<style lang="scss" scoped></style>
