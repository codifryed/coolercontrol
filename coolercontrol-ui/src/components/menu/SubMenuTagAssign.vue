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
// @ts-ignore
import SvgIcon from '@jamescoyle/vue-icon/lib/svg-icon.vue'
import { mdiTagOutline } from '@mdi/js'
import { UID } from '@/models/Device.ts'
import { useSettingsStore } from '@/stores/SettingsStore.ts'
import { computed, nextTick, ref, Ref, watch } from 'vue'
import Popover from 'primevue/popover'
import Button from 'primevue/button'
import InputText from 'primevue/inputtext'
import CCColorPicker from '@/components/CCColorPicker.vue'
import { useDeviceStore } from '@/stores/DeviceStore.ts'
import { useI18n } from 'vue-i18n'
import { useThemeColorsStore } from '@/stores/ThemeColorsStore.ts'
import { VueDraggable } from 'vue-draggable-plus'
import { TagSettings } from '@/models/UISettings.ts'

interface Props {
    deviceUID: UID
    channelName: string
}

const props = defineProps<Props>()
const emit = defineEmits<{
    (e: 'open', value: boolean): void
    (e: 'close'): void
}>()

const settingsStore = useSettingsStore()
const deviceStore = useDeviceStore()
const colorStore = useThemeColorsStore()
const { t } = useI18n()

const DEFAULT_TAG_COLOR = '#568af2'
const MAX_TAG_NAME_LENGTH = 15

const popRef = ref()
const popContent = ref()
const newTagName: Ref<string> = ref('')
const newTagColor: Ref<string> = ref(DEFAULT_TAG_COLOR)

interface TagEntry {
    name: string
    settings: TagSettings
}

const tagList: Ref<Array<TagEntry>> = ref([])

const syncTagList = (): void => {
    tagList.value = Array.from(settingsStore.tags.entries()).map(([name, settings]) => ({
        name,
        settings,
    }))
}
syncTagList()

watch(() => settingsStore.tags, syncTagList, { deep: true })

const onDragEnd = (): void => {
    settingsStore.reorderTags(tagList.value.map((entry) => entry.name))
    nextTick(() => {
        popContent.value?.click()
    })
}

// Edit state
const editingTagName: Ref<string | null> = ref(null)
const editName: Ref<string> = ref('')
const editColor: Ref<string> = ref('')
const editNameInput = ref()

const assignedTags = computed(() =>
    settingsStore.getChannelTags(props.deviceUID, props.channelName),
)

const isAssigned = (tagName: string): boolean => assignedTags.value.includes(tagName)

const toggleTag = (tagName: string): void => {
    if (isAssigned(tagName)) {
        settingsStore.removeTagFromChannel(props.deviceUID, props.channelName, tagName)
    } else {
        settingsStore.assignTagToChannel(props.deviceUID, props.channelName, tagName)
    }
}

const createAndAssign = (): void => {
    const name = newTagName.value.trim()
    if (name.length === 0) return
    if (!settingsStore.tags.has(name)) {
        settingsStore.createTag(name, newTagColor.value)
    }
    settingsStore.assignTagToChannel(props.deviceUID, props.channelName, name)
    newTagName.value = ''
    newTagColor.value = DEFAULT_TAG_COLOR
}

const startEdit = (tagName: string): void => {
    const tag = settingsStore.tags.get(tagName)
    if (tag == null) return
    editingTagName.value = tagName
    editName.value = tagName
    editColor.value = colorStore.rgbToHex(tag.color)
    nextTick(() => {
        const inputRef = Array.isArray(editNameInput.value)
            ? editNameInput.value[0]
            : editNameInput.value
        const el = inputRef?.$el ?? inputRef
        el?.focus()
    })
}

const saveEdit = (): void => {
    if (editingTagName.value == null) return
    const oldName = editingTagName.value
    const newName = editName.value.trim()
    if (newName.length === 0) {
        cancelEdit()
        return
    }
    settingsStore.updateTagColor(oldName, editColor.value)
    if (newName !== oldName) {
        settingsStore.renameTag(oldName, newName)
    }
    editingTagName.value = null
}

const cancelEdit = (): void => {
    editingTagName.value = null
}

const deleteTag = (tagName: string): void => {
    settingsStore.deleteTag(tagName)
}

const newTagValid = computed(
    () =>
        newTagName.value.trim().length > 0 && newTagName.value.trim().length < MAX_TAG_NAME_LENGTH,
)

const onPopoverClose = (): void => {
    editingTagName.value = null
    emit('open', false)
    emit('close')
}
</script>

