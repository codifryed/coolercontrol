<!--
  - CoolerControl - monitor and control your cooling and other devices
  - Copyright (c) 2021-2024  Guy Boldon and contributors
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
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon'
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport } from 'radix-vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { computed, onMounted } from 'vue'
import { DaemonStatus, useDaemonState } from '@/stores/DaemonState.ts'
import { mdiBookOpenVariantOutline, mdiCircle, mdiGit } from '@mdi/js'
import Button from 'primevue/button'

const appVersion = import.meta.env.PACKAGE_VERSION
const deviceStore = useDeviceStore()
const daemonState = useDaemonState()

const healthCheck = await deviceStore.health()
const convertLogsToHtml = computed((): string => {
    return deviceStore.logs
        .replaceAll('\n', '<br/>')
        .replaceAll('INFO', '<span class="text-success">INFO</span>')
        .replaceAll('ERROR', '<span class="text-error">ERROR</span>')
        .replaceAll('WARN', '<span class="text-warning">WARN</span>')
        .replaceAll('DEBUG', '<span class="text-info">DEBUG</span>')
        .replaceAll('TRACE', '<span class="text-pink">TRACE</span>')
})
const badgeColor = computed((): string => {
    switch (daemonState.status) {
        case DaemonStatus.OK:
            return 'text-success'
        case DaemonStatus.WARN:
            return 'text-warning'
        case DaemonStatus.ERROR:
            return 'text-error'
        default:
            return 'text-error'
    }
})

const downloadLogFileName = 'coolercontrold-current.log'
const downloadLogHref = computed((): string => {
    const blob = new Blob([deviceStore.logs], { type: 'text/plain' })
    return URL.createObjectURL(blob)
})
const downloadLogDatasetURL = computed((): string => {
    // used for a.dataset.downloadurl
    return ['text/plain', downloadLogFileName, downloadLogHref].join(':')
})

onMounted(() => {
    const logOutput = document.getElementById('log-output')!
    // scroll to bottom:
    logOutput.scrollTop = logOutput.scrollHeight
})
</script>

