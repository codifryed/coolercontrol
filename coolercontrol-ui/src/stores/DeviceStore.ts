// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import { defineStore } from 'pinia'
import { mdiAlertOutline } from '@mdi/js'
import { appendLogChunk, type LogLine } from '@/stores/logLines.ts'
import { Device, DeviceType, type UID } from '@/models/Device'
import DaemonClient from '@/stores/DaemonClient'
import { ChannelInfo } from '@/models/ChannelInfo'
import { DeviceResponseDTO, StatusResponseDTO } from '@/stores/DataTransferModels'
import { defineAsyncComponent, inject, Ref, ref, shallowRef, triggerRef } from 'vue'
import { useConfirm } from '@/shell/confirm'
import { useToast } from '@/shell/toast'
import { ErrorResponse } from '@/models/ErrorResponse'
import { useDialog } from '@/shell/dialog'
import { fetchEventSource } from '@microsoft/fetch-event-source'
import { plainToInstance } from 'class-transformer'
import { HealthCheck } from '@/models/HealthCheck.ts'
import { DaemonStatus, useDaemonState } from '@/stores/DaemonState.ts'
import { showLoadingOverlay } from '@/components/loadingOverlay.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { AlertLog, alertToast, AlertState } from '@/models/Alert.ts'
import {
    DeviceHealthDTO,
    FailsafeDelta,
    SourceDelta,
    UnreachableDelta,
} from '@/models/DeviceHealth.ts'
import { SystemEventDTO } from '@/models/PowerProfile.ts'
import { TempInfo } from '@/models/TempInfo.ts'
import { Emitter, EventType } from 'mitt'
import { ModeActivated } from '@/models/Mode.ts'
import { useI18n } from 'vue-i18n'
import { ChannelStatus, TempStatus } from '@/models/Status.ts'
import { PluginDto, HasUiDto } from '@/models/Plugins.ts'

/**
 * This is similar to the model_view in the old GUI, where it held global state for all the various hooks and accesses
 */
export interface ChannelValues {
    temp?: string
    rpm?: string
    duty?: string
    freq?: string
    watts?: string
}

export const DEFAULT_NAME_STRING_LENGTH: number = 40

