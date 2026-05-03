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
import { RouterView, useRouter } from 'vue-router'
import { Ref, onMounted, ref, inject, nextTick } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import Button from 'primevue/button'
import Toast from 'primevue/toast'
import ConfirmDialog from 'primevue/confirmdialog'
import Dialog from 'primevue/dialog'
import DynamicDialog from 'primevue/dynamicdialog'
import InputNumber from 'primevue/inputnumber'
import InputText from 'primevue/inputtext'
import { ElLoading, ElSwitch } from 'element-plus'
import 'element-plus/es/components/loading/style/css'
import { StartupPage, ThemeMode } from '@/models/UISettings.ts'
import { useDaemonState } from '@/stores/DaemonState.ts'
import { VOnboardingWrapper, VOnboardingStep, useVOnboarding } from 'v-onboarding'
import { Emitter, EventType } from 'mitt'
import { svgLoader, svgLoaderBackground, svgLoaderViewBox } from '@/models/Loader.ts'
import FloatLabel from 'primevue/floatlabel'
import { useI18n } from 'vue-i18n'

const { t, locale } = useI18n({ useScope: 'global' })
const loaded: Ref<boolean> = ref(false)
const initSuccessful = ref(true)
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const daemonState = useDaemonState()
const router = useRouter()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!

const daemonPort: Ref<number> = ref(deviceStore.getDaemonPort())
const daemonAddress: Ref<string> = ref(deviceStore.getDaemonAddress())
const daemonSslEnabled: Ref<boolean> = ref(deviceStore.getDaemonSslEnabled())
const saveDaemonSettings = () => {
    deviceStore.setDaemonAddress(daemonAddress.value)
    deviceStore.setDaemonPort(daemonPort.value)
    deviceStore.setDaemonSslEnabled(daemonSslEnabled.value)
    deviceStore.reloadUI()
}
const resetDaemonSettings = () => {
    deviceStore.clearDaemonAddress()
    deviceStore.clearDaemonPort()
    deviceStore.clearDaemonSslEnabled()
    deviceStore.reloadUI()
}
const applyCustomTheme = (): void => {
    if (settingsStore.themeMode !== ThemeMode.CUSTOM) return
    if (settingsStore.customTheme.accent) {
        document.documentElement.style.setProperty(
            '--colors-accent',
            settingsStore.customTheme.accent,
        )
    }
    if (settingsStore.customTheme.bgOne) {
        document.documentElement.style.setProperty(
            '--colors-bg-one',
            settingsStore.customTheme.bgOne,
        )
    }
    if (settingsStore.customTheme.bgTwo) {
        document.documentElement.style.setProperty(
            '--colors-bg-two',
            settingsStore.customTheme.bgTwo,
        )
    }
    if (settingsStore.customTheme.borderOne) {
        document.documentElement.style.setProperty(
            '--colors-border-one',
            settingsStore.customTheme.borderOne,
        )
    }
    if (settingsStore.customTheme.textColor) {
        document.documentElement.style.setProperty(
            '--colors-text-color',
            settingsStore.customTheme.textColor,
        )
    }
    if (settingsStore.customTheme.textColorSecondary) {
        document.documentElement.style.setProperty(
            '--colors-text-color-secondary',
            settingsStore.customTheme.textColorSecondary,
        )
    }
}

const onboardingWrapper = ref(null)
const { start, finish } = useVOnboarding(onboardingWrapper)

type TourMode = 'basic' | 'thorough'
const tourMode: Ref<TourMode | null> = ref(null)
const steps: Ref<any[]> = ref([])
const isTransitioningMode = ref(false)

const POPPER_RIGHT = { popper: { placement: 'right' } }
const GETTING_STARTED_URL =
    'https://docs.coolercontrol.org/getting-started.html#%F0%9F%A7%99-configure'

const makeStep = (selector: string, key: string): any => ({
    attachTo: { element: selector },
    content: {
        title: t(`components.onboarding.${key}`),
        description: t(`components.onboarding.${key}Desc`),
    },
    options: POPPER_RIGHT,
})

const welcomeStep = (): any => ({
    attachTo: { element: '#logo' },
    content: { kind: 'welcome' },
    options: POPPER_RIGHT,
})

