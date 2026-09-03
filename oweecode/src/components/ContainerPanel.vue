<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { save as saveDialog, open as openDialog } from '@tauri-apps/plugin-dialog'
import { listen } from '@tauri-apps/api/event'
import { useEditorStore } from '../composables/useEditorStore'
import { useContainerStore, type ContainerInfo, type PodInfo } from '../composables/useContainerStore'
import { useI18n } from '../composables/useI18n'
import ContainerFormModal from './ContainerFormModal.vue'
import PodFormModal from './PodFormModal.vue'
import ContainerRow from './ContainerRow.vue'
import PodmanSetup from './PodmanSetup.vue'
import PodExportModal from './PodExportModal.vue'

interface PortMapping { host: string; container: string; protocol: string }
interface VolumeMapping { host: string; container: string }
interface EnvVar { key: string; value: string }
interface ContainerOpts {
  name: string; image: string; ports: PortMapping[]; volumes: VolumeMapping[]
  env: EnvVar[]; command: string; restart_policy: string; network: string; workdir: string; pod?: string
}

const store = useEditorStore()
const { t } = useI18n()
const { containers, pods, runtime, runtimeError, loading } = useContainerStore()

const showAll = ref(true)
const busyId = ref<string | null>(null)
const podBusyId = ref<string | null>(null)
const pruning = ref(false)
const exportingPodId = ref<string | null>(null)
const importingPod = ref(false)
const podProgress = ref('')
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
const showPodFormModal = ref(false)
const showPodmanSetup = ref(false)
const exportModalPod = ref<PodInfo | null>(null)

const visibleContainers = computed(() =>
  showAll.value ? containers.value : containers.value.filter(c => c.running)
)

// Containers already shown inside a pod card are hidden from the flat list
// below — matched by name (pod-member ids come from `podman pod ps`'s JSON,
// which reports full ids, while `container ps` truncates — name is the only
// field guaranteed to line up between the two).
const podMemberNames = computed(() => new Set(pods.value.flatMap(p => p.containers.map(c => c.name))))
const standaloneContainers = computed(() => visibleContainers.value.filter(c => !podMemberNames.value.has(c.name)))

// Pod cards render full ContainerRow entries (ports, image, status, all the
// same actions as a standalone container) rather than bare name pills, so
// looking up "what port is this on" or editing/opening a shell works the
// same whether a container is in a pod or not. Matched by name against the
// already-fetched `containers` list, which carries the full ContainerInfo
// that `podman pod ps`'s nested Containers array doesn't.
const containersByPod = computed(() => {
  const map = new Map<string, ContainerInfo[]>()
  for (const p of pods.value) {
    const names = new Set(p.containers.map(m => m.name))
    map.set(p.id, containers.value.filter(c => names.has(c.name)))
  }
  return map
})

async function refresh() {
  loading.value = true
  actionError.value = ''
  try {
    if (!runtime.value) runtime.value = await invoke<string>('container_runtime')
    containers.value = projectOnly.value && store.state.rootPath
      ? await invoke<ContainerInfo[]>('container_list_for_project', { root: store.state.rootPath, all: showAll.value })
      : await invoke<ContainerInfo[]>('container_list', { all: showAll.value })
    // pod_list(_for_project) silently returns [] under Docker, so this is
    // safe to call regardless of runtime — no need to gate it here.
    pods.value = projectOnly.value && store.state.rootPath
      ? await invoke<PodInfo[]>('pod_list_for_project', { root: store.state.rootPath })
      : await invoke<PodInfo[]>('pod_list')
    runtimeError.value = ''
  } catch (e: any) {
    runtimeError.value = String(e)
    containers.value = []
    pods.value = []
  } finally {
    loading.value = false
  }
}

function isPodRunning(status: string) {
  return status.toLowerCase() === 'running'
}

async function togglePodRunning(id: string, running: boolean) {
  podBusyId.value = id
  actionError.value = ''
  try {
    await invoke(running ? 'pod_stop' : 'pod_start', { id })
    await refresh()
  } catch (e: any) { actionError.value = String(e) }
  finally { podBusyId.value = null }
}

async function removePod(id: string, running: boolean) {
  if (!confirm(t('confirmRemoveContainer'))) return
  podBusyId.value = id
  actionError.value = ''
  try {
    await invoke('pod_remove', { id, force: running })
    await refresh()
  } catch (e: any) { actionError.value = String(e) }
  finally { podBusyId.value = null }
}

