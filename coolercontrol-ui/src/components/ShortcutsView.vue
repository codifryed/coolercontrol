<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2021-2025  Guy Boldon and contributors
  -
  - This program is free software: you can redistribute it and/or modify
  - it under the terms of the GNU General Public License as published by
  - the Free Software Foundation, either version 3 of the License, or
  - (at your option) any later version.
  -
  - This program is distributed in the hope that it will be useful,
  - but WITHOUT ANY WARRANTY; without even the implied warranty of
  - MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  - GNU General Public License for more details.
  -
  - You should have received a copy of the GNU General Public License
  - along with this program.  If not, see <https://www.gnu.org/licenses/>.
  -->

<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
</script>

<template>
    <div class="text-text-color">
        <table class="w-[26rem]">
            <tbody>
                <tr>
                    <td class="table-data text-end">
                        {{ t('views.shortcuts.viewShortcuts') }}
                    </td>
                    <td class="table-data font-bold w-[12rem]">
                        <span class="border-border-one border-2 rounded-lg p-0.5">{{
                            t('views.shortcuts.ctrl')
                        }}</span>
                        <span class="border-border-one border-2 rounded-lg p-0.5 ml-1">{{
                            t('views.shortcuts.slash')
                        }}</span>
                    </td>
                </tr>
                <tr>
                    <td class="table-data text-end">
                        {{ t('views.shortcuts.home') }}
                    </td>
                    <td class="table-data font-bold">
                        <span class="border-border-one border-2 rounded-lg p-0.5">{{
                            t('views.shortcuts.ctrl')
                        }}</span>
                        <span class="border-border-one border-2 rounded-lg p-0.5 ml-1">{{
                            t('views.shortcuts.h')
                        }}</span>
                    </td>
                </tr>
                <tr>
                    <td class="table-data text-end">
                        {{ t('views.shortcuts.settings') }}
                    </td>
                    <td class="table-data font-bold">
                        <span class="border-border-one border-2 rounded-lg p-0.5">{{
                            t('views.shortcuts.ctrl')
                        }}</span>
                        <span class="border-border-one border-2 rounded-lg p-0.5 ml-1">{{
                            t('views.shortcuts.comma')
                        }}</span>
                    </td>
                </tr>
                <tr>
                    <td class="table-data text-end">
                        {{ t('views.shortcuts.info') }}
                    </td>
                    <td class="table-data font-bold">
                        <span class="border-border-one border-2 rounded-lg p-0.5">{{
                            t('views.shortcuts.ctrl')
                        }}</span>
                        <span class="border-border-one border-2 rounded-lg p-0.5 ml-1">{{
                            t('views.shortcuts.i')
                        }}</span>
                    </td>
                </tr>
                <tr>
                    <td class="table-data text-end">
                        {{ t('views.shortcuts.dashboardOne') }}
                    </td>
                    <td class="table-data font-bold">
                        <span class="border-border-one border-2 rounded-lg p-0.5">{{
                            t('views.shortcuts.ctrl')
                        }}</span>
                        <span class="border-border-one border-2 rounded-lg p-0.5 ml-1">{{
                            t('views.shortcuts.alt')
                        }}</span>
                        <span class="border-border-one border-2 rounded-lg p-0.5 ml-1">{{
                            t('views.shortcuts.one')
                        }}</span>
                    </td>
                </tr>
                <tr>
                    <td class="table-data text-end">
                        {{ t('views.shortcuts.dashboardTwo') }}
                    </td>
                    <td class="table-data font-bold">
                        <span class="border-border-one border-2 rounded-lg p-0.5">{{
                            t('views.shortcuts.ctrl')
                        }}</span>
                        <span class="border-border-one border-2 rounded-lg p-0.5 ml-1">{{
                            t('views.shortcuts.alt')
                        }}</span>
                        <span class="border-border-one border-2 rounded-lg p-0.5 ml-1">{{
                            t('views.shortcuts.two')
                        }}</span>
                    </td>
                </tr>
                <tr>
                    <td class="table-data text-end">
                        {{ t('views.shortcuts.dashboardThree') }}
                    </td>
                    <td class="table-data font-bold">
                        <span class="border-border-one border-2 rounded-lg p-0.5">{{
                            t('views.shortcuts.ctrl')
                        }}</span>
                        <span class="border-border-one border-2 rounded-lg p-0.5 ml-1">{{
                            t('views.shortcuts.alt')
                        }}</span>
                        <span class="border-border-one border-2 rounded-lg p-0.5 ml-1">{{
                            t('views.shortcuts.three')
                        }}</span>
                    </td>
                </tr>
                <tr>
                    <td class="table-data text-end">
                        {{ t('views.shortcuts.dashboardFour') }}
                    </td>
                    <td class="table-data font-bold">
                        <span class="border-border-one border-2 rounded-lg p-0.5">{{
                            t('views.shortcuts.ctrl')
                        }}</span>
                        <span class="border-border-one border-2 rounded-lg p-0.5 ml-1">{{
                            t('views.shortcuts.alt')
                        }}</span>
                        <span class="border-border-one border-2 rounded-lg p-0.5 ml-1">{{
                            t('views.shortcuts.four')
                        }}</span>
                    </td>
                </tr>
                <tr>
                    <td class="table-data text-end">
                        {{ t('views.shortcuts.alerts') }}
                    </td>
                    <td class="table-data font-bold">
                        <span class="border-border-one border-2 rounded-lg p-0.5">{{
                            t('views.shortcuts.ctrl')
                        }}</span>
                        <span class="border-border-one border-2 rounded-lg p-0.5 ml-1">{{
                            t('views.shortcuts.a')
                        }}</span>
                    </td>
                </tr>
                <tr>
                    <td class="table-data text-end">
                        {{ t('views.shortcuts.controls') }}
                    </td>
                    <td class="table-data font-bold">
                        <span class="border-border-one border-2 rounded-lg p-0.5">{{
                            t('views.shortcuts.ctrl')
                        }}</span>
                        <span class="border-border-one border-2 rounded-lg p-0.5 ml-1">{{
                            t('views.shortcuts.c')
                        }}</span>
                    </td>
                </tr>
                <tr>
                    <td class="table-data text-end">
                        {{ t('views.shortcuts.sideMenuCollapse') }}
                    </td>
                    <td class="table-data font-bold">
                        <span class="border-border-one border-2 rounded-lg p-0.5">{{
                            t('views.shortcuts.ctrl')
                        }}</span>
                        <span class="border-border-one border-2 rounded-lg p-0.5 ml-1">{{
                            t('views.shortcuts.left')
                        }}</span>
                    </td>
                </tr>
                <tr>
                    <td class="table-data text-end">
                        {{ t('views.shortcuts.sideMenuExpand') }}
                    </td>
                    <td class="table-data font-bold">
                        <span class="border-border-one border-2 rounded-lg p-0.5">{{
                            t('views.shortcuts.ctrl')
                        }}</span>
                        <span class="border-border-one border-2 rounded-lg p-0.5 ml-1">{{
                            t('views.shortcuts.right')
                        }}</span>
                    </td>
                </tr>
            </tbody>
        </table>
    </div>
</template>

<style scoped lang="scss">
.table-data {
    padding: 0.5rem;
    border: 1px solid rgb(var(--colors-border-one));
}
</style>