const finishStep = (): any => ({
    attachTo: { element: '#logo' },
    content: { kind: 'finish' },
    options: POPPER_RIGHT,
})

const filterPresent = (list: any[]): any[] =>
    list.filter((s) => document.querySelector(s.attachTo.element) !== null)

//todo: reorder and add:

const buildBasicSteps = (): any[] =>
    filterPresent([
        makeStep('#logo', 'appInfo'),
        makeStep('#add', 'quickAdd'),
        makeStep('#dashboard-quick', 'dashboardQuick'),
        makeStep('#controls', 'controls'),
        makeStep('#alerts-quick', 'alertsQuick'),
        makeStep('#settings', 'settings'),
        makeStep('#restart', 'restartMenu'),
        makeStep('#system-menu', 'systemMenu'),
        makeStep('#profiles', 'profiles'),
        makeStep('#functions', 'functions'),
        finishStep(),
    ])

const buildThoroughSteps = (): any[] =>
    filterPresent([
        makeStep('#logo', 'appInfo'),
        makeStep('#add', 'quickAdd'),
        makeStep('#dashboard-quick', 'dashboardQuick'),
        makeStep('#controls', 'controls'),
        makeStep('#modes-quick', 'modesQuick'),
        makeStep('#collapse-menu', 'collapseMenu'),
        makeStep('#alerts-quick', 'alertsQuick'),
        makeStep('#plugins-quick', 'pluginsQuick'),
        makeStep('#settings', 'settings'),
        makeStep('#access', 'access'),
        makeStep('#restart', 'restartMenu'),
        makeStep('#system-menu', 'systemMenu'),
        makeStep('#dashboards', 'dashboards'),
        makeStep('#profiles', 'profiles'),
        makeStep('#functions', 'functions'),
        makeStep('#modes', 'modes'),
        makeStep('#alerts', 'alerts'),
        makeStep('[data-tour-anchor="custom-sensors"]', 'customSensors'),
        finishStep(),
    ])

const startTour = (): void => {
    tourMode.value = null
    steps.value = [welcomeStep()]
    start()
}

const chooseTourMode = async (mode: TourMode): Promise<void> => {
    tourMode.value = mode
    isTransitioningMode.value = true
    finish()
    steps.value = mode === 'thorough' ? buildThoroughSteps() : buildBasicSteps()
    await nextTick()
    start()
    await nextTick()
    isTransitioningMode.value = false
}

const skipTour = (): void => {
    finish()
}

const openGettingStartedDocs = (): void => {
    window.open(GETTING_STARTED_URL, '_blank')
    finish()
}

const onTourFinished = (): void => {
    if (isTransitioningMode.value) return
    settingsStore.showOnboarding = false
}

emitter.on('start-tour', startTour)

/**
 * This is the Startup procedure for the UI application:
 */
