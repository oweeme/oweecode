<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { save as saveDialog } from '@tauri-apps/plugin-dialog'
import { useEditorStore } from '../composables/useEditorStore'
import { useContainerStore, type ContainerInfo } from '../composables/useContainerStore'
import { useI18n } from '../composables/useI18n'
import ContainerFormModal from './ContainerFormModal.vue'

interface PortMapping { host: string; container: string; protocol: string }
interface VolumeMapping { host: string; container: string }
interface EnvVar { key: string; value: string }
interface ContainerOpts {
  name: string; image: string; ports: PortMapping[]; volumes: VolumeMapping[]
  env: EnvVar[]; command: string; restart_policy: string; network: string; workdir: string
}

const store = useEditorStore()
const { t } = useI18n()
const { containers, runtime, runtimeError, loading } = useContainerStore()

const showAll = ref(true)
const busyId = ref<string | null>(null)
const actionError = ref('')
// Containers the agent creates get labeled with the project's root path
// (see agentTools.ts's container_create) — this just asks Podman/Docker to
// filter by that label instead of listing every container on the system, so
// "where did my container go" has a direct answer for anyone who used the
// agent (or hand-wrote a matching --label) to create one.
const projectOnly = ref(false)

const showFormModal = ref(false)
const formMode = ref<'create' | 'edit'>('create')
const editContainerId = ref<string | undefined>(undefined)
const editInitial = ref<ContainerOpts | undefined>(undefined)

const visibleContainers = computed(() =>
  showAll.value ? containers.value : containers.value.filter(c => c.running)
)

async function refresh() {
  loading.value = true
  actionError.value = ''
  try {
    if (!runtime.value) runtime.value = await invoke<string>('container_runtime')
    containers.value = projectOnly.value && store.state.rootPath
      ? await invoke<ContainerInfo[]>('container_list_for_project', { root: store.state.rootPath, all: showAll.value })
      : await invoke<ContainerInfo[]>('container_list', { all: showAll.value })
    runtimeError.value = ''
  } catch (e: any) {
    runtimeError.value = String(e)
    containers.value = []
  } finally {
    loading.value = false
  }
}

async function toggleRunning(id: string, running: boolean) {
  busyId.value = id
  actionError.value = ''
  try {
    await invoke(running ? 'container_stop' : 'container_start', { id })
    await refresh()
  } catch (e: any) { actionError.value = String(e) }
  finally { busyId.value = null }
}

async function restart(id: string) {
  busyId.value = id
  actionError.value = ''
  try {
    await invoke('container_restart', { id })
    await refresh()
  } catch (e: any) { actionError.value = String(e) }
  finally { busyId.value = null }
}

async function remove(id: string, running: boolean) {
  if (!confirm(t('confirmRemoveContainer'))) return
  busyId.value = id
  actionError.value = ''
  try {
    await invoke('container_remove', { id, force: running })
    await refresh()
  } catch (e: any) { actionError.value = String(e) }
  finally { busyId.value = null }
}

function openLogs(id: string, name: string) {
  store.openContainerLogs(id, name)
}

// Commits the container's current state (not the original image — whatever's
// changed since it started) and saves it as a portable .tar a coworker can
// load with `podman/docker load -i` to get your exact working environment.
async function exportContainer(id: string, name: string) {
  const path = await saveDialog({
    defaultPath: `${name.replace(/[^a-z0-9_-]+/gi, '_')}.tar`,
    filters: [{ name: 'Podman/Docker image', extensions: ['tar'] }],
  })
  if (!path) return
  busyId.value = id
  actionError.value = ''
  try {
    await invoke('container_export', { id, name, outPath: path })
    actionError.value = `✓ ${t('containerExported')}`
  } catch (e: any) { actionError.value = String(e) }
  finally { busyId.value = null }
}

function openCreateModal() {
  formMode.value = 'create'
  editContainerId.value = undefined
  editInitial.value = undefined
  actionError.value = ''
  showFormModal.value = true
}

async function openEditModal(id: string) {
  actionError.value = ''
  try {
    const detail = await invoke<ContainerOpts>('container_inspect', { id })
    formMode.value = 'edit'
    editContainerId.value = id
    editInitial.value = detail
    showFormModal.value = true
  } catch (e: any) { actionError.value = String(e) }
}

async function onFormSaved() {
  showFormModal.value = false
  await refresh()
}

onMounted(refresh)
</script>