<template>
    <div class="flex h-[3.5rem] border-b-4 border-border-one items-center justify-between">
        <div class="pl-4 py-2 text-2xl font-bold">Application Information</div>
    </div>
    <ScrollAreaRoot style="--scrollbar-size: 10px">
        <ScrollAreaViewport class="p-4 pb-16 h-screen w-full">
            <h3 class="mt-4 text-4xl font-sans subpixel-antialiased">
                CoolerControl
                <a href="https://gitlab.com/coolercontrol/coolercontrol/-/releases" target="_blank">
                    <span class="text-lg font-bold underline">v{{ appVersion }}</span>
                </a>
            </h3>
            <p class="text-sm italic">This program comes with absolutely no warranty.</p>
            <div class="mt-8">
                <div class="flex flex-row">
                    <div class="bg-bg-two border border-border-one p-4 rounded-lg text-text-color">
                        <table class="w-96">
                            <tbody>
                                <tr>
                                    <td
                                        class="mb-4 p-2 flex justify-end items-center font-semibold text-xl text-text-color"
                                    >
                                        Daemon Status
                                    </td>
                                    <td class="pl-2">
                                        <Button
                                            label="Acknowledge Issues"
                                            class="mb-4 bg-accent/80 hover:!bg-accent h-[2.375rem]"
                                            :disabled="daemonState.status === DaemonStatus.OK"
                                            @click="daemonState.acknowledgeLogIssues()"
                                        />
                                    </td>
                                </tr>
                                <tr>
                                    <td class="table-data font-bold text-lg text-end">Status</td>
                                    <td class="table-data">
                                        <div class="flex flex-row items-center">
                                            <svg-icon
                                                type="mdi"
                                                class="mr-2"
                                                :class="badgeColor"
                                                :path="mdiCircle"
                                                :size="deviceStore.getREMSize(1.25)"
                                            />
                                            {{ daemonState.status }}
                                        </div>
                                    </td>
                                </tr>
                                <tr>
                                    <td class="table-data font-bold text-lg text-end">
                                        Process Status
                                    </td>
                                    <td class="table-data">{{ healthCheck.status }}</td>
                                </tr>
                                <tr>
                                    <td class="table-data font-bold text-lg text-end">Host</td>
                                    <td class="table-data w-44">
                                        {{ healthCheck.system.name }}
                                    </td>
                                </tr>
                                <tr>
                                    <td class="table-data font-bold text-lg text-end">Uptime</td>
                                    <td class="table-data w-44">
                                        {{ healthCheck.details.uptime }}
                                    </td>
                                </tr>
                                <tr>
                                    <td class="table-data font-bold text-lg text-end">Version</td>
                                    <td class="table-data">{{ healthCheck.details.version }}</td>
                                </tr>
                                <tr>
                                    <td class="table-data font-bold text-lg text-end">
                                        Process ID
                                    </td>
                                    <td class="table-data">{{ healthCheck.details.pid }}</td>
                                </tr>
                                <tr>
                                    <td class="table-data font-bold text-lg text-end">
                                        Memory Usage
                                    </td>
                                    <td class="table-data">
                                        {{ healthCheck.details.memory_mb }} MB
                                    </td>
                                </tr>
                                <tr>
                                    <td class="table-data font-bold text-lg text-end">Liquidctl</td>
                                    <td class="table-data">
                                        {{
                                            healthCheck.details.liquidctl_connected
                                                ? 'Connected'
                                                : 'Disconnected'
                                        }}
                                    </td>
                                </tr>
                            </tbody>
                        </table>
                    </div>
                    <div class="w-full" />
                </div>
            </div>
            <div class="mt-8">
                <div
                    class="flex flex-col bg-bg-two border border-border-one p-4 rounded-lg text-text-color"
                >
                    <span class="pb-2 ml-1 font-semibold text-xl text-text-color">
                        Current Daemon Logs
                    </span>
                    <div
                        id="log-output"
                        class="w-full max-h-[42rem] overflow-x-auto overflow-y-auto bg-bg-one border border-border-one/100 p-2"
                        v-html="convertLogsToHtml"
                    />
                    <a
                        :download="downloadLogFileName"
                        :href="downloadLogHref"
                        :data-downloadurl="downloadLogDatasetURL"
                    >
                        <Button
                            label="Download Current Logs"
                            class="mt-4 bg-accent/80 hover:!bg-accent h-[2.375rem]"
                        />
                    </a>
                </div>
            </div>
            <div class="mt-8">
                <div
                    class="flex flex-col bg-bg-two border border-border-one p-4 rounded-lg text-text-color"
                >
                    <span class="mb-4 font-semibold text-xl text-text-color">More Logs</span>
                    <p class="text-wrap">
                        The CoolerControl daemons normally run as systemd system-level services. The
                        journal system collects all logs from your system services and you can
                        collect and view those logs using the
                        <code>journalctl</code> command.
                    </p>
                    <p class="text-wrap mt-4">
                        To see more logs, or to collect logs to fill out a bug report, use the
                        following commands in your terminal:
                    </p>
                    <p class="mt-4">To output logs to a file in your current directory:</p>
                    <div
                        class="mt-1 w-full max-h-[42rem] overflow-x-auto overflow-y-auto bg-bg-one border border-border-one/100 p-2"
                    >
                        <span class="font-semibold">
                            journalctl --no-pager -u coolercontrold -u coolercontrol-liqctld -n
                            10000 > coolercontrol-daemons.log
                        </span>
                    </div>
                    <p class="mt-4">To follow current log output live in your terminal:</p>
                    <div
                        class="mt-1 w-full max-h-[42rem] overflow-x-auto overflow-y-auto bg-bg-one border border-border-one/100 p-2"
                    >
                        <span class="font-semibold">
                            journalctl -u coolercontrold -u coolercontrol-liqctld -f -n 100
                        </span>
                    </div>
                    <p class="mt-4">
                        To change the current log level, for example for debug output:
                    </p>
                    <p class="mt-0 italic text-sm">Note: debug logs produces a lot of output</p>
                    <div
                        class="mt-1 w-full max-h-[42rem] overflow-x-auto overflow-y-auto bg-bg-one border border-border-one/100 p-2"
                    >
                        <span class="font-semibold">
                            sudo sed -i -E 's|COOLERCONTROL_LOG=INFO|COOLERCONTROL_LOG=DEBUG|'
                            /lib/systemd/system/coolercontrold.service<br />
                            sudo systemctl daemon-reload<br />
                            sudo systemctl restart coolercontrold
                        </span>
                    </div>
                </div>
            </div>
            <div class="mt-8">
                <div
                    class="flex flex-col bg-bg-two border border-border-one p-4 rounded-lg text-text-color"
                >
                    <span class="mb-4 font-semibold text-xl text-text-color">
                        Backup/Import Configuration Files
                    </span>
                    <p class="text-wrap">
                        Like most *nix <span class="font-bold">system-level</span> services, the
                        daemon configuration files are stored under <code>/etc</code>, and for
                        CoolerControl specifically under <code>/etc/coolercontrol/</code>.
                    </p>
                    <p class="mt-4">To create a complete backup:</p>
                    <div
                        class="mt-1 w-full max-h-[42rem] overflow-x-auto overflow-y-auto bg-bg-one border border-border-one/100 p-2"
                    >
                        <span class="font-semibold">
                            sudo tar -czvf coolercontrol-backup.tgz /etc/coolercontrol
                        </span>
                    </div>
                    <p class="mt-4">
                        To import and overwrite the current settings from a complete backup:
                    </p>
                    <div
                        class="mt-1 w-full max-h-[42rem] overflow-x-auto overflow-y-auto bg-bg-one border border-border-one/100 p-2"
                    >
                        <span class="font-semibold">
                            sudo systemctl stop coolercontrold<br />
                            sudo tar -xvf coolercontrol-backup.tgz -C /<br />
                            sudo systemctl start coolercontrold
                        </span>
                    </div>
                </div>
            </div>
            <div class="mt-8 mb-6">
                <div class="bg-bg-two border border-border-one p-4 rounded-lg text-text-color">
                    <span class="mb-4 font-semibold text-xl text-text-color"> Links </span>
                    <p class="mt-4 text-wrap flex flex-row items-center">
                        <a target="_blank" :href="healthCheck.links.docs" class="text-accent">
                            <div class="flex flex-row items-center">
                                <svg-icon
                                    type="mdi"
                                    class="mr-2"
                                    :path="mdiBookOpenVariantOutline"
                                    :size="deviceStore.getREMSize(2.0)"
                                />
                                Documentation
                            </div> </a
                        >&nbsp;- Get more information
                    </p>
                    <p class="mt-4 text-wrap flex flex-row items-center">
                        <a target="_blank" :href="healthCheck.links.repository" class="text-accent">
                            <div class="flex flex-row items-center">
                                <svg-icon
                                    type="mdi"
                                    class="mr-2"
                                    :path="mdiGit"
                                    :size="deviceStore.getREMSize(2.0)"
                                />
                                Git Repository
                            </div> </a
                        >&nbsp;- Submit Issues or Feature Requests.
                    </p>
                    <p class="mt-4 text-wrap flex flex-row items-center">
                        <a target="_blank" href="https://discord.gg/MbcgUFAfhV" class="text-accent">
                            <div class="flex flex-row items-center">
                                <span class="mr-2 pi pi-discord text-[2.0rem]" />
                                Discord
                            </div> </a
                        >&nbsp;- Join our Discord community.
                    </p>
                </div>
            </div>
        </ScrollAreaViewport>
        <ScrollAreaScrollbar
            class="flex select-none touch-none p-0.5 bg-transparent transition-colors duration-[120ms] ease-out data-[orientation=vertical]:w-2.5"
            orientation="vertical"
        >
            <ScrollAreaThumb
                class="flex-1 bg-border-one opacity-80 rounded-lg relative before:content-[''] before:absolute before:top-1/2 before:left-1/2 before:-translate-x-1/2 before:-translate-y-1/2 before:w-full before:h-full before:min-w-[44px] before:min-h-[44px]"
            />
        </ScrollAreaScrollbar>
    </ScrollAreaRoot>
</template>

<style scoped lang="scss">
.table-data {
    padding: 0.5rem;
    border: 1px solid rgb(var(--colors-border-one));
}
</style>
