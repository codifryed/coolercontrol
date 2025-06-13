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
import 'reflect-metadata'
import { RouterView } from 'vue-router'
import { Ref, onMounted, ref, inject } from 'vue'
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
import { ThemeMode } from '@/models/UISettings.ts'
import { useDaemonState } from '@/stores/DaemonState.ts'
import { VOnboardingWrapper, VOnboardingStep, useVOnboarding } from 'v-onboarding'
import { Emitter, EventType } from 'mitt'
import { svgLoader, svgLoaderBackground, svgLoaderViewBox } from '@/models/Loader.ts'
import FloatLabel from 'primevue/floatlabel'
import { useI18n } from 'vue-i18n'

const { t, locale } = useI18n()
const loaded: Ref<boolean> = ref(false)
const initSuccessful = ref(true)
const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const daemonState = useDaemonState()
const emitter: Emitter<Record<EventType, any>> = inject('emitter')!

const reloadPage = () => window.location.reload()

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
const loading = ElLoading.service({
    lock: true,
    text: t('common.loading'),
    background: svgLoaderBackground,
    svg: svgLoader,
    svgViewBox: svgLoaderViewBox,
})
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
emitter.on('start-tour', start)
const steps = [
    {
        attachTo: { element: '#logo' },
        content: {
            title: t('components.onboarding.welcome'),
            description: 'Filled by template - special message',
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
    // {
    //     attachTo: { element: '#system-menu' },
    //     content: {
    //         title: 'System Menu',
    //         description:
    //             "This is the start of the main menu where this system's devices and sensors can be viewed and controlled.",
    //     },
    //     options: {
    //         popper: {
    //             placement: 'right',
    //         },
    //     },
    // },
    {
        attachTo: { element: '#dashboards' },
        content: {
            title: t('components.onboarding.dashboards'),
            description: t('components.onboarding.dashboardsDesc'),
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
    {
        attachTo: { element: '#profiles' },
        content: {
            title: t('components.onboarding.profiles'),
            description: t('components.onboarding.profilesDesc'),
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
    {
        attachTo: { element: '#functions' },
        content: {
            title: t('components.onboarding.functions'),
            description: t('components.onboarding.functionsDesc'),
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
    // {
    //     attachTo: { element: '#custom-sensors' },
    //     content: {
    //         title: 'Custom Sensors',
    //         description:
    //             'Custom Sensors allow you to combine existing sensor data in various ways, ' +
    //             'and enable you to use your own custom scripted sensor output.',
    //     },
    //     options: {
    //         popper: {
    //             placement: 'right',
    //         },
    //     },
    // },
    // {
    //     attachTo: { element: '#modes' },
    //     content: {
    //         title: 'Modes',
    //         description:
    //             'Modes are saved collections of your settings, allowing you to switch ' +
    //             'between silent and performance modes easily',
    //     },
    //     options: {
    //         popper: {
    //             placement: 'right',
    //         },
    //     },
    // },
    // {
    //     attachTo: { element: '#alerts' },
    //     content: {
    //         title: 'Alerts',
    //         description: 'Alerts will notify you when sensors values exceed chosen thresholds',
    //     },
    //     options: {
    //         popper: {
    //             placement: 'right',
    //         },
    //     },
    // },
    {
        attachTo: { element: '#logo' },
        content: {
            title: t('components.onboarding.appInfo'),
            description: t('components.onboarding.appInfoDesc'),
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
    {
        attachTo: { element: '#add' },
        content: {
            title: t('components.onboarding.quickAdd'),
            description: t('components.onboarding.quickAddDesc'),
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
    {
        attachTo: { element: '#dashboard-quick' },
        content: {
            title: t('components.onboarding.dashboardQuick'),
            description: t('components.onboarding.dashboardQuickDesc'),
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
    {
        attachTo: { element: '#controls' },
        content: {
            title: t('components.onboarding.controls'),
            description: t('components.onboarding.controlsDesc'),
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
    // {
    //     attachTo: { element: '#modes-quick' },
    //     content: {
    //         title: 'Modes Quick Menu',
    //         description: 'This is a menu to quickly switch between your saved Modes.',
    //     },
    //     options: {
    //         popper: {
    //             placement: 'right',
    //         },
    //     },
    // },
    // {
    //     attachTo: { element: '#collapse-menu' },
    //     content: {
    //         title: 'Collapse / Expand Main Menu',
    //         description: 'Use this to expand or collapse the main menu.',
    //     },
    //     options: {
    //         popper: {
    //             placement: 'right',
    //         },
    //     },
    // },
    // {
    //     attachTo: { element: '#alerts-quick' },
    //     content: {
    //         title: 'Alerts Overview',
    //         description: 'This is where you can view all the alert statuses and logs.',
    //     },
    //     options: {
    //         popper: {
    //             placement: 'right',
    //         },
    //     },
    // },
    {
        attachTo: { element: '#settings' },
        content: {
            title: t('components.onboarding.settings'),
            description: t('components.onboarding.settingsDesc'),
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
    // {
    //     attachTo: { element: '#access' },
    //     content: {
    //         title: 'Access Menu',
    //         description: 'This is where you manage your password and verify your access level.',
    //     },
    //     options: {
    //         popper: {
    //             placement: 'right',
    //         },
    //     },
    // },
    {
        attachTo: { element: '#restart' },
        content: {
            title: t('components.onboarding.restartMenu'),
            description: t('components.onboarding.restartMenuDesc'),
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
    {
        attachTo: { element: '#logo' },
        content: {
            title: t('components.onboarding.thatsIt'),
            description: 'filled by template - special message',
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
]

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

    initSuccessful.value = await deviceStore.initializeDevices()
    if (!initSuccessful.value) {
        loading.close()
        return
    }
    await settingsStore.initializeSettings(deviceStore.allDevices())
    applyCustomTheme()
    await daemonState.init()
    loaded.value = true
    loading.close()
    await deviceStore.login()
    await deviceStore.loadLogs()
    // Some other dialogs, like the password dialog, will wait until Onboarding has closed
    if (settingsStore.showOnboarding) start()
    let signalLoadFinished = async (): Promise<void> => {
        if (deviceStore.isQtApp()) {
            // Helps with Qt startup handling, i.e. startInTray
            // @ts-ignore
            const ipc = window.ipc
            await ipc.loadFinished()
        }
    }
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
            <Button :label="t('common.retry')" icon="pi pi-refresh" @click="reloadPage" />
        </template>
    </Dialog>
    <VOnboardingWrapper
        ref="onboardingWrapper"
        :steps="steps"
        :options="{
            autoFinishByExit: true,
            scrollToStep: { enabled: !settingsStore.showOnboarding },
            overlay: { preventOverlayInteraction: settingsStore.showOnboarding },
        }"
        @finish="settingsStore.showOnboarding = false"
    >
        <template #default="{ previous, next, step, isFirst, isLast }">
            <VOnboardingStep>
                <div class="bg-bg-two shadow rounded-lg">
                    <div class="px-4 py-5 sm:p-6">
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
                                    <div v-if="isFirst">
                                        <p>
                                            {{ t('components.onboarding.beforeStart') }}
                                        </p>
                                        <p class="mt-4">
                                            {{ t('components.onboarding.beforeStart') }}
                                            <br />
                                            <a
                                                href="https://docs.coolercontrol.org/hardware-support.html"
                                                target="_blank"
                                                class="text-accent outline-0 underline"
                                            >
                                                {{ t('components.onboarding.settingUpDrivers') }}
                                            </a>
                                            .
                                        </p>
                                        <br />
                                        <p>
                                            {{ t('components.onboarding.fansNotShowing')
                                            }}<br /><br />

                                            {{ t('components.onboarding.checkDocs') }}
                                            <a
                                                href="https://docs.coolercontrol.org/hardware-support.html"
                                                target="_blank"
                                                class="text-accent outline-0"
                                            >
                                                {{ t('components.onboarding.checkingDocs') }}
                                            </a>
                                            <br /><br />
                                            <span class="italic">{{
                                                t('components.onboarding.startTourAgain')
                                            }}</span>
                                            <br /><br />
                                            {{ t('components.onboarding.letsStart') }}
                                        </p>
                                    </div>
                                    <div v-else-if="isLast">
                                        <p>
                                            {{ t('components.onboarding.ready') }}
                                            <br />
                                            <a
                                                href="https://docs.coolercontrol.org/hardware-support.html"
                                                target="_blank"
                                                class="text-accent outline-0"
                                            >
                                                {{ t('components.onboarding.checkingDocs') }}
                                            </a>
                                        </p>
                                        <br />
                                        {{ t('components.onboarding.startNow') }}
                                    </div>
                                    <p v-else>{{ step.content.description }}</p>
                                </div>
                            </div>
                            <div
                                class="mt-5 space-x-4 sm:mt-0 sm:ml-6 sm:flex sm:flex-shrink-0 sm:items-end relative"
                            >
                                <!--<span class="absolute right-0 bottom-full mb-2 mr-2 text-text-color-secondary font-medium text-xs">{{ `${index + 1}/${steps.length}` }}</span>-->
                                <button
                                    @click="finish"
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