function openPodCreateModal() {
  actionError.value = ''
  showPodFormModal.value = true
}

async function onPodFormCreated() {
  showPodFormModal.value = false
  await refresh()
}

// Bundles a pod for migration, written to a folder the user picks. Two
// modes (chosen in PodExportModal): "full" commits + saves each member
// container as its own image tar (offline-restorable, but bundles the base
// images too); "light" keeps each container's original image reference and
// only exports its named volumes (small, but the destination re-pulls the
// images — needs internet there). pod_import reverses either.
function openExportModal(pod: PodInfo) {
  exportModalPod.value = pod
}

async function exportPod(pod: PodInfo, full: boolean) {
  exportModalPod.value = null
  const dir = await openDialog({ directory: true, multiple: false, title: `${t('exportPod')}: ${pod.name}` })
  if (!dir || typeof dir !== 'string') return
  const memberIds = (containersByPod.value.get(pod.id) ?? []).map(c => c.id)
  if (memberIds.length === 0) return
  exportingPodId.value = pod.id
  podProgress.value = ''
  actionError.value = ''
  const unlisten = await listen<string>('pod-export-progress', (ev) => { podProgress.value = ev.payload })
  try {
    await invoke('pod_export', { id: pod.id, podName: pod.name, containerIds: memberIds, outDir: dir, full })
    actionError.value = `✓ ${t('podExported')}: ${dir}`
  } catch (e: any) { actionError.value = String(e) }
  finally { exportingPodId.value = null; podProgress.value = ''; unlisten() }
}

async function importPod() {
  const manifestPath = await openDialog({
    multiple: false,
    filters: [{ name: 'pod-manifest.json', extensions: ['json'] }],
  })
  if (!manifestPath || typeof manifestPath !== 'string') return
  importingPod.value = true
  podProgress.value = ''
  actionError.value = ''
  const unlisten = await listen<string>('pod-import-progress', (ev) => { podProgress.value = ev.payload })
  try {
    const name = await invoke<string>('pod_import', { manifestPath })
    await refresh()
    actionError.value = `✓ ${t('podImported')}: ${name}`
  } catch (e: any) { actionError.value = String(e) }
  finally { importingPod.value = false; podProgress.value = ''; unlisten() }
}

