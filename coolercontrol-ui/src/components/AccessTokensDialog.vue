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
import { onMounted, ref, type Ref } from 'vue'
import { useDeviceStore } from '@/stores/DeviceStore'
import { useSettingsStore } from '@/stores/SettingsStore'
import { useConfirm } from 'primevue/useconfirm'
import { useToast } from 'primevue/usetoast'
import { useI18n } from 'vue-i18n'
import { ErrorResponse } from '@/models/ErrorResponse'
import type { AccessTokenInfo, CreateTokenResponse } from '@/models/AccessToken'
import Button from 'primevue/button'
import InputText from 'primevue/inputtext'
import DataTable from 'primevue/datatable'
import Column from 'primevue/column'
import Tag from 'primevue/tag'
import FloatLabel from 'primevue/floatlabel'
import DatePicker from 'primevue/datepicker'
import Message from 'primevue/message'

const deviceStore = useDeviceStore()
const settingsStore = useSettingsStore()
const confirm = useConfirm()
const toast = useToast()
const { t } = useI18n()

const tokens: Ref<AccessTokenInfo[]> = ref([])
const newLabel: Ref<string> = ref('')
const newExpiry: Ref<Date | null> = ref(null)
const createdToken: Ref<CreateTokenResponse | null> = ref(null)
const loading: Ref<boolean> = ref(false)

async function loadTokens(): Promise<void> {
    loading.value = true
    const result = await deviceStore.daemonClient.listTokens()
    loading.value = false
    if (result instanceof ErrorResponse) {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: t('auth.tokenLoadError'),
            life: 5000,
        })
        return
    }
    tokens.value = result
}

async function createToken(): Promise<void> {
    const label = newLabel.value.trim()
    if (!label) return
    const expiresAt = newExpiry.value ? newExpiry.value.toISOString() : null
    const result = await deviceStore.daemonClient.createToken(label, expiresAt)
    if (result instanceof ErrorResponse) {
        toast.add({
            severity: 'error',
            summary: t('common.error'),
            detail: result.error || t('auth.tokenCreateError'),
            life: 5000,
        })
        return
    }
    createdToken.value = result
    newLabel.value = ''
    newExpiry.value = null
    await loadTokens()
}

async function deleteToken(tokenId: string): Promise<void> {
    confirm.require({
        message: t('auth.tokenDeleteConfirm'),
        header: t('auth.tokenDeleteHeader'),
        icon: 'pi pi-exclamation-triangle',
        acceptClass: '!bg-red-500 hover:!bg-red-600',
        accept: async () => {
            const result = await deviceStore.daemonClient.deleteToken(tokenId)
            if (result instanceof ErrorResponse) {
                toast.add({
                    severity: 'error',
                    summary: t('common.error'),
                    detail: t('auth.tokenDeleteError'),
                    life: 5000,
                })
                return
            }
            toast.add({
                severity: 'success',
                summary: t('common.success'),
                detail: t('auth.tokenDeleted'),
                life: 3000,
            })
            await loadTokens()
        },
    })
}

function copyToken(): void {
    if (!createdToken.value) return
    const text = createdToken.value.token
    if (navigator.clipboard?.writeText) {
        navigator.clipboard.writeText(text).catch(() => fallbackCopy(text))
    } else {
        fallbackCopy(text)
    }
    toast.add({
        severity: 'info',
        summary: t('auth.tokenCopied'),
        life: 2000,
    })
}

function fallbackCopy(text: string): void {
    const textarea = document.createElement('textarea')
    textarea.value = text
    textarea.style.position = 'fixed'
    textarea.style.opacity = '0'
    document.body.appendChild(textarea)
    textarea.select()
    document.execCommand('copy')
    document.body.removeChild(textarea)
}

function formatDate(dateStr: string | null): string {
    if (!dateStr) return t('auth.never')
    const options: Intl.DateTimeFormatOptions = settingsStore.time24 ? { hour12: false } : {}
    return new Date(dateStr).toLocaleString(undefined, options)
}

function isExpired(expiresAt: string | null): boolean {
    if (!expiresAt) return false
    return new Date(expiresAt) <= new Date()
}

function expiryStatus(expiresAt: string | null): { label: string; severity: string } {
    if (!expiresAt) return { label: t('auth.never'), severity: 'info' }
    if (isExpired(expiresAt)) return { label: t('auth.expired'), severity: 'danger' }
    return { label: t('auth.active'), severity: 'success' }
}

onMounted(loadTokens)
</script>

<template>
    <!-- Created token alert -->
    <Message v-if="createdToken" severity="warn" :closable="true" @close="createdToken = null">
        <div class="flex flex-col gap-2">
            <span class="font-semibold">{{ t('auth.tokenCreated') }}</span>
            <span class="text-sm">{{ t('auth.tokenCreatedDetail') }}</span>
            <div class="flex items-center gap-2">
                <code class="bg-bg-one px-2 py-1 rounded-lg text-sm break-all">
                    {{ createdToken.token }}
                </code>
                <Button icon="pi pi-copy" severity="secondary" text rounded @click="copyToken" />
            </div>
        </div>
    </Message>

    <!-- Create form -->
    <div class="flex items-end gap-3 mt-6 mb-4">
        <FloatLabel class="flex-grow">
            <InputText
                id="token-label"
                v-model="newLabel"
                class="w-full h-12"
                :class="{ filled: newLabel.trim().length > 0 }"
                @keydown.enter="createToken"
            />
            <label for="token-label">{{ t('auth.tokenLabel') }}</label>
        </FloatLabel>
        <FloatLabel>
            <DatePicker
                id="token-expiry"
                v-model="newExpiry"
                class="h-12"
                :class="{ filled: newExpiry != null }"
                :min-date="new Date()"
                show-time
                hour-format="24"
                date-format="yy-mm-dd"
            />
            <label for="token-expiry">{{ t('auth.tokenExpiry') }}</label>
        </FloatLabel>
        <Button
            class="!bg-accent/80 hover:!bg-accent/100 h-12"
            :label="t('auth.createToken')"
            icon="pi pi-plus"
            @click="createToken"
            :disabled="newLabel.trim().length === 0"
        />
    </div>

    <!-- Token list -->
    <DataTable
        :value="tokens"
        :loading="loading"
        striped-rows
        size="small"
        :empty-message="t('auth.noTokens')"
    >
        <Column field="label" :header="t('auth.label')" />
        <Column field="created_at" :header="t('auth.created')">
            <template #body="{ data }">{{ formatDate(data.created_at) }}</template>
        </Column>
        <Column field="expires_at" :header="t('auth.expires')">
            <template #body="{ data }">
                <Tag
                    :value="expiryStatus(data.expires_at).label"
                    :severity="expiryStatus(data.expires_at).severity as any"
                />
                <span v-if="data.expires_at" class="ml-2">
                    {{ formatDate(data.expires_at) }}
                </span>
            </template>
        </Column>
        <Column field="last_used" :header="t('auth.lastUsed')">
            <template #body="{ data }">
                {{ data.last_used ? formatDate(data.last_used) : t('auth.neverUsed') }}
            </template>
        </Column>
        <Column :header="t('auth.actions')" style="width: 5rem">
            <template #body="{ data }">
                <Button
                    icon="pi pi-trash"
                    severity="danger"
                    text
                    rounded
                    @click="deleteToken(data.id)"
                />
            </template>
        </Column>
    </DataTable>
</template>

<style scoped lang="scss"></style>
