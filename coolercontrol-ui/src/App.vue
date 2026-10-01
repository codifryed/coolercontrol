<!--
  SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
  SPDX-License-Identifier: GPL-3.0-or-later
-->

<script setup lang="ts">
import { RouterView, useRouter } from 'vue-router'
import { startupRouteName } from '@/shell/sections.ts'
import { sortEntitiesByTree } from '@/shell/libraryFolders.ts'
import { TOUR_STEPS } from '@/shell/tour.ts'
import { buildQtStrings } from '@/shell/qtStrings.ts'
import { useToolWizards } from '@/composables/useToolWizards.ts'
import { Ref, onMounted, ref, inject, nextTick } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useCalibrationStore } from '@/stores/CalibrationStore'
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiClose, mdiOpenInNew, mdiRefresh } from '@mdi/js'
import UiButton from '@/shell/ui/UiButton.vue'
import UiInput from '@/shell/ui/UiInput.vue'
import UiNumberInput from '@/shell/ui/UiNumberInput.vue'
import UiToast from '@/shell/ui/UiToast.vue'
import ConnectionLostOverlay from '@/shell/ConnectionLostOverlay.vue'
import SupportWizardOverlay from '@/shell/SupportWizardOverlay.vue'
import UiConfirmDialog from '@/shell/ui/UiConfirmDialog.vue'
import UiModal from '@/shell/ui/UiModal.vue'
import UiDynamicDialog from '@/shell/ui/UiDynamicDialog.vue'
import { ThemeMode } from '@/models/UISettings.ts'
import { useDaemonState } from '@/stores/DaemonState.ts'
import { VOnboardingWrapper, VOnboardingStep, useVOnboarding } from 'v-onboarding'
import { Emitter, EventType } from 'mitt'
import { showLoadingOverlay } from '@/components/loadingOverlay.ts'
import UiSwitch from '@/shell/ui/UiSwitch.vue'
import { useI18n } from 'vue-i18n'

const { t, locale } = useI18n({ useScope: 'global' })
const loaded: Ref<boolean> = ref(false)
const initSuccessful = ref(true)
const deviceStore = useDeviceStore()
const router = useRouter()
const { openCalibrationWizard } = useToolWizards()
const settingsStore = useSettingsStore()
const calibrationStore = useCalibrationStore()
const daemonState = useDaemonState()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!
// True only when the app opened at the root URL (captured in main.ts before the
// router rewrote the hash); gates the configured-start-page redirect below.
const startedAtRoot = inject<boolean>('startedAtRoot', false)

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

const steps: Ref<any[]> = ref([])
const isTransitioningMode = ref(false)

const POPPER_RIGHT = { popper: { placement: 'right' } }
const GETTING_STARTED_URL =
    'https://docs.coolercontrol.org/getting-started.html#%F0%9F%A7%99-configure'