// Mirrors Podman Desktop's "clean up": drops every stopped container plus
// any pod left with zero containers. Running containers/pods are untouched.
async function pruneUnused() {
  if (!confirm(t('confirmPruneUnused'))) return
  pruning.value = true
  actionError.value = ''
  try {
    const label = projectOnly.value && store.state.rootPath ? store.state.rootPath : undefined
    await invoke('container_prune', { label })
    if (runtime.value === 'podman') await invoke('pod_prune')
    await refresh()
    actionError.value = `✓ ${t('pruneDone')}`
  } catch (e: any) { actionError.value = String(e) }
  finally { pruning.value = false }
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

function openShell(id: string, name: string) {
  store.openContainerShell(id, name)
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
          :title="t('projectOnlyHint')"
        >
          {{ projectOnly ? t('thisProjectLabel') : t('allProjectsLabel') }}
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
        <button class="ctr-icon-btn" @click="pruneUnused" :title="t('pruneUnused')" :disabled="pruning">
          <span v-if="pruning" class="ctr-spin" />
          <svg v-else width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
            <path d="M5.5 5.5A.5.5 0 016 6v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm2.5 0a.5.5 0 01.5.5v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm3 .5a.5.5 0 00-1 0v6a.5.5 0 001 0V6z"/>
            <path fill-rule="evenodd" d="M14.5 3a1 1 0 01-1 1H13v9a2 2 0 01-2 2H5a2 2 0 01-2-2V4h-.5a1 1 0 010-2h4a1 1 0 011-1h2a1 1 0 011 1h4a1 1 0 011 1zM4.118 4L4 4.059V13a1 1 0 001 1h6a1 1 0 001-1V4.059L11.882 4H4.118zM2.5 3a.5.5 0 000 1h11a.5.5 0 000-1h-11z"/>
          </svg>
        </button>
        <button v-if="runtime === 'podman'" class="ctr-icon-btn" @click="importPod" :title="t('importPod')" :disabled="importingPod">
          <span v-if="importingPod" class="ctr-spin" />
          <svg v-else width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 1a.5.5 0 01.5.5v8.793l2.146-2.147a.5.5 0 01.708.708l-3 3a.5.5 0 01-.708 0l-3-3a.5.5 0 11.708-.708L7.5 10.293V1.5A.5.5 0 018 1z"/>
            <path d="M3 12.5a.5.5 0 01.5-.5h9a.5.5 0 01.5.5v1a1.5 1.5 0 01-1.5 1.5h-7A1.5 1.5 0 013 13.5v-1z"/>
          </svg>
        </button>
        <button v-if="runtime === 'podman'" class="ctr-icon-btn" @click="openPodCreateModal" :title="t('createPodTitle')">+ {{ t('createPod') }}</button>
        <button class="ctr-icon-btn" @click="openCreateModal" :title="t('createContainer')">+</button>
      </div>
    </div>

    <div v-if="runtime && !runtimeError" class="ctr-runtime-badge">{{ t('usingRuntime') }}: {{ runtime }}</div>

    <div v-if="runtimeError" class="ctr-error">
      {{ runtimeError }}
      <div class="ctr-error-hint">{{ t('installPodmanHint') }}</div>
      <button class="ctr-install-btn" @click="showPodmanSetup = true">⚙ {{ t('installPodmanTitle') }}</button>
    </div>
    <div v-if="actionError" class="ctr-error">{{ actionError }}</div>

    <div class="ctr-scroll">
    <div v-if="pods.length > 0" class="ctr-pods">
      <div class="ctr-pods-title">{{ t('pods') }}</div>
      <div v-for="p in pods" :key="p.id" class="ctr-pod-card">
        <div class="ctr-pod-head">
          <span class="ctr-dot" :class="{ on: isPodRunning(p.status) }" />
          <span class="ctr-pod-name">{{ p.name }}</span>
          <span class="ctr-meta">{{ p.status }}<span v-if="exportingPodId === p.id && podProgress"> · {{ podProgress }}</span></span>
          <div class="ctr-actions">
            <button
              class="ctr-action-btn" :class="{ on: isPodRunning(p.status) }"
              @click.stop="togglePodRunning(p.id, isPodRunning(p.status))"
              :disabled="podBusyId === p.id"
              :title="isPodRunning(p.status) ? t('stopPod') : t('startPod')"
            >
              <span v-if="podBusyId === p.id" class="ctr-spin" />
              <svg v-else-if="isPodRunning(p.status)" width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><rect x="3" y="3" width="10" height="10" rx="1"/></svg>
              <svg v-else width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M4 2.5v11l9-5.5-9-5.5z"/></svg>
            </button>
            <button
              class="ctr-action-btn"
              @click.stop="openExportModal(p)"
              :disabled="exportingPodId === p.id"
              :title="t('exportPod')"
            >
              <span v-if="exportingPodId === p.id" class="ctr-spin" />
              <svg v-else width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
                <path d="M8 1a.5.5 0 01.5.5v9.793l2.146-2.147a.5.5 0 01.708.708l-3 3a.5.5 0 01-.708 0l-3-3a.5.5 0 11.708-.708L7.5 11.293V1.5A.5.5 0 018 1z"/>
                <path d="M3 13.5a.5.5 0 01.5-.5h9a.5.5 0 010 1h-9a.5.5 0 01-.5-.5z"/>
              </svg>
            </button>
            <button class="ctr-action-btn ctr-action-del" @click.stop="removePod(p.id, isPodRunning(p.status))" :disabled="podBusyId === p.id" :title="t('removePod')">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
                <path d="M5.5 5.5A.5.5 0 016 6v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm2.5 0a.5.5 0 01.5.5v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm3 .5a.5.5 0 00-1 0v6a.5.5 0 001 0V6z"/>
                <path fill-rule="evenodd" d="M14.5 3a1 1 0 01-1 1H13v9a2 2 0 01-2 2H5a2 2 0 01-2-2V4h-.5a1 1 0 010-2h4a1 1 0 011-1h2a1 1 0 011 1h4a1 1 0 011 1zM4.118 4L4 4.059V13a1 1 0 001 1h6a1 1 0 001-1V4.059L11.882 4H4.118zM2.5 3a.5.5 0 000 1h11a.5.5 0 000-1h-11z"/>
              </svg>
            </button>
          </div>
        </div>
        <div v-if="(containersByPod.get(p.id) ?? []).length > 0" class="ctr-pod-rows">
          <ContainerRow
            v-for="c in containersByPod.get(p.id)" :key="c.id"
            :container="c" :busy="busyId === c.id"
            @toggle="toggleRunning(c.id, c.running)"
            @restart="restart(c.id)"
            @logs="openLogs(c.id, c.name)"
            @shell="openShell(c.id, c.name)"
            @export-img="exportContainer(c.id, c.name)"
            @edit="openEditModal(c.id)"
            @remove="remove(c.id, c.running)"
          />
        </div>
        <div v-else class="ctr-pod-empty">{{ t('podEmptyHint') }}</div>
      </div>
    </div>

    <div v-if="loading && containers.length === 0" class="ctr-loading"><span class="ctr-spinner" /> {{ t('loading') }}</div>

    <div v-else-if="!runtimeError" class="ctr-list">
      <div v-if="pods.length > 0 && standaloneContainers.length > 0" class="ctr-pods-title ctr-standalone-title">{{ t('standaloneContainers') }}</div>
      <div v-if="standaloneContainers.length === 0 && pods.length === 0" class="ctr-empty">
        <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" opacity=".3">
          <rect x="2" y="7" width="20" height="13" rx="2"/><path d="M6 7V4a2 2 0 012-2h8a2 2 0 012 2v3"/><path d="M2 11h20"/>
        </svg>
        <span>{{ t('noContainers') }}</span>
      </div>

      <ContainerRow
        v-for="c in standaloneContainers" :key="c.id"
        :container="c" :busy="busyId === c.id"
        @toggle="toggleRunning(c.id, c.running)"
        @restart="restart(c.id)"
        @logs="openLogs(c.id, c.name)"
        @shell="openShell(c.id, c.name)"
        @export-img="exportContainer(c.id, c.name)"
        @edit="openEditModal(c.id)"
        @remove="remove(c.id, c.running)"
      />
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
    <PodFormModal
      v-if="showPodFormModal"
      :root-path="store.state.rootPath || ''"
      @close="showPodFormModal = false"
      @created="onPodFormCreated"
    />
    <PodmanSetup
      v-if="showPodmanSetup"
      @close="showPodmanSetup = false; refresh()"
    />
    <PodExportModal
      v-if="exportModalPod"
      :pod-name="exportModalPod.name"
      @close="exportModalPod = null"
      @confirm="(full) => exportPod(exportModalPod!, full)"
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
.ctr-install-btn {
  margin-top: 8px; background: var(--accent); border: none; border-radius: 6px;
  color: var(--accent-fg); font-size: 11px; font-weight: 600; padding: 6px 12px; cursor: pointer;
}
.ctr-install-btn:hover { opacity: 0.85; }

.ctr-loading { display: flex; align-items: center; gap: 8px; padding: 16px; color: var(--fg-muted); font-size: 12px; }
.ctr-spinner {
  width: 12px; height: 12px; border: 2px solid rgba(208,208,216,0.3);
  border-top-color: var(--accent); border-radius: 50%; animation: spin 0.7s linear infinite; display: inline-block;
}
@keyframes spin { to { transform: rotate(360deg); } }

.ctr-scroll { flex: 1; min-height: 0; overflow-y: auto; }
.ctr-pods { padding: 8px 8px 0; display: flex; flex-direction: column; gap: 6px; }
.ctr-pods-title { font-size: 10px; font-weight: 700; letter-spacing: 1px; color: var(--fg-muted); text-transform: uppercase; padding: 0 4px; }
.ctr-pod-card {
  border: 1px solid var(--border); border-radius: 6px; padding: 7px 9px;
  display: flex; flex-direction: column; gap: 6px; background: var(--bg-mid);
}
.ctr-pod-head { display: flex; align-items: center; gap: 6px; }
.ctr-pod-name { font-size: 12px; font-weight: 600; color: var(--fg); flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.ctr-pod-rows { display: flex; flex-direction: column; border-top: 1px solid var(--border); margin-top: 2px; padding-top: 4px; }
.ctr-pod-empty { font-size: 10.5px; color: var(--fg-muted); font-style: italic; padding-left: 12px; }

.ctr-list { padding: 6px 0; }
.ctr-standalone-title { margin: 2px 8px 6px; }
.ctr-empty {
  display: flex; flex-direction: column; align-items: center; gap: 10px;
  padding: 32px 16px; color: var(--fg-muted); font-size: 12px;
}

.ctr-dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; background: var(--fg-muted); opacity: 0.4; }
.ctr-dot.on { background: var(--green); opacity: 1; box-shadow: 0 0 5px rgba(166,227,161,0.7); }

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