onMounted(async () => {
    deviceStore.connectToQtIPC()

    // Add theme change event listener
    window.addEventListener('theme-changed', () => {
        // Ensure custom theme is correctly applied
        if (settingsStore.themeMode === ThemeMode.CUSTOM) {
            applyCustomTheme()
        }
    })

    // Set default language
    const savedLocale = localStorage.getItem('locale')

    if (savedLocale) {
        locale.value = savedLocale
    } else {
        // Use browser language if supported
        const browserLang = navigator.language.toLowerCase()

        // List of supported languages
        const supportedLanguages: Record<string, string> = {
            zh: 'zh', // Chinese (Simplified)
            'zh-cn': 'zh', // Chinese (Mainland China)
            'zh-tw': 'zh-tw', // Chinese (Traditional)
            'zh-hk': 'zh-tw', // Chinese (Hong Kong)
            ja: 'ja', // Japanese
            ru: 'ru', // Russian
            de: 'de', // German
            fr: 'fr', // French
            es: 'es', // Spanish
            ar: 'ar', // Arabic
            pt: 'pt', // Portuguese
            'pt-br': 'pt', // Brazilian Portuguese
            hi: 'hi', // Hindi
        }

        // Check for exact match
        if (supportedLanguages[browserLang]) {
            locale.value = supportedLanguages[browserLang]
        }
        // Check for language prefix match (e.g. en-US matches en)
        else {
            const langPrefix = browserLang.split('-')[0]
            if (supportedLanguages[langPrefix]) {
                locale.value = supportedLanguages[langPrefix]
            } else {
                // Default to English
                locale.value = 'en'
            }
        }

        // Save to localStorage
        localStorage.setItem('locale', locale.value)
    }
    document.querySelector('html')?.setAttribute('lang', locale.value)

    // Handshake and login must happen before anything else
    const handshakeSuccessful = await deviceStore.handshake()
    if (!handshakeSuccessful) {
        initSuccessful.value = false
        return
    }
    const loginSuccessful = await deviceStore.login()
    if (!loginSuccessful) {
        return
    }
    const loading = ElLoading.service({
        lock: true,
        text: t('common.loading'),
        background: svgLoaderBackground,
        svg: svgLoader,
        svgViewBox: svgLoaderViewBox,
    })

    const deviceInitSuccessful = await deviceStore.initializeDevices()
    if (!deviceInitSuccessful) {
        initSuccessful.value = false
        loading.close()
        return
    }
    await settingsStore.initializeSettings(deviceStore.allDevices())
    // Honor the configured startup page, but only when the user landed on the
    // default root route (no deep link). The empty path's component is
    // AppInfoView, so AppInfo needs no redirect; Controls and HomeDashboard do.
    if (router.currentRoute.value.name === 'startup-page') {
        const startup = settingsStore.startupPage
        if (startup === StartupPage.Controls) {
            await router.replace({ name: 'system-controls' })
        } else if (startup === StartupPage.HomeDashboard) {
            await router.replace({ name: 'dashboards' })
        }
    }
    applyCustomTheme()
    await daemonState.init()
    await deviceStore.loadAllPlugins()
    loaded.value = true
    loading.close()
    await deviceStore.loadLogs()
    // Some other dialogs, like the password dialog, will wait until Onboarding has closed
    if (settingsStore.showOnboarding) startTour()
    let signalLoadFinished = async (): Promise<void> => {
        if (deviceStore.isQtApp()) {
            // Helps with Qt startup handling, i.e. startInTray
            // @ts-ignore
            const ipc = window.ipc
            await ipc.loadFinished()
        }
    }
    // Fire-and-forget: SW manages its own SSE connection independently.
    deviceStore.initNotificationWorker()
    // async functions that run for the lifetime of the application:
    await Promise.all([
        deviceStore.updateStatusFromSSE(),
        deviceStore.updateLogsFromSSE(),
        deviceStore.updateAlertsFromSSE(),
        deviceStore.updateActiveModeFromSSE(),
        signalLoadFinished(),
    ])
})
</script>