export const useDeviceStore = defineStore('device', () => {
    // Internal properties that we don't want to be reactive (overhead) ------------------------------------------------
    const devices = new Map<UID, Device>()
    // const DEFAULT_DAEMON_ADDRESS = 'localhost'
    // const DEFAULT_DAEMON_PORT = 11987
    // const DEFAULT_DAEMON_SSL_ENABLED = false
    const CONFIG_DAEMON_ADDRESS = 'daemonAddress'
    const CONFIG_DAEMON_PORT = 'daemonPort'
    const CONFIG_DAEMON_SSL_ENABLED = 'daemonSslEnabled'
    let daemonClient = new DaemonClient(getDaemonAddress(), getDaemonPort(), getDaemonSslEnabled())
    daemonClient.setUnauthorizedCallback(unauthorizedCallback)
    const confirm = useConfirm()
    const passwordDialog = defineAsyncComponent(() => import('../components/PasswordDialog.vue'))
    const accessTokensDialog = defineAsyncComponent(
        () => import('../components/AccessTokensDialog.vue'),
    )
    const dialog = useDialog()
    const toast = useToast()
    const emitter: Emitter<Record<EventType, any>> = inject('emitter')!
    // This is the threshold for status timestamp differences, which is the only way to determine if the system has been suspended.
    const reloadAfterTimeoutThreshold: number = 30_000 // ms
    // This is the threshold for determining if the app has been suspended recently enough
    const reloadAfterShownThreshold: number = 7_000 // ms
    // We watch when the browser suspends or resumes the app, and use this to determine if we need to reload the UI
    let appShownTimestamp: number = 0 // start of the epoch
    let appHiddenTimestamp: number = 0 // start of the epoch
    const appStartTime = Date.now()
    const showLoginMessageThreshold: number = 3_000 // ms
    let chromeNetworkErrorCount: number = 0
    const chromeNetworkErrorThreshold: number = 7
    let sessionExpiredHandled: boolean = false
    // Set only when no service worker is carrying notifications, so they are never raised twice.
    let inPageNotificationsEnabled: boolean = false
    const { t } = useI18n()
    // -----------------------------------------------------------------------------------------------------------------

    // Reactive properties ------------------------------------------------

    const currentDeviceStatus = shallowRef(new Map<UID, Map<string, ChannelValues>>())
    const loggedIn: Ref<boolean> = ref(false)
    const isDefaultPasswd: Ref<boolean> = ref(true)
    const accessDenied: Ref<boolean> = ref(false)
    // Capped, pre-highlighted log lines (see logLines.ts).
    const logLines: Ref<LogLine[]> = ref([])
    const plugins: Ref<PluginDto[]> = ref([])
    const pluginUiInfo: Ref<Map<string, HasUiDto>> = ref(new Map())

    // Getters ---------------------------------------------------------------------------------------------------------
    function allDevices(): IterableIterator<Device> {
        return devices.values()
    }

    function sleep(ms: number): Promise<number> {
        return new Promise((r) => setTimeout(r, ms))
    }

    async function waitAndReload(wait_secs: number = 3): Promise<void> {
        showLoadingOverlay({ text: t('common.restarting') })
        let s = 0
        const daemonState = useDaemonState()
        while (s < 30) {
            s++
            // make sure daemon has re-connected before reloading
            if (s > wait_secs && daemonState.connected) {
                break
            }
            await sleep(1_000)
        }
        reloadUI(true)
    }

    function reloadUI(force: boolean = false): void {
        if (force && isQtApp()) {
            // call force refresh in the Qt App, which will clear the cache and reload the UI
            // @ts-ignore
            const ipc = window.ipc
            try {
                ipc.forceRefresh()
            } catch (_) {
                // This will catch the very first time this logic is used and the Qt app
                // hasn't been restarted yet. (delete later)
                console.error(
                    'Could not force refresh the UI. Please restart the desktop application.',
                )
                window.location.reload()
            }
        } else {
            window.location.reload()
        }
    }

    function toTitleCase(str: string): string {
        return str.replace(
            /\w\S*/g,
            (txt: string) => txt.charAt(0).toUpperCase() + txt.substring(1).toLowerCase(),
        )
    }

    function limitStringLength(str: string, limit: number): string {
        return str.substring(0, limit)
    }

    function sanitizeString(str: string, lengthLimit: number = DEFAULT_NAME_STRING_LENGTH): string {
        return limitStringLength(str.trim(), lengthLimit)
    }

    function round(value: number, precision: number = 0): number {
        const multiplier = Math.pow(10, precision)
        return Math.round(value * multiplier) / multiplier
    }

    function getREMSize(rem: number): number {
        const fontSize = window.getComputedStyle(document.querySelector('html')!).fontSize
        return parseFloat(fontSize) * rem
    }

    function isQtApp(): boolean {
        return 'ipc' in window
    }

    function isSafariWebKit(): boolean {
        return /apple computer/.test(navigator.vendor.toLowerCase())
    }

    function connectToQtIPC(): void {
        try {
            if (!('qt' in window)) {
                return
            }
            function loadScript(src: string, onload: () => void) {
                let script = document.createElement('script')
                // @ts-ignore
                script.onload = onload
                    ? onload
                    : function (e) {
                          // @ts-ignore
                          console.log(e.target.src + ' is loaded.')
                      }
                script.src = src
                script.async = false
                document.head.appendChild(script)
            }
            loadScript('qrc:///qtwebchannel/qwebchannel.js', (): void => {
                // @ts-ignore
                new QWebChannel(qt.webChannelTransport, async function (channel: any) {
                    // @ts-ignore
                    window.ipc = channel.objects.ipc
                    console.debug('Connected to Qt WebChannel, ready to send/receive messages!')
                })
            })
        } catch (e) {
            console.debug('Could not connect to Qt: ' + e)
        }
    }

    // Private methods ------------------------------------------------
    /**
     * Sorts the devices in the DeviceResponseDTO by first type, and then by typeIndex
     */
    function sortDevices(dto: DeviceResponseDTO): void {
        dto.devices.sort((a, b) => {
            const aTypeOrdinal = Object.values(DeviceType).indexOf(a.type)
            const bTypeOrdinal = Object.values(DeviceType).indexOf(b.type)
            if (aTypeOrdinal > bTypeOrdinal) {
                return 1
            } else if (aTypeOrdinal < bTypeOrdinal) {
                return -1
            } else if (a.type_index > b.type_index) {
                return 1
            } else if (a.type_index < b.type_index) {
                return -1
            } else {
                return 0
            }
        })
    }

    /**
     * Sorts channels by channel name
     */
    function sortChannels(device: Device): void {
        if (device.info?.channels) {
            device.info.channels = new Map<string, ChannelInfo>(
                [...device.info.channels.entries()].sort(([c1name, c1i], [c2name, c2i]) => {
                    // sort by channel type first, then by name
                    const channelTypeCompare = getChannelPrio(c1i) - getChannelPrio(c2i)
                    return channelTypeCompare === 0
                        ? c1name.localeCompare(c2name, undefined, {
                              numeric: true,
                              sensitivity: 'base',
                          })
                        : channelTypeCompare
                }),
            )
        }
        if (device.info?.temps) {
            device.info.temps = new Map<string, TempInfo>(
                [...device.info.temps.entries()].sort(([c1name], [c2name]) => {
                    return c1name.localeCompare(c2name, undefined, {
                        numeric: true,
                        sensitivity: 'base',
                    })
                }),
            )
        }
    }

    /**
     * Re-sorts the devices by menuOrder.
     * This must be called after the settingsStore is loaded.
     */
    function reSortDevicesByMenuOrder(allStatuses: boolean = false): void {
        const settingsStore = useSettingsStore()
        if (settingsStore.menuOrder.length > 0) {
            const deviceEntriesToSort = [...devices]
            const getDeviceIndex = (deviceEntry: [UID, Device]) => {
                const index = settingsStore.menuOrder.findIndex(
                    (menuItem) => menuItem.id === deviceEntry[0],
                )
                return index >= 0 ? index : Number.MAX_SAFE_INTEGER
            }
            deviceEntriesToSort.sort(
                (a: [UID, Device], b: [UID, Device]) => getDeviceIndex(a) - getDeviceIndex(b),
            )

            // Sort channels and temps:
            deviceEntriesToSort.forEach((deviceEntry: [UID, Device]) => {
                const menuOrderItem = settingsStore.menuOrder.find(
                    (item) => item.id === deviceEntry[0],
                )
                if (menuOrderItem?.children?.length) {
                    const device = deviceEntry[1]
                    const getIndex = (channelName: string) => {
                        const index = menuOrderItem.children.indexOf(`${device.uid}_${channelName}`)
                        return index >= 0 ? index : Number.MAX_SAFE_INTEGER
                    }
                    if (device.info?.channels) {
                        device.info.channels = new Map<string, ChannelInfo>(
                            [...device.info.channels.entries()].sort(
                                ([c1name, _c1i], [c2name, _c2i]) =>
                                    getIndex(c1name) - getIndex(c2name),
                            ),
                        )
                    }
                    if (device.info?.temps) {
                        device.info.temps = new Map<string, TempInfo>(
                            [...device.info.temps.entries()].sort(
                                ([c1name], [c2name]) => getIndex(c1name) - getIndex(c2name),
                            ),
                        )
                    }
                    if (allStatuses) {
                        device.status_history.forEach((status) => {
                            status.channels.sort(
                                (a: ChannelStatus, b: ChannelStatus) =>
                                    getIndex(a.name) - getIndex(b.name),
                            )
                            status.temps.sort(
                                (a: TempStatus, b: TempStatus) =>
                                    getIndex(a.name) - getIndex(b.name),
                            )
                        })
                    }
                }
            })
            devices.clear()
            for (const deviceEntry of deviceEntriesToSort) {
                devices.set(deviceEntry[0], deviceEntry[1])
            }
        }
    }

    function getChannelPrio(channelInfo: ChannelInfo): number {
        // freq, power, load, fans, lightings, lcds, others by name only
        // multiple channels of the same type are sorted by name numerically
        if (channelInfo.speed_options != null) {
            return 11
        } else if (channelInfo.lighting_modes.length > 0) {
            return 12
        } else if (channelInfo.lcd_info != null) {
            return 13
        } else if (channelInfo.label?.toLowerCase().includes('freq')) {
            return 1
        } else if (channelInfo.label?.toLowerCase().includes('power')) {
            return 2
        } else if (channelInfo.label?.toLowerCase().includes('load')) {
            return 3
        }
        return 14
    }

    async function unauthorizedCallback(error: any): Promise<void> {
        if (error.response?.status === 401) {
            handleSessionExpired()
        }
    }

    function handleSessionExpired(): void {
        if (sessionExpiredHandled) return
        sessionExpiredHandled = true
        loggedIn.value = false
        toast.add({
            severity: 'warn',
            summary: t('device_store.unauthorized.summary'),
            detail: t('device_store.unauthorized.detail'),
            life: 3000,
        })
        setTimeout(() => reloadUI(true), 500)
    }

    async function requestPasswd(retryCount: number = 1): Promise<boolean> {
        const thisStore = useDeviceStore()
        if (thisStore.isQtApp()) {
            // The desktop app window should be shown if login fails
            // @ts-ignore
            const ipc = window.ipc
            await ipc.loadFinished()
            await ipc.forceShow()
        }
        return new Promise((resolve) => {
            dialog.open(passwordDialog, {
                props: {
                    header: t('auth.enterPassword'),
                    position: 'center',
                    modal: false,
                    dismissableMask: false,
                    closable: false,
                },
                data: {
                    setPasswd: false,
                },
                onClose: (options: any) => {
                    // Defer async work to allow dialog to fully close and clean up modal mask
                    setTimeout(async () => {
                        if (options.data && options.data.passwd) {
                            const loginResult = await daemonClient.login(options.data.passwd)
                            if (loginResult === true) {
                                loggedIn.value = true
                                isDefaultPasswd.value = false
                                localStorage.setItem('isDefaultPasswd', 'false')
                                console.info('Login successful')
                                resolve(true)
                                return
                            }
                            if (loginResult.status === 429) {
                                toast.add({
                                    severity: 'warn',
                                    summary: t('device_store.login.rate_limited.summary'),
                                    detail: loginResult.error,
                                    life: 30000,
                                })
                                accessDenied.value = true
                                resolve(false)
                                return
                            }
                            toast.add({
                                severity: 'error',
                                summary: t('device_store.login.failed.summary'),
                                detail: t('device_store.login.failed.detail'),
                                life: 3000,
                            })
                            if (retryCount > 2) {
                                console.info('Login failed')
                                accessDenied.value = true
                                resolve(false)
                                return
                            }
                            resolve(await requestPasswd(++retryCount))
                            return
                        }
                        // Dialog closed without entering password
                        accessDenied.value = true
                        resolve(false)
                    }, 0)
                },
            })
        })
    }

    function getDaemonAddress(): string {
        // const defaultAddress: string = isQtApp()
        //     ? DEFAULT_DAEMON_ADDRESS
        //     : window.location.hostname
        const defaultAddress: string = window.location.hostname
        return localStorage.getItem(CONFIG_DAEMON_ADDRESS) || defaultAddress
    }

    function setDaemonAddress(address: string): void {
        localStorage.setItem(CONFIG_DAEMON_ADDRESS, address)
    }

    function clearDaemonAddress(): void {
        localStorage.removeItem(CONFIG_DAEMON_ADDRESS)
    }

    function getDaemonPort(): number {
        // const defaultPort: string = isQtApp()
        //     ? DEFAULT_DAEMON_PORT.toString()
        //     : window.location.port || (window.location.protocol === 'https:' ? '443' : '80')
        const defaultPort: string =
            window.location.port || (window.location.protocol === 'https:' ? '443' : '80')
        return parseInt(localStorage.getItem(CONFIG_DAEMON_PORT) || defaultPort)
    }

    function setDaemonPort(port: number): void {
        localStorage.setItem(CONFIG_DAEMON_PORT, port.toString())
    }

    function clearDaemonPort(): void {
        localStorage.removeItem(CONFIG_DAEMON_PORT)
    }

    function getDaemonSslEnabled(): boolean {
        // const defaultSslEnabled: boolean = isQtApp()
        //     ? DEFAULT_DAEMON_SSL_ENABLED
        //     : window.location.protocol === 'https:'
        const defaultSslEnabled: boolean = window.location.protocol === 'https:'
        return localStorage.getItem(CONFIG_DAEMON_SSL_ENABLED) != null
            ? localStorage.getItem(CONFIG_DAEMON_SSL_ENABLED) === 'true'
            : defaultSslEnabled
    }

    function setDaemonSslEnabled(sslEnabled: boolean): void {
        localStorage.setItem(CONFIG_DAEMON_SSL_ENABLED, sslEnabled.toString())
    }

    function clearDaemonSslEnabled(): void {
        localStorage.removeItem(CONFIG_DAEMON_SSL_ENABLED)
    }

    function appAgeMilli(): number {
        return Date.now() - appStartTime
    }

    function isChromeNetworkError(err: any): boolean {
        return err.message.includes('network error')
    }

    // Actions -----------------------------------------------------------------------
    async function login(): Promise<boolean> {
        const sessionIsValid = await daemonClient.sessionIsValid()
        if (sessionIsValid) {
            loggedIn.value = true
            // The daemon's verify-session endpoint guarantees the password is not default
            // when it returns success, so we trust it over localStorage which may be stale.
            isDefaultPasswd.value = false
            localStorage.setItem('isDefaultPasswd', 'false')
            console.info('Login Session still valid')
            // Do not show this message on startup:
            if (appAgeMilli() > showLoginMessageThreshold) {
                toast.add({
                    severity: 'info',
                    summary: t('layout.topbar.login'),
                    detail: t('layout.topbar.loginSuccessful'),
                    life: 1500,
                })
            }
            if (isDefaultPasswd.value) {
                await promptDefaultPasswdChange()
            }
            return true
        }
        const defaultLoginResult = await daemonClient.login()
        if (defaultLoginResult === true) {
            loggedIn.value = true
            isDefaultPasswd.value = true
            localStorage.setItem('isDefaultPasswd', 'true')
            console.info('Login successful')
            // Do not show this message on startup:
            if (appAgeMilli() > showLoginMessageThreshold) {
                toast.add({
                    severity: 'info',
                    summary: t('layout.topbar.login'),
                    detail: t('layout.topbar.loginSuccessful'),
                    life: 1500,
                })
            }
            await promptDefaultPasswdChange()
            return true
        }
        if (defaultLoginResult.status === 429) {
            toast.add({
                severity: 'warn',
                summary: t('device_store.login.rate_limited.summary'),
                detail: defaultLoginResult.error,
                life: 8000,
            })
            accessDenied.value = true
            return false
        }
        return await requestPasswd()
    }

    async function setPasswd(): Promise<void> {
        dialog.open(passwordDialog, {
            props: {
                header: t('auth.setNewPassword'),
                position: 'center',
                modal: true,
                dismissableMask: false,
            },
            data: {
                setPasswd: true,
                onVerifyCurrentPassword: async (currentPasswd: string): Promise<string | null> => {
                    const result = await daemonClient.setPasswd(currentPasswd, currentPasswd)
                    if (result instanceof ErrorResponse) {
                        return result.error
                    }
                    await daemonClient.login(currentPasswd)
                    return null
                },
                onSubmit: async (currentPasswd: string, passwd: string): Promise<string | null> => {
                    const response = await daemonClient.setPasswd(currentPasswd, passwd)
                    if (response instanceof ErrorResponse) {
                        return response.error
                    }
                    isDefaultPasswd.value = false
                    localStorage.setItem('isDefaultPasswd', 'false')
                    await daemonClient.login(passwd)
                    toast.add({
                        severity: 'success',
                        summary: t('device_store.password.set_success.summary'),
                        detail: t('device_store.password.set_success.detail'),
                        life: 3000,
                    })
                    return null
                },
            },
            onClose: async (_options: any) => {},
        })
    }

    async function manageTokens(): Promise<void> {
        dialog.open(accessTokensDialog, {
            props: {
                header: t('auth.accessTokens'),
                position: 'center',
                modal: true,
                dismissableMask: false,
            },
        })
    }

    async function promptDefaultPasswdChange(): Promise<void> {
        const thisStore = useDeviceStore()
        if (thisStore.isQtApp()) {
            // The desktop app window should be shown if user needs to set a new password
            // @ts-ignore
            const ipc = window.ipc
            await ipc.loadFinished()
            await ipc.forceShow()
        }
        return new Promise((resolve) => {
            dialog.open(passwordDialog, {
                props: {
                    header: t('auth.setNewPassword'),
                    position: 'center',
                    modal: false,
                    dismissableMask: false,
                    closable: false,
                },
                data: {
                    setPasswd: true,
                    currentPasswd: daemonClient.defaultPasswd,
                    promptMessage: t('auth.changeDefaultPassword'),
                    onSubmit: async (
                        currentPasswd: string,
                        passwd: string,
                    ): Promise<string | null> => {
                        const response = await daemonClient.setPasswd(currentPasswd, passwd)
                        if (response instanceof ErrorResponse) {
                            return response.error
                        }
                        isDefaultPasswd.value = false
                        localStorage.setItem('isDefaultPasswd', 'false')
                        await daemonClient.login(passwd)
                        toast.add({
                            severity: 'success',
                            summary: t('device_store.password.set_success.summary'),
                            detail: t('device_store.password.set_success.detail'),
                            life: 3000,
                        })
                        return null
                    },
                },
                onClose: (_options: any) => {
                    resolve()
                },
            })
        })
    }

    async function logout(): Promise<void> {
        await daemonClient.logout()
        loggedIn.value = false
        console.info('Admin Logged Out')
        toast.add({
            severity: 'info',
            summary: t('device_store.logout.summary'),
            detail: t('device_store.logout.detail'),
            life: 3000,
        })
    }

    async function health(): Promise<HealthCheck> {
        return await daemonClient.health()
    }

    async function loadLogs(): Promise<void> {
        logLines.value = appendLogChunk([], await daemonClient.logs())
    }

    async function acknowledgeIssues(): Promise<void> {
        return await daemonClient.acknowledgeIssues()
    }

    async function handshake(): Promise<boolean> {
        console.info('Performing Handshake')
        return await daemonClient.handshake()
    }

    async function initializeDevices(): Promise<boolean> {
        console.info('Initializing Devices')
        const dto = await daemonClient.requestDevices()
        if (dto.devices.length === 0) {
            console.warn('There are no available devices!')
        }
        sortDevices(dto)
        for (const device of dto.devices) {
            if (device.lc_info?.unknown_asetek) {
                // wait until the Onboarding dialog isn't open without blocking:
                setTimeout(async () => {
                    const settingsStore = useSettingsStore()
                    while (settingsStore.showOnboarding) {
                        await sleep(1000)
                    }
                    confirm.require({
                        group: 'AseTek690',
                        message: `${device.type_index}`,
                        header: t('device_store.asetek.header'),
                        icon: mdiAlertOutline,
                        acceptLabel: t('components.aseTek690.acceptLabel'),
                        rejectLabel: t('components.aseTek690.rejectLabel'),
                        accept: async () => {
                            console.debug(`Setting device ${device.uid} as a Legacy 690`)
                            await handleAseTekResponse(device.uid, true)
                        },
                        reject: async () => {
                            console.debug(`Setting device ${device.uid} as a EVGA CLC`)
                            await handleAseTekResponse(device.uid, false)
                        },
                    })
                })
            }
            sortChannels(device)
            devices.set(device.uid, device)
        }
        await loadCompleteStatusHistory()
        console.debug('Initialized with devices:')
        console.debug(devices)
        return true
    }

    async function loadAllPlugins(): Promise<void> {
        const pluginsDto = await daemonClient.loadPlugins()
        plugins.value = pluginsDto.plugins
        const uiMap = new Map<string, HasUiDto>()
        for (const plugin of pluginsDto.plugins) {
            const uiInfo = await daemonClient.hasPluginUi(plugin.id)
            uiMap.set(plugin.id, uiInfo)
        }
        pluginUiInfo.value = uiMap
        console.debug('Loaded plugins:', plugins.value)
    }

    async function handleAseTekResponse(deviceUID: UID, isLegacy690: boolean): Promise<void> {
        const response = await daemonClient.setAseTekDeviceType(deviceUID, isLegacy690)
        if (response instanceof ErrorResponse) {
            toast.add({
                severity: 'error',
                summary: t('device_store.asetek.error.summary'),
                detail: response.error + ' - ' + t('device_store.asetek.error.detail'),
                life: 4000,
            })
            return
        }
        const msg = isLegacy690
            ? t('device_store.asetek.success.detail_legacy')
            : t('device_store.asetek.success.detail_evga')
        toast.add({
            severity: 'success',
            summary: t('device_store.asetek.success.summary'),
            detail: msg,
            life: 3000,
        })
        if (isLegacy690) {
            await daemonClient.shutdownDaemon()
            await waitAndReload()
        }
    }

    /**
     * requests and loads all the statuses for each device.
     */
    async function loadCompleteStatusHistory(): Promise<void> {
        const allStatusesDto = await daemonClient.completeStatusHistory()
        for (const dtoDevice of allStatusesDto.devices) {
            // not all device UIDs are present locally (composite can be ignored for example)
            if (devices.has(dtoDevice.uid)) {
                const statuses = devices.get(dtoDevice.uid)!.status_history
                statuses.length = 0 // clear array if this is a re-sync
                statuses.push(...dtoDevice.status_history) // shallow copy
            }
        }
        updateRecentDeviceStatus()
    }

    /**
     * Requests the most recent status for all devices and adds it to the current status array.
     * @return boolean true if only the most recent status was updated. False if all statuses were updated.
     */
    async function updateStatus(dto: StatusResponseDTO): Promise<boolean> {
        let onlyLatestStatus: boolean = true
        let timeDiffMillis: number = 0
        // now handled by server side events:
        // const dto = await daemonClient.recentStatus()
        if (dto.devices.length === 0 || dto.devices[0].status_history.length === 0) {
            // Since the introduction of SSEs, this shouldn't happen anymore as the
            // status history is filled, and we're no longer polling for data.
            console.error("Device Statuses are empty, this shouldn't happen anymore.")
            return onlyLatestStatus // we can't update anything without data
        }
        if (devices.size > 0) {
            const device: Device = devices.values().next().value! // get the first device's timestamp
            timeDiffMillis = Math.abs(
                new Date(device.status.timestamp).getTime() -
                    new Date(dto.devices[0].status_history[0].timestamp).getTime(),
            )
            // UI Reload use cases:
            // - Daemon Disconnected (SSE closed - auto SSE retry every second - reload on reconnect)
            //   - Daemon disconnect is handled below by `daemonState.connected` logic
            // - App Recently Hidden (doc event - hidden = true, just take a timestamp - start of app suspension)
            // - App Recently Shown (doc event - hidden = false, if suspended for above the threshold time, reload))
            //   - App browser suspension is handled near the bottom by watching for the 'visibilitychange' event
            // - System Sleep (no noticeable event - time difference between last status and current status exceeds threshold (30 seconds) then Reload)
            //   - System sleep is handled here (no event/time difference):
            if (timeDiffMillis > reloadAfterTimeoutThreshold) {
                onlyLatestStatus = false
            }
        }

        if (onlyLatestStatus) {
            sortAllStatusHistory(dto)
            for (const dtoDevice of dto.devices) {
                // not all device UIDs are present locally (composite can be ignored for example)
                if (devices.has(dtoDevice.uid)) {
                    const statuses = devices.get(dtoDevice.uid)!.status_history
                    statuses.push(...dtoDevice.status_history)
                    statuses.shift()
                }
            }
            updateRecentDeviceStatus()
        } else {
            const daemonState = useDaemonState()
            if (!daemonState.connected) {
                // On daemon reconnect a full UI refresh is done.
                console.info(
                    `[${new Date().toUTCString()}]:\nDevice Statuses are out of sync by ${new Intl.NumberFormat().format(
                        timeDiffMillis,
                    )}ms. Daemon was disconnected, full UI reload incoming.`,
                )
                // returning true will prevent a full data request, since the UI will reload anyway:
                return true
            }
            console.info(
                `[${new Date().toUTCString()}]:\nDevice Statuses are out of sync by ${new Intl.NumberFormat().format(
                    timeDiffMillis,
                )}ms, reloading all states and statuses.`,
            )
            // Previously we attempted to only refresh the status data, but there are issues with
            // array order of the graph and other edge cases when doing this. To avoid having to
            // handle these edge cases throughout the application, we do a full refresh instead.
            // This doesn't happen very often, and is pretty fast for the majority of systems.
            reloadUI()
            // const settingsStore = useSettingsStore()
            // await settingsStore.loadAlertsAndLogs()
            // await settingsStore.getActiveModes()
            // const healthCheck = await health()
            // daemonState.warnings = healthCheck.details.warnings
            // daemonState.errors = healthCheck.details.errors
            // if (daemonState.errors > 0) {
            //     await daemonState.setStatus(DaemonStatus.ERROR)
            // } else if (daemonState.warnings > 0) {
            //     await daemonState.setStatus(DaemonStatus.WARN)
            // } else {
            //     await daemonState.setStatus(DaemonStatus.OK)
            // }
            // await loadLogs()
            // await loadCompleteStatusHistory()
        }
        return onlyLatestStatus
    }

    /**
     * Sorts the status history of all devices so that all consumers have the right order.
     * @param allStatusesDto
     */
    function sortAllStatusHistory(allStatusesDto: StatusResponseDTO) {
        const settingsStore = useSettingsStore()
        for (const dtoDevice of allStatusesDto.devices) {
            const menuOrderItem = settingsStore.menuOrder.find((item) => item.id === dtoDevice.uid)
            if (menuOrderItem?.children?.length) {
                const getIndex = (channelName: string) => {
                    const index = menuOrderItem.children.indexOf(`${dtoDevice.uid}_${channelName}`)
                    return index >= 0 ? index : Number.MAX_SAFE_INTEGER
                }
                dtoDevice.status_history.forEach((status) => {
                    status.channels.sort(
                        (a: ChannelStatus, b: ChannelStatus) => getIndex(a.name) - getIndex(b.name),
                    )
                    status.temps.sort(
                        (a: TempStatus, b: TempStatus) => getIndex(a.name) - getIndex(b.name),
                    )
                })
            }
        }
    }

    /**
     * One connection carrying every daemon event kind. The daemon multiplexes them onto
     * /sse and tags each with its own event name. Browsers cap concurrent connections per
     * origin over HTTP/1.1 (6 in Chrome, counted per profile rather than per tab), so one
     * stream per event kind starved the UI's own REST calls once a second tab was open.
     */
    async function updateFromSSE(): Promise<void> {
        const thisStore = useDeviceStore()
        const daemonState = useDaemonState()
        const settingsStore = useSettingsStore()

        async function handleStatus(data: string): Promise<void> {
            const dto = plainToInstance(StatusResponseDTO, JSON.parse(data) as object)
            await thisStore.updateStatus(dto)
            daemonState.noteStatusReceived()
            await daemonState.setConnected(true)
            chromeNetworkErrorCount = 0
        }

        async function handleLog(newLog: string): Promise<void> {
            logLines.value = appendLogChunk(logLines.value, newLog)
            if (newLog.includes('ERROR')) {
                await daemonState.setStatus(DaemonStatus.ERROR)
            } else if (newLog.includes('WARN')) {
                if (daemonState.status !== DaemonStatus.ERROR) {
                    await daemonState.setStatus(DaemonStatus.WARN)
                }
            }
        }

        async function handleMode(data: string): Promise<void> {
            const modeMessage = plainToInstance(ModeActivated, JSON.parse(data) as object)
            settingsStore.modeActiveCurrent = modeMessage.uid
            settingsStore.modeActivePrevious = modeMessage.previous_uid
            await settingsStore.loadDaemonDeviceSettings() // need to reload all settings after applying mode
            emitter.emit('active-modes-change-menu')
        }

        function handleAlert(data: string): void {
            const alertMessage = plainToInstance(AlertLog, JSON.parse(data) as object)
            console.debug('Received Alert: ', alertMessage)
            settingsStore.alertLogs.push(alertMessage)
            let foundAlert = settingsStore.alerts.find((alert) => alert.uid === alertMessage.uid)
            if (foundAlert) {
                foundAlert.state = alertMessage.state
            }
            settingsStore.clearAlertAttention(alertMessage.uid)
            if (alertMessage.state === AlertState.Active) {
                settingsStore.alertsActive.push(alertMessage.uid)
            } else if (alertMessage.state === AlertState.Error) {
                settingsStore.alertsError.push(alertMessage.uid)
            }
            // Silenced and quiet changes still update state; only the toast is muted.
            const decision = alertToast(alertMessage)
            if (decision == null) return
            toast.add({
                severity: decision.severity,
                summary: t(decision.summaryKey),
                detail: `${alertMessage.name} - ${alertMessage.message}`,
                life: decision.life,
            })
        }

        async function handleEvent(eventName: string, data: string): Promise<void> {
            switch (eventName) {
                case 'status':
                    await handleStatus(data)
                    return
                // Device-health transitions arrive as one batch of deltas per subject per tick.
                case 'missing':
                    for (const delta of plainToInstance(
                        SourceDelta,
                        JSON.parse(data) as Array<object>,
                    )) {
                        settingsStore.applyMissingDelta(delta)
                    }
                    return
                case 'stale-source':
                    for (const delta of plainToInstance(
                        SourceDelta,
                        JSON.parse(data) as Array<object>,
                    )) {
                        settingsStore.applyStaleSourceDelta(delta)
                    }
                    return
                case 'failsafe':
                    for (const delta of plainToInstance(
                        FailsafeDelta,
                        JSON.parse(data) as Array<object>,
                    )) {
                        settingsStore.applyFailsafeDelta(delta)
                    }
                    return
                case 'unreachable':
                    for (const delta of plainToInstance(
                        UnreachableDelta,
                        JSON.parse(data) as Array<object>,
                    )) {
                        settingsStore.applyUnreachableDelta(delta)
                    }
                    return
                // Full-state resync the daemon sends when this client lagged the
                // health broadcast and missed transition batches.
                case 'health':
                    settingsStore.applyDeviceHealthSnapshot(
                        plainToInstance(DeviceHealthDTO, JSON.parse(data) as object),
                    )
                    return
                case 'log':
                    await handleLog(data)
                    return
                case 'mode':
                    await handleMode(data)
                    return
                case 'alert':
                    handleAlert(data)
                    return
                case 'system':
                    settingsStore.applySystemEvent(
                        plainToInstance(SystemEventDTO, JSON.parse(data) as object),
                    )
                    return
                case 'notification':
                    // Only when the service worker is not carrying them for us.
                    if (inPageNotificationsEnabled) showDesktopNotification(data)
                    return
                default:
                    // An event kind this client does not know yet. Ignore it rather than
                    // mis-handling it as a status tick.
                    console.debug(`Ignoring unknown SSE event: ${eventName}`)
            }
        }

        async function startSSE(): Promise<void> {
            await fetchEventSource(`${daemonClient.daemonURL}sse`, {
                credentials: 'include',
                async onopen(response) {
                    if (response.ok) return
                    if (response.status === 401) {
                        throw new Error('Unauthorized')
                    }
                    throw new Error(`SSE error: ${response.status}`)
                },
                async onmessage(event) {
                    if (event.data.length === 0) return // keep-alive message
                    await handleEvent(event.event, event.data)
                },
                async onclose() {
                    // attempt to re-establish connection automatically (resume/restart)
                    await daemonState.setConnected(false)
                    thisStore.loggedIn = false
                    chromeNetworkErrorCount = 0
                    await sleep(1000)
                    await startSSE()
                },
                // @ts-ignore
                // changing onerror to async causes spam retry loop
                onerror(err: any) {
                    if (
                        isChromeNetworkError(err) &&
                        chromeNetworkErrorCount < chromeNetworkErrorThreshold
                    ) {
                        // net::ERR_NETWORK_CHANGED
                        // https://issues.chromium.org/issues/41465264
                        // There is an issue with docker and chrome, where chrome interprets
                        // docker network activity as a network change, and throws a network error.
                        console.warn('Chrome Network error. Retrying...')
                        chromeNetworkErrorCount++
                        return
                    }
                    if (err.message === 'Unauthorized') {
                        console.warn('SSE returned 401 - reloading for re-authentication.')
                        thisStore.handleSessionExpired()
                        throw err // stop retries
                    }
                    daemonState.setConnected(false)
                    thisStore.loggedIn = false
                    // auto-retry every second (return number to specify retry interval)
                },
            })
        }
        console.info('Listening for Daemon Events')
        return await startSSE()
    }

    async function initNotificationWorker(): Promise<void> {
        if (isQtApp()) {
            // The Qt app handles notifications via coolercontrold notify subprocess.
            return
        }
        if (!('Notification' in window)) {
            console.warn('Notification API not supported; browser notifications disabled.')
            return
        }
        const permission = await Notification.requestPermission()
        if (permission !== 'granted') {
            console.info('Notification permission denied by user.')
            return
        }
        // Try Service Worker first for background-independent notifications.
        // Falls back to the page's own stream when SW is unavailable (self-signed SSL,
        // insecure context, or browser without SW support).
        // It subscribes to notifications alone: the full stream would push a status tick
        // per second at it forever just to catch the occasional notification.
        const notificationsUrl = `${daemonClient.daemonURL}sse?events=notifications`
        if ('serviceWorker' in navigator) {
            try {
                const registration = await navigator.serviceWorker.register('/notification-sw.js')
                const sw = registration.active || registration.waiting || registration.installing
                if (sw && sw.state === 'activated') {
                    sw.postMessage({ type: 'start', url: notificationsUrl })
                } else if (sw) {
                    sw.addEventListener('statechange', function listener() {
                        if (sw.state === 'activated') {
                            sw.removeEventListener('statechange', listener)
                            sw.postMessage({ type: 'start', url: notificationsUrl })
                        }
                    })
                }
                console.info('Notification Service Worker registered.')
                return
            } catch (err) {
                console.warn('Service Worker registration failed, using in-page fallback:', err)
            }
        }
        // No worker, so raise notifications from the page's own stream instead.
        inPageNotificationsEnabled = true
        console.info('Handling notifications in-page.')
    }

    const NOTIFICATION_ICON_MAP: Record<string, string> = {
        triggered: '/icons/alert-triggered.png',
        resolved: '/icons/alert-resolved.png',
        error: '/icons/alert-error.png',
        info: '/icons/information.png',
        shutdown: '/icons/shutdown.png',
    }

    // Raised from the shared /sse stream when no service worker is carrying them.
    function showDesktopNotification(data: string): void {
        try {
            const notification = JSON.parse(data)
            new Notification(notification.title || 'CoolerControl', {
                body: notification.body || '',
                icon: NOTIFICATION_ICON_MAP[notification.icon] || NOTIFICATION_ICON_MAP['info'],
                silent: !notification.audio,
                requireInteraction: notification.urgency >= 2,
            })
        } catch (_) {
            // Ignore malformed messages.
        }
    }

    function updateRecentDeviceStatus(): void {
        for (const [uid, device] of devices) {
            if (!currentDeviceStatus.value.has(uid)) {
                currentDeviceStatus.value.set(uid, new Map<string, ChannelValues>())
            }
            let deviceStatuses = currentDeviceStatus.value.get(uid)!
            for (const temp of device.status.temps) {
                if (deviceStatuses.has(temp.name)) {
                    deviceStatuses.get(temp.name)!.temp = temp.temp.toFixed(1)
                } else {
                    deviceStatuses.set(temp.name, { temp: temp.temp.toFixed(1) })
                }
            }
            for (const channel of device.status.channels) {
                // This gives us both "load" and "speed" channels
                if (deviceStatuses.has(channel.name)) {
                    deviceStatuses.get(channel.name)!.duty = channel.duty?.toFixed(0)
                    deviceStatuses.get(channel.name)!.rpm = channel.rpm?.toFixed(0)
                    deviceStatuses.get(channel.name)!.freq = channel.freq?.toFixed(0)
                    deviceStatuses.get(channel.name)!.watts = channel.watts?.toFixed(1)
                } else {
                    deviceStatuses.set(channel.name, {
                        duty: channel.duty?.toFixed(0),
                        rpm: channel.rpm?.toFixed(0),
                        freq: channel.freq?.toFixed(0),
                        watts: channel.watts?.toFixed(1),
                    })
                }
            }
        }
        triggerRef(currentDeviceStatus)
    }

    document.addEventListener('visibilitychange', () => {
        if (document.hidden) {
            // app suspension starts shortly after this
            appHiddenTimestamp = Date.now()
        } else {
            // app resume starts when the app has been hidden for a while (a second is too short to suspend)
            appShownTimestamp = Date.now()
            if (appShownTimestamp - appHiddenTimestamp > reloadAfterShownThreshold) {
                console.info(`App resumed after ${reloadAfterShownThreshold}ms, reloading UI`)
                reloadUI()
            }
        }
    })

    console.debug(`Device Store created`)
    return {
        daemonClient,
        allDevices,
        sleep,
        waitAndReload,
        reloadUI,
        toTitleCase,
        getDaemonAddress,
        setDaemonAddress,
        clearDaemonAddress,
        getDaemonPort,
        setDaemonPort,
        clearDaemonPort,
        getDaemonSslEnabled,
        setDaemonSslEnabled,
        clearDaemonSslEnabled,
        handshake,
        login,
        logout,
        handleSessionExpired,
        health,
        logLines,
        acknowledgeIssues,
        loadLogs,
        setPasswd,
        manageTokens,
        initializeDevices,
        loggedIn,
        isDefaultPasswd,
        accessDenied,
        updateStatus,
        updateFromSSE,
        initNotificationWorker,
        currentDeviceStatus,
        round,
        sanitizeString,
        getREMSize,
        isQtApp,
        isSafariWebKit,
        connectToQtIPC,
        reSortDevicesByMenuOrder,
        plugins,
        pluginUiInfo,
        loadAllPlugins,
    }
})