<template>
  <div class="ctr-panel">
    <div class="ctr-header">
      <span class="ctr-title">{{ t('containers') }}</span>
      <div class="ctr-header-btns">
        <button
          v-if="store.state.rootPath"
          class="ctr-icon-btn" :class="{ active: projectOnly }"
          @click="projectOnly = !projectOnly; refresh()"
          :title="'Mostrar solo contenedores creados para este proyecto'"
        >
          {{ projectOnly ? 'Este proyecto' : 'Todos los proyectos' }}
        </button>
        <button class="ctr-icon-btn" :class="{ active: showAll }" @click="showAll = !showAll; refresh()" :title="t('showAllContainers')">
          {{ showAll ? t('allLabel') : t('runningLabel') }}
        </button>
        <button class="ctr-icon-btn" @click="refresh" :title="t('refreshBtn')" :disabled="loading">
          <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2v1z"/>
            <path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466z"/>
          </svg>
        </button>
        <button class="ctr-icon-btn" @click="openCreateModal" :title="t('createContainer')">+</button>
      </div>
    </div>

    <div v-if="runtime && !runtimeError" class="ctr-runtime-badge">{{ t('usingRuntime') }}: {{ runtime }}</div>

    <div v-if="runtimeError" class="ctr-error">
      {{ runtimeError }}
      <div class="ctr-error-hint">{{ t('installPodmanHint') }}</div>
    </div>
    <div v-if="actionError" class="ctr-error">{{ actionError }}</div>

    <div v-if="loading && containers.length === 0" class="ctr-loading"><span class="ctr-spinner" /> {{ t('loading') }}</div>

    <div v-else-if="!runtimeError" class="ctr-list">
      <div v-if="visibleContainers.length === 0" class="ctr-empty">
        <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" opacity=".3">
          <rect x="2" y="7" width="20" height="13" rx="2"/><path d="M6 7V4a2 2 0 012-2h8a2 2 0 012 2v3"/><path d="M2 11h20"/>
        </svg>
        <span>{{ t('noContainers') }}</span>
      </div>

      <div v-for="c in visibleContainers" :key="c.id" class="ctr-row" :class="{ running: c.running }">
        <span class="ctr-dot" :class="{ on: c.running }" />
        <div class="ctr-info" @click="openLogs(c.id, c.name)">
          <div class="ctr-name">{{ c.name }}</div>
          <div class="ctr-meta">{{ c.image }}</div>
          <div class="ctr-meta ctr-status">{{ c.status }}<span v-if="c.ports"> · {{ c.ports }}</span></div>
        </div>
        <div class="ctr-actions">
          <button
            class="ctr-action-btn"
            :class="{ on: c.running }"
            @click.stop="toggleRunning(c.id, c.running)"
            :disabled="busyId === c.id"
            :title="c.running ? t('stopContainer') : t('startContainer')"
          >
            <span v-if="busyId === c.id" class="ctr-spin" />
            <svg v-else-if="c.running" width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><rect x="3" y="3" width="10" height="10" rx="1"/></svg>
            <svg v-else width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M4 2.5v11l9-5.5-9-5.5z"/></svg>
          </button>
          <button class="ctr-action-btn" @click.stop="restart(c.id)" :disabled="busyId === c.id" :title="t('restartContainer')">
            <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
              <path d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2v1z"/>
              <path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466z"/>
            </svg>
          </button>
          <button class="ctr-action-btn" @click.stop="openLogs(c.id, c.name)" :title="t('viewLogs')">
            <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
              <path d="M2 2.5A1.5 1.5 0 013.5 1h9A1.5 1.5 0 0114 2.5v11a1.5 1.5 0 01-1.5 1.5h-9A1.5 1.5 0 012 13.5v-11zM4 4v1h8V4H4zm0 3v1h8V7H4zm0 3v1h5v-1H4z"/>
            </svg>
          </button>
          <button class="ctr-action-btn" @click.stop="exportContainer(c.id, c.name)" :disabled="busyId === c.id" :title="t('exportContainer')">
            <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
              <path d="M8 1a.5.5 0 01.5.5v9.793l2.146-2.147a.5.5 0 01.708.708l-3 3a.5.5 0 01-.708 0l-3-3a.5.5 0 11.708-.708L7.5 11.293V1.5A.5.5 0 018 1z"/>
              <path d="M3 13.5a.5.5 0 01.5-.5h9a.5.5 0 010 1h-9a.5.5 0 01-.5-.5z"/>
            </svg>
          </button>
          <button class="ctr-action-btn" @click.stop="openEditModal(c.id)" :title="t('editContainer')">
            <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
              <path d="M12.854.146a.5.5 0 00-.707 0L10.5 1.793 14.207 5.5l1.647-1.646a.5.5 0 000-.708l-3-3zm.646 6.061L9.793 2.5 3.293 9H3.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.207l6.5-6.5zm-7.468 7.468A.5.5 0 015.5 14H5v-.5a.5.5 0 01-.5-.5H4v-.5a.5.5 0 01-.5-.5H3v-.5a.5.5 0 01-.5-.5H2.5v-.5a.5.5 0 01.646-.473l.853.214 1.832-1.832 3.709 3.709-1.832 1.832.214.853z"/>
            </svg>
          </button>
          <button class="ctr-action-btn ctr-action-del" @click.stop="remove(c.id, c.running)" :disabled="busyId === c.id" :title="t('removeContainer')">
            <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
              <path d="M5.5 5.5A.5.5 0 016 6v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm2.5 0a.5.5 0 01.5.5v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm3 .5a.5.5 0 00-1 0v6a.5.5 0 001 0V6z"/>
              <path fill-rule="evenodd" d="M14.5 3a1 1 0 01-1 1H13v9a2 2 0 01-2 2H5a2 2 0 01-2-2V4h-.5a1 1 0 010-2h4a1 1 0 011-1h2a1 1 0 011 1h4a1 1 0 011 1zM4.118 4L4 4.059V13a1 1 0 001 1h6a1 1 0 001-1V4.059L11.882 4H4.118zM2.5 3a.5.5 0 000 1h11a.5.5 0 000-1h-11z"/>
            </svg>
          </button>
        </div>
      </div>
    </div>

    <ContainerFormModal
      v-if="showFormModal"
      :mode="formMode"
      :container-id="editContainerId"
      :initial="editInitial"
      @close="showFormModal = false"
      @saved="onFormSaved"
    />
  </div>
