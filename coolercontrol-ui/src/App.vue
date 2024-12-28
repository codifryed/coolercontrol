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
import Checkbox from 'primevue/checkbox'
import { ElLoading } from 'element-plus'
import 'element-plus/es/components/loading/style/css'
import { ThemeMode } from '@/models/UISettings.ts'
import { useDaemonState } from '@/stores/DaemonState.ts'
import { VOnboardingWrapper, VOnboardingStep, useVOnboarding } from 'v-onboarding'
import { Emitter, EventType } from 'mitt'
import { svgLoader, svgLoaderBackground, svgLoaderViewBox } from '@/models/Loader.ts'

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
    text: 'Initializing...',
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
            title: 'Welcome to CoolerControl!',
            description: 'Filled by template - special message',
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
    {
        attachTo: { element: '#system-menu' },
        content: {
            title: 'System Menu',
            description:
                "This is the start of the main menu where this system's devices and sensors can be viewed and controlled.",
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
    {
        attachTo: { element: '#dashboards' },
        content: {
            title: 'Dashboards',
            description:
                "Dashboards are a curated collection of charts to view your system's sensor data.",
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
            title: 'Profiles',
            description:
                'Profiles define customizable settings for controlling fan speeds. ' +
                'The same Profile can be used for multiple fans and devices.',
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
            title: 'Functions',
            description:
                'Functions are configurable algorithms that can be applied to a ' +
                "Profile's output. This can be helpful for managing when fan speed changes occur.",
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
    {
        attachTo: { element: '#custom-sensors' },
        content: {
            title: 'Custom Sensors',
            description:
                'Custom Sensors allow you to combine existing sensor data in various ways, ' +
                'and enable you to use your own custom scripted sensor output.',
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
    {
        attachTo: { element: '#modes' },
        content: {
            title: 'Modes',
            description:
                'Modes are saved collections of your settings, allowing you to switch ' +
                'between silent and performance modes easily',
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
            title: 'Application and Daemon Information',
            description:
                'Clicking the logo opens the Application Information page, where you can ' +
                "to get information about the application, the system daemon, and logs. It's a " +
                "good place to go when troubleshooting issues and there's a small daemon-status " +
                'badge here to notify you of any potential issues.',
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
            title: 'Quick Add',
            description:
                'This is a quick menu to easily add new items like Dashboards, Profiles, etc.',
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
    {
        attachTo: { element: '#modes-quick' },
        content: {
            title: 'Quick Modes Change',
            description: 'This is a menu to quickly switch between saved Modes.',
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
    {
        attachTo: { element: '#access' },
        content: {
            title: 'Access Menu',
            description: 'This is where you manage your password and verify your access level.',
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
    {
        attachTo: { element: '#settings' },
        content: {
            title: 'Settings',
            description:
                'This button will open up the settings page containing different UI and daemon settings.',
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
    {
        attachTo: { element: '#restart' },
        content: {
            title: 'Restart Menu',
            description:
                'Here you can choose whether to reload the UI or restart the system daemon.',
        },
        options: {
            popper: {
                placement: 'right',
            },
        },
    },
]

/**
 * Startup procedure for the application.
 */
onMounted(async () => {
    const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
    initSuccessful.value = await deviceStore.initializeDevices()
    if (!initSuccessful.value) {
        loading.close()
        return
    }
    await settingsStore.initializeSettings(deviceStore.allDevices())
    applyCustomTheme()
    await sleep(300) // give the engine a moment to catch up for a smoother start
    await daemonState.init()
    loaded.value = true
    loading.close()
    await deviceStore.login()
    await deviceStore.load_logs()
    // Some other dialogs, like the password dialog, will wait until Onboarding has closed
    if (settingsStore.showOnboarding) start()
    // This basically blocks at this point:
    await Promise.all([deviceStore.updateStatusFromSSE(), deviceStore.updateLogsFromSSE()])
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
                    The legacy NZXT Krakens and the EVGA CLC happen to have the same device ID and
                    CoolerControl can not determine which device is connected. This is required for
                    proper device communication.
                </p>
                <p>
                    A restart of the CoolerControl systemd services may be required and will be
                    handled automatically if needed.
                </p>
                <p>
                    Is Liquidctl Device <strong>#{{ slotProps.message.message }}</strong> one of the
                    following models?<br />
                    NZXT Kraken X40, X60, X31, X41, X51 or X61
                </p>
            </div>
        </template>
    </ConfirmDialog>
    <Dialog
        class="leading-loose"
        :visible="!initSuccessful"
        header="CoolerControl Connection Error"
        :style="{ width: '50vw' }"
    >
        <p>
            A connection to the CoolerControl Daemon could not be established. <br />
            Please make sure that the systemd service is running and available.
        </p>
        <p>
            Check the
            <a
                href="https://gitlab.com/coolercontrol/coolercontrol/"
                target="_blank"
                style="color: rgb(var(--colors-accent))"
            >
                project page</a
            >
            for installation instructions.
        </p>
        <br />
        <p>Some helpful commands to enable and verify the daemon:</p>
        <p>
            <code>
                sudo systemctl enable --now coolercontrold<br />
                sudo systemctl status coolercontrold<br />
            </code>
        </p>
        <br />
        <p>
            If you have configured a non-standard address to connect to the daemon, you can set it
            here:
        </p>
        <br />
        <h6 v-if="deviceStore.isTauriApp()" class="text-xl">Daemon Address - Desktop App</h6>
        <h6 v-else class="text-xl mb-4">Daemon Address - Web UI</h6>
        <div>
            <div>
                <InputText
                    v-model="daemonAddress"
                    class="mb-2 w-24"
                    :input-style="{ width: '12rem' }"
                    v-tooltip.right="
                        'The IP address to use to communicate with the daemon. ' +
                        'This can be an IPv4 or IPv6 address.'
                    "
                />
            </div>
            <InputNumber
                v-model="daemonPort"
                showButtons
                :min="80"
                :max="65535"
                :useGrouping="false"
                class="mb-2"
                :input-style="{ width: '10rem' }"
                v-tooltip.right="'The port to use to communicate with the daemon'"
            />
            <div class="flex mb-3 leading-none align-middle">
                <Checkbox
                    v-model="daemonSslEnabled"
                    :binary="true"
                    v-tooltip.right="'Whether to connect to the daemon using SSL/TLS'"
                />
                <span class="ml-2 m-1">SSL/TLS</span>
            </div>
            <div>
                <Button
                    label="Save and Refresh"
                    class="mb-2 w-44"
                    v-tooltip.right="'Saves the daemon settings and reloads the UI.'"
                    @click="saveDaemonSettings"
                />
            </div>
            <Button
                label="Reset"
                v-tooltip.right="'Resets the daemon settings to their defaults and reloads the UI.'"
                @click="resetDaemonSettings"
            />
        </div>
        <template #footer>
            <Button label="Retry" icon="pi pi-refresh" @click="reloadPage" />
        </template>
    </Dialog>
    <VOnboardingWrapper
        ref="onboardingWrapper"
        :steps="steps"
        :options="{ autoFinishByExit: true }"
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
                                            This is a short introduction to get you started with
                                            CoolerControl.
                                        </p>
                                        <p class="mt-4">
                                            Before we get started, one of them most important things
                                            to know about is settings up your hardware drivers.
                                        </p>
                                        <br />
                                        <p>
                                            If your fans are not showing up or cannot be controlled,
                                            then likely there is an issue with your currently
                                            installed kernel drivers.<br /><br />

                                            Before opening an issue, please confirm that all drivers
                                            have been properly loaded by checking the
                                            <a
                                                href="https://gitlab.com/coolercontrol/coolercontrol/-/wikis/HWMon-Support"
                                                target="_blank"
                                                class="text-accent outline-0"
                                            >
                                                HWMon Support
                                            </a>
                                            and
                                            <a
                                                href="https://gitlab.com/coolercontrol/coolercontrol/-/wikis/adding-device-support"
                                                target="_blank"
                                                class="text-accent outline-0"
                                            >
                                                Adding Device Support
                                            </a>
                                            pages. <br /><br />
                                            <span class="italic"
                                                >Note: you can start this tour again at any time
                                                from the settings page.</span
                                            >
                                            <br /><br />
                                            Ok, let's get started!
                                        </p>
                                    </div>
                                    <div v-else-if="isLast">
                                        {{ step.content.description }}
                                        <br /><br />
                                        Ok, that's it. You're ready to get started!
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
                                        Previous
                                    </button>
                                </template>
                                <button
                                    @click="next"
                                    type="button"
                                    class="mt-12 inline-flex items-center rounded-lg border border-transparent bg-accent/80 hover:!bg-accent px-4 py-2 font-medium text-text-color shadow-sm focus:outline-none sm:text-sm"
                                >
                                    {{ isLast ? 'Finish' : 'Next' }}
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