const makeStep = (selector: string, key: string, placement: string = 'right'): any => ({
    attachTo: { element: selector },
    content: {
        title: t(`components.onboarding.${key}`),
        description: t(`components.onboarding.${key}Desc`),
    },
    options: { popper: { placement } },
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

// filterPresent drops any absent anchor, such as #rail-plugins when no plugins
// are installed.
const buildTourSteps = (): any[] =>
    filterPresent([
        ...TOUR_STEPS.map((step) => makeStep(step.selector, step.key, step.placement)),
        finishStep(),
    ])

const startTour = (): void => {
    steps.value = [welcomeStep()]
    start()
}

const startWalkthrough = async (): Promise<void> => {
    isTransitioningMode.value = true
    finish()
    steps.value = buildTourSteps()
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
    settingsStore.completeOnboarding()
}

emitter.on('start-tour', startTour)

/**
 * This is the Startup procedure for the UI application:
 */
onMounted(async () => {
    deviceStore.connectToQtIPC()

    if (window.parent !== window) {
        // notify parent window, e.g. Cockpit, that UI has started loading
        window.parent.postMessage('loadstart', '*')
    }

    // Add theme change event listener
    window.addEventListener('theme-changed', () => {
        // Ensure custom theme is correctly applied
        if (settingsStore.themeMode === ThemeMode.CUSTOM) {
            applyCustomTheme()
        }
    })

    // The language was resolved when the i18n instance was built, from the
    // cached setting; loadUISettings replaces it with the daemon's copy.
    document.documentElement.setAttribute('lang', locale.value)

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
    const loading = showLoadingOverlay({ text: t('common.loading') })

    const deviceInitSuccessful = await deviceStore.initializeDevices()
    if (!deviceInitSuccessful) {
        initSuccessful.value = false
        loading.close()
        return
    }
    await settingsStore.initializeSettings(deviceStore.allDevices())
    // Apply the persisted panel order (devices, channels, profiles, functions);
    // the old tree menu did this on build, the shell applies it once at boot.
    deviceStore.reSortDevicesByMenuOrder()
    // Folders expand in place, so a filed profile keeps the position its folder
    // has: what the panel shows top to bottom is what every dropdown lists.
    sortEntitiesByTree(settingsStore.menuOrder, 'profiles', settingsStore.profiles, (p) => p.uid)
    sortEntitiesByTree(settingsStore.menuOrder, 'functions', settingsStore.functions, (f) => f.uid)
    // Prime the calibration cache before `loaded` flips below: the cooling
    // channel page reads `statusFor` on first paint.
    await calibrationStore.refreshAllStatuses()
    // Re-attach to a calibration batch the daemon is still driving after a
    // reload (the daemon owns the queue, so it survived the suspend/reload).
    const calibrationBatchResumed = await calibrationStore.ensureBatchPolling()
    // Honor the configured startup page, but only when the app opened at the
    // root URL (startedAtRoot), not on a direct/deep link like /#/home. The
    // `startup-page` route resolves this too, but it ran before settings had
    // loaded, so it landed on the default; re-apply now that they are in.
    if (startedAtRoot) {
        const target = startupRouteName(settingsStore.startupPage)
        if (target !== 'section-home') {
            await router.replace({ name: target })
        }
    }
    applyCustomTheme()
    await daemonState.init()
    await deviceStore.loadAllPlugins()
    loaded.value = true
    loading.close()
    // Reopen the calibration wizard to show live progress once the layout
    // has mounted.
    if (calibrationBatchResumed) {
        await nextTick()
        openCalibrationWizard()
    }
    await deviceStore.loadLogs()
    // Some other dialogs, like the password dialog, will wait until Onboarding has closed.
    if (settingsStore.showOnboarding) startTour()
    let signalLoadFinished = async (): Promise<void> => {
        if (deviceStore.isQtApp()) {
            // Helps with Qt startup handling, i.e. startInTray
            // @ts-ignore
            const ipc = window.ipc
            await ipc.loadFinished()
            // Push the current alert state now the IPC bridge is confirmed ready.
            settingsStore.pushTrayAlertState()
            // Qt renders its own dialogs and has no translations. Hand it the strings
            // it needs in the active locale; it caches them, since a discarded
            // renderer cannot be asked later.
            // Optional: a Qt app that has not been restarted since the update has no
            // such method, and a bare call there rejects this whole function, which
            // would strand pushTrayPinnedSensors below.
            ipc.setTranslations?.(JSON.stringify(buildQtStrings(t)))
            // Seed the tray's sensor list. The watch only fires on later changes.
            settingsStore.pushTrayPinnedSensors()
        } else if (window.parent !== window) {
            // notify parent window, e.g. Cockpit, that UI has finished loading
            window.parent.postMessage('loadend', '*')
        }
    }
    // Fire-and-forget: SW manages its own SSE connection independently.
    deviceStore.initNotificationWorker()
    // async functions that run for the lifetime of the application:
    await Promise.all([deviceStore.updateFromSSE(), signalLoadFinished()])
})
</script>

<template>
    <RouterView v-if="loaded" />
    <ConnectionLostOverlay />
    <SupportWizardOverlay />
    <UiToast />
    <UiDynamicDialog />
    <UiConfirmDialog />
    <UiConfirmDialog group="AseTek690">
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
    </UiConfirmDialog>
    <UiModal
        content-class="leading-loose"
        :open="!initSuccessful"
        :title="t('views.error.connectionError')"
        :content-style="{ width: '50vw' }"
        :closable="false"
        :dismissable="false"
        :close-on-escape="false"
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
            <div class="mt-8 flex flex-row items-end">
                <div>
                    <label
                        for="host-address"
                        class="mb-1 ml-1 block text-sm text-text-color-secondary"
                        >{{ t('common.address') }}</label
                    >
                    <UiInput
                        id="host-address"
                        v-model="daemonAddress"
                        class="mb-2 w-60"
                        v-tooltip.top="t('views.error.addressTooltip')"
                        :invalid="daemonAddress.length === 0"
                    />
                </div>
                <span class="mx-2 mb-4">:</span>
                <div>
                    <label
                        for="daemon-port"
                        class="mb-1 ml-1 block text-sm text-text-color-secondary"
                        >{{ t('common.port') }}</label
                    >
                    <UiNumberInput
                        id="daemon-port"
                        v-model="daemonPort"
                        :min="80"
                        :max="65535"
                        class="mb-2"
                        v-tooltip.top="t('views.error.portTooltip')"
                    />
                </div>
            </div>
            <div class="flex flex-col mb-3 w-12 leading-none align-middle">
                <small class="ml-3 font-light text-sm text-text-color-secondary">{{
                    t('common.protocol')
                }}</small>
                <div class="flex flex-row items-center" v-tooltip.top="t('views.error.sslTooltip')">
                    <UiSwitch v-model="daemonSslEnabled" />
                    <span class="ml-2 m-1">{{ t('common.sslTls') }}</span>
                </div>
            </div>
            <div>
                <UiButton
                    class="mb-2 w-44"
                    v-tooltip.top="t('views.error.saveTooltip')"
                    @click="saveDaemonSettings"
                >
                    {{ t('common.saveAndRefresh') }}
                </UiButton>
            </div>
            <UiButton
                variant="outline"
                v-tooltip.top="t('views.error.resetTooltip')"
                @click="resetDaemonSettings"
            >
                {{ t('common.reset') }}
            </UiButton>
        </div>
        <div class="mt-4 flex justify-end">
            <UiButton class="gap-1" @click="deviceStore.reloadUI()">
                <svg-icon type="mdi" :path="mdiRefresh" :size="18" />
                {{ t('common.retry') }}
            </UiButton>
        </div>
    </UiModal>
    <UiModal
        content-class="leading-loose"
        :open="deviceStore.accessDenied"
        :title="t('views.error.accessDenied')"
        :content-style="{ width: '30vw' }"
        :closable="false"
        :dismissable="false"
        :close-on-escape="false"
    >
        <p>
            {{ t('views.error.accessDeniedMessage') }}
        </p>
        <div class="mt-4 flex justify-end">
            <UiButton
                class="gap-1"
                @click="deviceStore.reloadUI()"
                @keydown.enter="deviceStore.reloadUI()"
                autofocus
            >
                <svg-icon type="mdi" :path="mdiRefresh" :size="18" />
                {{ t('common.retry') }}
            </UiButton>
        </div>
    </UiModal>
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
                <div
                    class="bg-bg-two drop-shadow-[0_4px_8px_rgba(0,0,0,0.5)] rounded-lg border-2 border-border-one"
                >
                    <div class="px-4 py-5 sm:p-6">
                        <!-- Welcome step: intro + start-tour choice -->
                        <template v-if="step.content?.kind === 'welcome'">
                            <div class="relative max-w-2xl">
                                <button
                                    @click="skipTour"
                                    class="absolute right-0 top-0 text-text-color-secondary font-medium outline-0"
                                >
                                    <svg-icon type="mdi" :path="mdiClose" :size="16" />
                                </button>
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
                                            {{ t('views.appInfo.gettingStartedStep1') }}
                                        </li>
                                        <li>
                                            {{ t('views.appInfo.gettingStartedStep2') }}
                                        </li>
                                        <li>
                                            {{ t('views.appInfo.gettingStartedStep3') }}
                                        </li>
                                    </ol>
                                    <p class="mt-3 ml-2 text-sm text-text-color-secondary">
                                        {{
                                            t('views.appInfo.gettingStartedAutoCreate', {
                                                wizard: t(
                                                    'views.appInfo.gettingStartedAutoCreateLink',
                                                ),
                                            })
                                        }}
                                    </p>
                                </div>
                                <div class="mt-4 flex flex-col gap-2 text-sm">
                                    <a
                                        target="_blank"
                                        :href="GETTING_STARTED_URL"
                                        class="flex w-fit items-center gap-2 text-accent outline-0"
                                    >
                                        <svg-icon type="mdi" :path="mdiOpenInNew" :size="16" />
                                        {{ t('views.appInfo.gettingStarted') }}
                                    </a>
                                    <a
                                        target="_blank"
                                        href="https://docs.coolercontrol.org/hardware-support.html"
                                        class="flex w-fit items-center gap-2 text-accent outline-0"
                                    >
                                        <svg-icon type="mdi" :path="mdiOpenInNew" :size="16" />
                                        {{ t('views.appInfo.hardwareSupport') }}
                                    </a>
                                </div>
                                <p class="mt-4 text-sm italic text-text-color-secondary">
                                    {{ t('components.onboarding.startTourAgain') }}
                                </p>
                                <div class="mt-6 flex flex-col sm:flex-row gap-3">
                                    <button
                                        @click="startWalkthrough"
                                        type="button"
                                        class="inline-flex items-center justify-center rounded-lg border border-transparent bg-accent px-4 py-2 font-medium text-accent-fg shadow-sm outline-none hover:bg-accent/90"
                                    >
                                        {{ t('components.onboarding.startTour') }}
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
                                    class="absolute right-0 top-0 text-text-color-secondary font-medium outline-0"
                                >
                                    <svg-icon type="mdi" :path="mdiClose" :size="16" />
                                </button>
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
                                        <svg-icon
                                            type="mdi"
                                            :path="mdiOpenInNew"
                                            :size="16"
                                            class="mr-2"
                                        />
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
                                        class="absolute right-0 bottom-full mb-[-1.0rem] text-text-color-secondary font-medium outline-0"
                                    >
                                        <svg-icon type="mdi" :path="mdiClose" :size="16" />
                                    </button>
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
}

/*
 * The arrow is a rotated square; its two bordered sides form the tip pointing
 * toward the anchor. The border pair therefore differs per placement (the base
 * ::before draws no border, each placement sets its own).
 */
[data-v-onboarding-wrapper] [data-popper-placement^='top'] > [data-popper-arrow] {
    bottom: 7px;
}
[data-v-onboarding-wrapper] [data-popper-placement^='top'] > [data-popper-arrow]::before {
    border-right: 2px solid rgb(var(--colors-border-one));
    border-bottom: 2px solid rgb(var(--colors-border-one));
}

[data-v-onboarding-wrapper] [data-popper-placement^='right'] > [data-popper-arrow] {
    left: -6px;
}
[data-v-onboarding-wrapper] [data-popper-placement^='right'] > [data-popper-arrow]::before {
    border-left: 2px solid rgb(var(--colors-border-one));
    border-bottom: 2px solid rgb(var(--colors-border-one));
}

[data-v-onboarding-wrapper] [data-popper-placement^='bottom'] > [data-popper-arrow] {
    top: -6px;
}
[data-v-onboarding-wrapper] [data-popper-placement^='bottom'] > [data-popper-arrow]::before {
    border-top: 2px solid rgb(var(--colors-border-one));
    border-left: 2px solid rgb(var(--colors-border-one));
}

[data-v-onboarding-wrapper] [data-popper-placement^='left'] > [data-popper-arrow] {
    right: -6px;
}
[data-v-onboarding-wrapper] [data-popper-placement^='left'] > [data-popper-arrow]::before {
    /* anchor to the arrow element's right edge so the diamond straddles the
       popover's right edge (element is positioned by its right side here) */
    left: auto;
    right: 0;
    border-top: 2px solid rgb(var(--colors-border-one));
    border-right: 2px solid rgb(var(--colors-border-one));
}
</style>