</template>

<style scoped>
.ctr-panel { display: flex; flex-direction: column; height: 100%; font-family: var(--font-ui); }

.ctr-header {
  display: flex; align-items: center; justify-content: space-between;
  padding: 0 10px; height: 36px; flex-shrink: 0; border-bottom: 1px solid var(--border);
}
.ctr-title { font-size: 10px; font-weight: 700; letter-spacing: 1px; color: var(--fg-muted); text-transform: uppercase; }
.ctr-header-btns { display: flex; gap: 6px; }
.ctr-icon-btn {
  height: 22px; padding: 0 7px; border-radius: 5px; background: none; border: 1px solid var(--border);
  color: var(--fg-muted); font-size: 10.5px; cursor: pointer; display: flex; align-items: center; justify-content: center; gap: 4px;
}
.ctr-icon-btn:hover { background: var(--bg-hover); color: var(--fg); }
.ctr-icon-btn.active { color: var(--accent); border-color: var(--accent); }
.ctr-icon-btn:disabled { opacity: 0.5; cursor: default; }

.ctr-runtime-badge { margin: 8px 12px 0; font-size: 10.5px; color: var(--fg-muted); }

.ctr-error {
  margin: 8px 12px; padding: 8px 10px; background: rgba(248,81,73,0.12);
  border: 1px solid rgba(248,81,73,0.3); border-radius: 6px;
  font-size: 11px; color: #f85149; line-height: 1.5;
}
.ctr-error-hint { margin-top: 4px; color: var(--fg-muted); }

.ctr-loading { display: flex; align-items: center; gap: 8px; padding: 16px; color: var(--fg-muted); font-size: 12px; }
.ctr-spinner {
  width: 12px; height: 12px; border: 2px solid rgba(208,208,216,0.3);
  border-top-color: var(--accent); border-radius: 50%; animation: spin 0.7s linear infinite; display: inline-block;
}
@keyframes spin { to { transform: rotate(360deg); } }

.ctr-list { flex: 1; overflow-y: auto; padding: 6px 0; }
.ctr-empty {
  display: flex; flex-direction: column; align-items: center; gap: 10px;
  padding: 32px 16px; color: var(--fg-muted); font-size: 12px;
}

.ctr-row {
  display: flex; align-items: center; gap: 8px;
  padding: 7px 10px 7px 9px; border-left: 3px solid transparent;
  transition: background 0.15s, border-color 0.15s; border-radius: 4px; margin: 1px 4px;
}
.ctr-row:hover { background: var(--bg-hover); }
.ctr-row.running { background: rgba(166,227,161,0.06); border-left-color: var(--green); }
.ctr-row.running:hover { background: rgba(166,227,161,0.12); }

.ctr-dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; background: var(--fg-muted); opacity: 0.4; }
.ctr-dot.on { background: var(--green); opacity: 1; box-shadow: 0 0 5px rgba(166,227,161,0.7); }

.ctr-info { flex: 1; min-width: 0; cursor: pointer; }
.ctr-name { font-size: 12.5px; font-weight: 600; color: var(--fg); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.ctr-meta { font-size: 10.5px; color: var(--fg-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 1px; }
.ctr-status { font-family: var(--font-mono); }

.ctr-actions { display: flex; gap: 2px; flex-shrink: 0; }
.ctr-action-btn {
  width: 22px; height: 22px; background: none; border: none;
  border-radius: 4px; color: var(--fg-muted); cursor: pointer;
  display: flex; align-items: center; justify-content: center; transition: all 0.1s;
}
.ctr-action-btn:hover { background: var(--bg-active); color: var(--fg); }
.ctr-action-btn.on { color: var(--green); }
.ctr-action-btn:disabled { opacity: 0.5; cursor: default; }
.ctr-action-del:hover { color: #f85149 !important; }
.ctr-spin {
  width: 10px; height: 10px; border: 2px solid rgba(208,208,216,0.3);
  border-top-color: var(--accent); border-radius: 50%; animation: spin 0.7s linear infinite;
}
</style>