<template>
    <Button
        class="w-full !justify-start !rounded-lg border-none text-text-color-secondary h-12 !p-4 !px-7 hover:text-text-color hover:bg-surface-hover outline-none"
        @click.stop.prevent="(event) => popRef.toggle(event)"
    >
        <svg-icon
            class="outline-0"
            type="mdi"
            :path="mdiTagOutline"
            :size="deviceStore.getREMSize(1.5)"
        />
        <span class="ml-1.5">
            {{ t('layout.menu.tooltips.tags') }}
        </span>
    </Button>
    <Popover ref="popRef" @show="emit('open', true)" @hide="onPopoverClose">
        <div
            ref="popContent"
            class="w-full bg-bg-two border border-border-one p-4 rounded-lg text-text-color"
        >
            <span class="text-xl font-bold">{{ t('components.menuTagAssign.title') }}</span>
            <VueDraggable
                v-model="tagList"
                :animation="200"
                handle=".tag-drag-handle"
                class="mt-4 flex flex-col gap-y-1"
                :force-fallback="true"
                :fallback-tolerance="15"
                @end="onDragEnd"
            >
                <!--                :fallback-on-body="true"-->
                <!--                :revert-on-spill="true"-->
                <div v-for="entry in tagList" :key="entry.name">
                    <!-- Edit mode -->
                    <div
                        v-if="editingTagName === entry.name"
                        class="flex flex-col gap-y-2 p-2 rounded bg-bg-one"
                    >
                        <div class="flex items-center gap-x-2">
                            <c-c-color-picker
                                v-model="editColor"
                                color-format="hex"
                                :default-color="editColor"
                                :size="2"
                                @open="(isOpen) => emit('open', isOpen)"
                                @click.stop
                            />
                            <InputText
                                ref="editNameInput"
                                v-model="editName"
                                :maxlength="MAX_TAG_NAME_LENGTH"
                                class="flex-1 h-8"
                                @keydown.enter.prevent="saveEdit"
                                @keydown.escape.prevent="cancelEdit"
                                @click.stop
                            />
                        </div>
                        <div class="flex justify-end gap-x-2">
                            <Button class="h-7 px-2 bg-bg-two text-xs" @click.stop="cancelEdit">
                                {{ t('common.cancel') }}
                            </Button>
                            <Button
                                class="h-7 px-2 bg-accent/80 hover:!bg-accent text-xs"
                                @click.stop="saveEdit"
                            >
                                {{ t('common.save') }}
                            </Button>
                        </div>
                    </div>
                    <!-- Normal mode -->
                    <div
                        v-else
                        class="group/tag flex items-center gap-x-2 rounded px-2 py-1 hover:bg-bg-one select-none"
                    >
                        <span
                            class="tag-drag-handle pi pi-bars text-xs text-text-color-secondary cursor-grab"
                        />
                        <div
                            class="flex items-center gap-x-2 flex-1 cursor-pointer"
                            @click="toggleTag(entry.name)"
                        >
                            <span
                                class="pi"
                                :class="isAssigned(entry.name) ? 'pi-check-square' : 'pi-stop'"
                            />
                            <span
                                class="w-3 h-3 rounded-full inline-block"
                                :style="{
                                    backgroundColor: colorStore.rgbToHex(entry.settings.color),
                                }"
                            />
                            <span>{{ entry.name }}</span>
                        </div>
                        <div class="hidden group-hover/tag:flex items-center gap-x-1">
                            <span
                                v-tooltip.top="t('components.menuTagAssign.editTag')"
                                class="pi pi-pencil text-xs text-text-color-secondary hover:text-text-color cursor-pointer p-0.5"
                                @click="startEdit(entry.name)"
                            />
                            <span
                                v-tooltip.top="t('components.menuTagAssign.deleteTag')"
                                class="pi pi-trash text-xs text-text-color-secondary hover:text-error cursor-pointer p-0.5"
                                @click="deleteTag(entry.name)"
                            />
                        </div>
                    </div>
                </div>
            </VueDraggable>
            <div
                v-if="settingsStore.tags.size === 0"
                class="mt-4 text-text-color-secondary text-sm"
            >
                {{ t('components.menuTagAssign.noTags') }}
            </div>
            <div class="mt-4 border-t border-border-one pt-3">
                <span class="text-sm font-medium">{{ t('components.menuTagAssign.newTag') }}</span>
                <div class="flex items-center gap-x-2 mt-2">
                    <c-c-color-picker
                        v-model="newTagColor"
                        color-format="hex"
                        :default-color="DEFAULT_TAG_COLOR"
                        :size="2"
                        @open="(isOpen) => emit('open', isOpen)"
                        @click.stop
                    />
                    <InputText
                        v-model="newTagName"
                        :placeholder="t('components.menuTagAssign.tagName')"
                        :maxlength="MAX_TAG_NAME_LENGTH"
                        class="flex-1 h-8 w-48"
                        @keydown.enter.prevent="createAndAssign"
                        @click.stop
                    />
                    <Button
                        class="h-8 bg-accent/80 hover:!bg-accent px-2"
                        :disabled="!newTagValid"
                        @click.stop="createAndAssign"
                    >
                        {{ t('common.add') }}
                    </Button>
                </div>
            </div>
        </div>
    </Popover>
</template>

<style scoped lang="scss">
.sortable-fallback {
    color: rgb(var(--colors-text-color));
    background-color: rgba(var(--colors-bg-two) / 0.625);
    border-radius: 0.5rem;
}
</style>