<template>
    <RouterView v-if="loaded" />
    <Toast position="top-center" />
    <DynamicDialog />
    <ConfirmDialog
        :pt="{
            mask: {
                style: 'backdrop-filter: blur(2px); -webkit-backdrop-filter: blur(2px);',
            },
        }"
    >
        <template #message="slotProps">
            <div class="flex flex-col items-center">
                <i
                    v-if="slotProps.message.icon"
                    class="text-text-color-secondary text-4xl mb-2"
                    :class="slotProps.message.icon"
                />
                <p class="w-96">{{ slotProps.message.message }}</p>
            </div>
        </template>
    </ConfirmDialog>
    <ConfirmDialog
        group="AseTek690"
        :pt="{
            mask: {
                style: 'backdrop-filter: blur(2px); -webkit-backdrop-filter: blur(2px);',
            },
        }"
    >
        <template #message="slotProps">
            <div class="flex flex-col w-[34rem] gap-3 text-wrap">
                <p>
                    {{ t('components.aseTek690.sameDeviceID') }}
                </p>
                <p>
                    {{ t('components.aseTek690.restartRequired') }}
                </p>
                <p>
                    {{ t('components.aseTek690.deviceModel') }}
                    <strong>#{{ slotProps.message.message }}</strong
                    ><br />
                    {{ t('components.aseTek690.modelList') }}
                </p>
            </div>
        </template>
    </ConfirmDialog>
    <Dialog
        class="leading-loose"
        :visible="!initSuccessful"
        :header="t('views.error.connectionError')"
        :style="{ width: '50vw' }"
    >
        <p>
            {{ t('views.error.connectionErrorMessage') }} <br />
            {{ t('views.error.serviceRunningMessage') }}
        </p>
        <p>
            {{ t('views.error.checkProjectPage') }}
            <a
                href="https://gitlab.com/coolercontrol/coolercontrol/"
                target="_blank"
                style="color: rgb(var(--colors-accent))"
            >
                {{ t('views.error.projectPage') }}</a
            >
        </p>
        <br />
        <p>{{ t('views.error.helpfulCommands') }}</p>
        <p>
            <code>
                sudo systemctl enable --now coolercontrold<br />
                sudo systemctl status coolercontrold<br />
            </code>
        </p>
        <br />
        <p>
            {{ t('views.error.nonStandardAddress') }}
        </p>
        <br />
        <h6 v-if="deviceStore.isQtApp()" class="text-lg">
            {{ t('views.error.daemonAddressDesktop') }}
        </h6>
        <h6 v-else class="text-xl mb-4">{{ t('views.error.daemonAddressWeb') }}</h6>
        <div>
            <div class="mt-8 flex flex-row">
                <FloatLabel variant="on">
                    <InputText
                        id="host-address"
                        v-model="daemonAddress"
                        class="mb-2 w-60"
                        v-tooltip.top="t('views.error.addressTooltip')"
                        :invalid="daemonAddress.length === 0"
                    />
                    <label for="host-address">{{ t('common.address') }}</label>
                </FloatLabel>
                <span class="mx-2">:</span>
                <FloatLabel variant="on">
                    <InputNumber
                        id="daemon-port"
                        v-model="daemonPort"
                        showButtons
                        :min="80"
                        :max="65535"
                        :useGrouping="false"
                        class="mb-2"
                        :input-style="{ width: '6rem' }"
                        v-tooltip.top="t('views.error.portTooltip')"
                        button-layout="horizontal"
                        :allow-empty="false"
                    >
                        <template #incrementicon>
                            <span class="pi pi-plus" />
                        </template>
                        <template #decrementicon>
                            <span class="pi pi-minus" />
                        </template>
                    </InputNumber>
                    <label for="daemon-port">{{ t('common.port') }}</label>
                </FloatLabel>
            </div>
            <div class="flex flex-col mb-3 w-12 leading-none align-middle">
                <small class="ml-3 font-light text-sm text-text-color-secondary">{{
                    t('common.protocol')
                }}</small>
                <div
                    class="flex flex-row items-center"
                    v-tooltip.left="t('views.error.sslTooltip')"
                >
                    <el-switch v-model="daemonSslEnabled" size="large" />
                    <span class="ml-2 m-1">{{ t('common.sslTls') }}</span>
                </div>
            </div>
            <div>
                <Button
                    :label="t('common.saveAndRefresh')"
                    class="mb-2 w-44"
                    v-tooltip.left="t('views.error.saveTooltip')"
                    @click="saveDaemonSettings"
                />
            </div>
            <Button
                :label="t('common.reset')"
                v-tooltip.left="t('views.error.resetTooltip')"
                @click="resetDaemonSettings"
            />
        </div>
        <template #footer>
            <Button
                :label="t('common.retry')"
                icon="pi pi-refresh"
                @click="deviceStore.reloadUI()"
            />
        </template>
    </Dialog>
    <Dialog
        class="leading-loose"
        :visible="deviceStore.accessDenied"
        :header="t('views.error.accessDenied')"
        :style="{ width: '30vw' }"
        :closable="false"
    >
        <p>
            {{ t('views.error.accessDeniedMessage') }}
        </p>
        <template #footer>
            <Button
                class="outline-none"
                :label="t('common.retry')"
                icon="pi pi-refresh"
                @click="deviceStore.reloadUI()"
                @keydown.enter="deviceStore.reloadUI()"
                autofocus
            />
        </template>
    </Dialog>
    <VOnboardingWrapper
        ref="onboardingWrapper"
        :steps="steps"
        :options="{
            autoFinishByExit: true,
            scrollToStep: { enabled: !settingsStore.showOnboarding },
        }"
        @finish="onTourFinished"
    >
        <template #default="{ previous, next, step, isFirst, isLast }">
            <VOnboardingStep>
                <div class="bg-bg-two shadow rounded-lg border-2 border-border-one">
                    <div class="px-4 py-5 sm:p-6">
                        <!-- Welcome step: intro + 3-button mode choice -->
                        <template v-if="step.content?.kind === 'welcome'">
                            <div class="relative max-w-2xl">
                                <button
                                    @click="skipTour"
                                    class="absolute right-0 top-0 text-text-color-secondary font-medium text-base pi pi-times outline-0"
                                />
                                <h3 class="text-2xl leading-6 text-text-color">
                                    {{ t('components.onboarding.welcome') }}
                                </h3>
                                <p class="mt-4 text-base text-text-color-secondary leading-relaxed">
                                    {{ t('components.onboarding.gettingStartedIntro') }}
                                </p>
                                <div
                                    class="mt-4 p-3 rounded-md bg-accent/10 border-l-4 border-accent"
                                >
                                    <p class="text-text-color-secondary font-semibold leading-snug">
                                        {{ t('views.appInfo.helpSettingUp') }}
                                    </p>
                                    <ol
                                        class="mt-3 ml-2 pl-6 list-decimal text-sm text-text-color-secondary space-y-1"
                                    >
                                        <li>
                                            {{
                                                t('views.appInfo.gettingStartedStep1', {
                                                    profile: t(
                                                        'views.appInfo.gettingStartedGraphProfile',
                                                    ),
                                                })
                                            }}
                                        </li>
                                        <li>
                                            {{
                                                t('views.appInfo.gettingStartedStep2', {
                                                    controls: t(
                                                        'views.appInfo.gettingStartedControlsPage',
                                                    ),
                                                })
                                            }}
                                        </li>
                                        <li>
                                            {{ t('views.appInfo.gettingStartedStep3') }}
                                        </li>
                                    </ol>
                                </div>
                                <div class="mt-4 flex flex-col gap-2 text-sm">
                                    <a
                                        target="_blank"
                                        :href="GETTING_STARTED_URL"
                                        class="text-accent outline-0"
                                    >
                                        <span class="pi pi-external-link mr-2" />
                                        {{ t('views.appInfo.gettingStarted') }}
                                    </a>
                                    <a
                                        target="_blank"
                                        href="https://docs.coolercontrol.org/hardware-support.html"
                                        class="text-accent outline-0"
                                    >
                                        <span class="pi pi-external-link mr-2" />
                                        {{ t('views.appInfo.hardwareSupport') }}
                                    </a>
                                </div>
                                <p class="mt-4 text-sm italic text-text-color-secondary">
                                    {{ t('components.onboarding.startTourAgain') }}
                                </p>
                                <div class="mt-6 flex flex-col sm:flex-row gap-3">
                                    <button
                                        @click="chooseTourMode('basic')"
                                        type="button"
                                        class="inline-flex items-center justify-center rounded-lg border border-transparent bg-accent/80 hover:!bg-accent px-4 py-2 font-medium text-text-color shadow-sm focus:outline-none"
                                    >
                                        {{ t('components.onboarding.quickTour') }}
                                    </button>
                                    <button
                                        @click="chooseTourMode('thorough')"
                                        type="button"
                                        class="inline-flex items-center justify-center rounded-lg border border-transparent bg-accent/80 hover:!bg-accent px-4 py-2 font-medium text-text-color shadow-sm focus:outline-none"
                                    >
                                        {{ t('components.onboarding.thoroughTour') }}
                                    </button>
                                    <button
                                        @click="skipTour"
                                        type="button"
                                        class="inline-flex items-center justify-center rounded-lg border border-border-one bg-bg-two hover:bg-bg-one px-4 py-2 font-medium text-text-color-secondary shadow-sm focus:outline-none"
                                    >
                                        {{ t('components.onboarding.maybeLater') }}
                                    </button>
                                </div>
                            </div>
                        </template>
                        <!-- Finish step: docs link + finish buttons -->
                        <template v-else-if="step.content?.kind === 'finish'">
                            <div class="relative max-w-xl">
                                <button
                                    @click="skipTour"
                                    class="absolute right-0 top-0 text-text-color-secondary font-medium text-base pi pi-times outline-0"
                                />
                                <h3 class="text-2xl leading-6 text-text-color">
                                    {{ t('components.onboarding.thatsIt') }}
                                </h3>
                                <p class="mt-4 text-base text-text-color-secondary leading-relaxed">
                                    {{ t('components.onboarding.startNow') }}
                                </p>
                                <div class="mt-6 flex flex-col sm:flex-row gap-3">
                                    <button
                                        @click="openGettingStartedDocs"
                                        type="button"
                                        class="inline-flex items-center justify-center rounded-lg border border-transparent bg-accent/80 hover:!bg-accent px-4 py-2 font-medium text-text-color shadow-sm focus:outline-none"
                                    >
                                        <span class="pi pi-external-link mr-2" />
                                        {{ t('components.onboarding.openGettingStarted') }}
                                    </button>
                                    <button
                                        @click="skipTour"
                                        type="button"
                                        class="inline-flex items-center justify-center rounded-lg border border-border-one bg-bg-two hover:bg-bg-one px-4 py-2 font-medium text-text-color shadow-sm focus:outline-none"
                                    >
                                        {{ t('components.onboarding.finishLater') }}
                                    </button>
                                </div>
                            </div>
                        </template>
                        <!-- Middle steps: title + description + Previous/Next -->
                        <template v-else>
                            <div class="sm:flex sm:justify-between">
                                <div v-if="step.content">
                                    <h3
                                        v-if="step.content.title"
                                        class="text-2xl leading-6 text-text-color"
                                    >
                                        {{ step.content.title }}
                                    </h3>
                                    <div
                                        v-if="step.content.description"
                                        class="mt-4 max-w-xl text-base text-text-color-secondary"
                                    >
                                        <p>{{ step.content.description }}</p>
                                    </div>
                                </div>
                                <div
                                    class="mt-5 space-x-4 sm:mt-0 sm:ml-6 sm:flex sm:flex-shrink-0 sm:items-end relative"
                                >
                                    <button
                                        @click="skipTour"
                                        class="absolute right-0 bottom-full mb-[-1.0rem] text-text-color-secondary font-medium text-base pi pi-times outline-0"
                                    />
                                    <template v-if="!isFirst">
                                        <button
                                            @click="previous"
                                            type="button"
                                            class="mt-12 inline-flex items-center rounded-lg border border-transparent bg-accent/80 hover:!bg-accent px-4 py-2 font-medium text-text-color shadow-sm focus:outline-none sm:text-sm"
                                        >
                                            {{ t('common.previous') || 'Previous' }}
                                        </button>
                                    </template>
                                    <button
                                        @click="next"
                                        type="button"
                                        class="mt-12 inline-flex items-center rounded-lg border border-transparent bg-accent/80 hover:!bg-accent px-4 py-2 font-medium text-text-color shadow-sm focus:outline-none sm:text-sm"
                                    >
                                        {{
                                            isLast
                                                ? t('common.finish') || 'Finish'
                                                : t('common.next') || 'Next'
                                        }}
                                    </button>
                                </div>
                            </div>
                        </template>
                    </div>
                </div>
            </VOnboardingStep>
        </template>
    </VOnboardingWrapper>
</template>

<style>
:root {
    background-color: rgb(var(--colors-bg-one));
    --el-color-primary: rgb(var(--colors-accent));
    --v-onboarding-overlay-z: 60;
    --v-onboarding-step-z: 70;
    --v-onboarding-overlay-opacity: 0.125;
}
[data-v-onboarding-wrapper] [data-popper-arrow]::before {
    content: '';
    background: rgb(var(--colors-bg-two));
    top: 0;
    left: 0;
    transition:
        transform 0.2s ease-out,
        visibility 0.2s ease-out;
    visibility: visible;
    transform: translateX(0px) rotate(45deg);
    transform-origin: center;
    width: 1rem;
    height: 1rem;
    position: absolute;
    z-index: -1;
    border-left: 2px solid rgb(var(--colors-border-one));
    border-bottom: 2px solid rgb(var(--colors-border-one));
}

[data-v-onboarding-wrapper] [data-popper-placement^='top'] > [data-popper-arrow] {
    bottom: 7px;
}

[data-v-onboarding-wrapper] [data-popper-placement^='right'] > [data-popper-arrow] {
    left: -6px;
}

[data-v-onboarding-wrapper] [data-popper-placement^='bottom'] > [data-popper-arrow] {
    top: -6px;
}

[data-v-onboarding-wrapper] [data-popper-placement^='left'] > [data-popper-arrow] {
    right: -6px;
}
</style>
