<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, computed } from 'vue'
import FileExplorer from './components/FileExplorer.vue'
import EditorArea from './components/EditorArea.vue'
import StatusBar from './components/StatusBar.vue'
import Terminal from './components/Terminal.vue'
import AiPanel from './components/AiPanel.vue'
import DatabasePanel from './components/DatabasePanel.vue'
import FtpPanel from './components/FtpPanel.vue'
import RedisPanel from './components/RedisPanel.vue'
import GitPanel from './components/GitPanel.vue'
import ContainerPanel from './components/ContainerPanel.vue'
import SearchPanel from './components/SearchPanel.vue'
import ApiCollections from './components/ApiCollections.vue'
import TitleBar from './components/TitleBar.vue'
import PreferencesModal from './components/PreferencesModal.vue'
import ShortcutsModal from './components/ShortcutsModal.vue'
import GlobalContextMenu from './components/GlobalContextMenu.vue'
import UpdateBanner from './components/UpdateBanner.vue'
import { openUrl } from '@tauri-apps/plugin-opener'
import { getVersion } from '@tauri-apps/api/app'
import { useEditorStore } from './composables/useEditorStore'
import { useI18n } from './composables/useI18n'
import { AI_CLIS, cliMeta } from './composables/aiClis'
import { useAppTheme } from './composables/useAppTheme'


const store = useEditorStore()
const { t } = useI18n()
useAppTheme() // applies the saved theme attribute before first paint

const sidebarWidth = ref(240)
const isSideResizing = ref(false)
const showSidebar = ref(true)
const sidebarView = ref<'explorer' | 'search' | 'ai' | 'database' | 'ftp' | 'redis' | 'api' | 'git' | 'containers'>('explorer')

// Settings / About / Preferences
const showAbout = ref(false)
const showPreferences = ref(false)
const showShortcuts = ref(false)
// Read straight from the compiled app's own version (tauri.conf.json) so the
// About dialog can never drift out of sync with what people actually download.
const appVersion = ref('')
getVersion().then(v => { appVersion.value = v }).catch(() => {})
const showSettingsMenu = ref(false)
const showCliMenu = ref(false)
const cliMenuBtnEl = ref<HTMLElement | null>(null)
const cliMenuPos = ref({ top: 0, right: 0 })

function openCli(id: string) {
  store.openCliTab(id, cliMeta(id).label)
}

function toggleCliMenu() {
  if (!showCliMenu.value && cliMenuBtnEl.value) {
    const r = cliMenuBtnEl.value.getBoundingClientRect()
    // Anchor to the LEFT of the button — the activity bar sits flush against
    // the window's right edge, so positioning to the right pushed the menu
    // off-screen entirely.
    cliMenuPos.value = { top: r.top, right: window.innerWidth - r.left + 6 }
  }
  showCliMenu.value = !showCliMenu.value
}

const bottomHeight = ref(240)
const isBottomResizing = ref(false)
const showPanel = ref(false)

// Multi-terminal management
interface TermTab { id: string; name: string }
const terminals = ref<TermTab[]>([{ id: 'term-1', name: 'bash' }])
const activeTermId = ref('term-1')
const termRefs: Record<string, InstanceType<typeof Terminal> | null> = {}
function setTermRef(id: string, el: any) { termRefs[id] = el }
const activeTermRef = () => termRefs[activeTermId.value] ?? null
const renamingTermId = ref<string | null>(null)
const renamingTermName = ref('')

function addTerminal() {
  const id = `term-${Date.now()}`
  terminals.value.push({ id, name: 'bash' })
  activeTermId.value = id
  if (showPanel.value) setTimeout(() => { termRefs[id]?.fit(); termRefs[id]?.focus() }, 120)
}

function closeTerminal(id: string) {
  if (terminals.value.length === 1) return
  const idx = terminals.value.findIndex(t => t.id === id)
  terminals.value.splice(idx, 1)
  if (activeTermId.value === id) {
    activeTermId.value = terminals.value[Math.max(0, idx - 1)].id
    if (showPanel.value) setTimeout(() => { activeTermRef()?.fit(); activeTermRef()?.focus() }, 60)
  }
}

function startRenameTerminal(t: TermTab) {
  renamingTermId.value = t.id
  renamingTermName.value = t.name
}

function switchTerminal(id: string) {
  activeTermId.value = id
  if (showPanel.value) setTimeout(() => { termRefs[id]?.fit(); termRefs[id]?.focus() }, 30)
}

function cycleTerminal(dir: 1 | -1) {
  if (!showPanel.value || terminals.value.length < 2) return
  const idx = terminals.value.findIndex(t => t.id === activeTermId.value)
  const next = (idx + dir + terminals.value.length) % terminals.value.length
  switchTerminal(terminals.value[next].id)
}

function confirmRenameTerminal() {
  const t = terminals.value.find(t => t.id === renamingTermId.value)
  if (t && renamingTermName.value.trim()) t.name = renamingTermName.value.trim()
  renamingTermId.value = null
}
const apiCollRef = ref<{ saveRequest: (req: any) => void } | null>(null)

function onSaveApiRequest(req: any) {
  apiCollRef.value?.saveRequest(req)
}

onMounted(async () => {
  const saved = localStorage.getItem('last_project_path')
  if (saved) store.setRootPath(saved)
  window.addEventListener('keydown', onGlobalKeydown)
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onGlobalKeydown)
})

function isCodeMirrorTarget(e: KeyboardEvent): boolean {
  return !!(e.target as HTMLElement | null)?.closest('.cm-editor')
}

// New Ctrl+Shift+<letter> panel-jump shortcuts below are global (fire
// regardless of focus), so this keeps them from hijacking a real text-editing
// keystroke (e.g. Ctrl+Shift+V for paste-as-plain-text) while the user is
// typing in any rename/create/search input across the app.
function isTextInputTarget(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement | null
  return !!t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)
}

function focusSidebarPanel(view: 'explorer' | 'search' | 'ai' | 'database' | 'ftp' | 'redis' | 'api' | 'git' | 'containers') {
  if (sidebarView.value === view) { showSidebar.value = !showSidebar.value }
  else { sidebarView.value = view; showSidebar.value = true }
}

function onGlobalKeydown(e: KeyboardEvent) {
  const ctrl = e.ctrlKey || e.metaKey

  // Ctrl+Tab / Ctrl+Shift+Tab — cycle tabs
  if (ctrl && e.key === 'Tab') {
    e.preventDefault()
    const tabs = store.state.tabs
    if (tabs.length < 2) return
    const idx = tabs.findIndex(t => t.path === store.state.activeTabPath)
    const next = e.shiftKey
      ? (idx - 1 + tabs.length) % tabs.length
      : (idx + 1) % tabs.length
    store.setActive(tabs[next].path)
    return
  }

  // Ctrl+W — close active tab
  if (ctrl && !e.shiftKey && e.key === 'w') {
    e.preventDefault()
    if (store.state.activeTabPath) store.closeTab(store.state.activeTabPath)
    return
  }

  // Ctrl+B — toggle sidebar
  if (ctrl && !e.shiftKey && e.key === 'b') {
    e.preventDefault()
    showSidebar.value = !showSidebar.value
    return
  }

  // Ctrl+Shift+F — open search panel in sidebar
  if (ctrl && e.shiftKey && (e.key === 'f' || e.key === 'F')) {
    e.preventDefault()
    sidebarView.value = 'search'
    showSidebar.value = true
    return
  }

  // Ctrl+F — toggle editor find panel (handled globally so it works even when
  // focus is elsewhere, e.g. a sidebar input). Skipped when focus is already
  // inside CodeMirror: its own Mod-f keymap opens the panel directly there,
  // and letting both fire in sequence opened-then-immediately-reclosed it.
  if (ctrl && !e.shiftKey && e.key === 'f' && !isCodeMirrorTarget(e)) {
    e.preventDefault()
    window.dispatchEvent(new CustomEvent('editor-toggle-search'))
    return
  }

  // Escape — close modals first, then editor search panel
  if (e.key === 'Escape') {
    if (showPreferences.value) { showPreferences.value = false; return }
    if (showShortcuts.value)   { showShortcuts.value = false; return }
    if (showAbout.value)       { showAbout.value = false; return }
    if (showSettingsMenu.value){ showSettingsMenu.value = false; return }
    window.dispatchEvent(new CustomEvent('editor-escape'))
  }

  // Ctrl+` — toggle terminal. Checked via e.code (physical key), not e.key:
  // holding Shift changes what character that key types on most layouts
  // (backtick → tilde on US, and other symbols entirely on ES/LatAm layouts),
  // so e.key === '`' silently stopped matching as soon as Shift was added.
  if (ctrl && !e.shiftKey && e.code === 'Backquote') {
    e.preventDefault()
    togglePanel()
    return
  }

  // Ctrl+Shift+` — new terminal tab (opens the panel too if it was closed),
  // same convention VSCode uses for "Create New Terminal"
  if (ctrl && e.shiftKey && e.code === 'Backquote') {
    e.preventDefault()
    if (!showPanel.value) showPanel.value = true
    addTerminal()
    return
  }

  // Ctrl+PageDown / Ctrl+PageUp — cycle terminal tabs (only meaningful while
  // the panel is open with more than one tab; harmless no-op otherwise)
  if (ctrl && (e.key === 'PageDown' || e.key === 'PageUp')) {
    e.preventDefault()
    cycleTerminal(e.key === 'PageDown' ? 1 : -1)
    return
  }

  // Ctrl+Shift+A — open AI assistant panel
  if (ctrl && e.shiftKey && (e.key === 'a' || e.key === 'A')) {
    e.preventDefault()
    sidebarView.value = 'ai'
    showSidebar.value = true
    return
  }

  // Ctrl+Shift+C — open Claude Code CLI as workspace tab
  if (ctrl && e.shiftKey && (e.key === 'c' || e.key === 'C')) {
    e.preventDefault()
    openCli('claude')
    return
  }

  // Ctrl+Shift+<letter> — jump straight to a sidebar panel or open a fresh
  // workspace tab, mirroring the existing Search/AI/Claude shortcuts above.
  // Picked to avoid both this app's own combos (CodeMirror's Mod-Shift-K
  // "delete line", Mod-Shift-M "lint panel", Mod-Shift-L "select matches")
  // and common native text-editing ones (Ctrl+Shift+V "paste as plain text").
  if (ctrl && e.shiftKey && !isTextInputTarget(e)) {
    const k = e.key.toLowerCase()
    const panelMap: Record<string, typeof sidebarView.value> = {
      e: 'explorer', d: 'database', t: 'ftp', r: 'redis', g: 'git', n: 'containers', q: 'api',
    }
    if (k in panelMap) { e.preventDefault(); focusSidebarPanel(panelMap[k]); return }
    if (k === 'x') { e.preventDefault(); store.openDesignTab(); return }
    if (k === 'b') { e.preventDefault(); store.openBrowserTab(); return }
  }

  // Ctrl+S — save active file. Skipped when a CodeMirror instance has focus:
  // both the main editor and the SQL query box already bind Mod-s themselves
  // (the query box to a different action — "save query" — not saveActiveFile),
  // so letting this global handler also fire double-invoked/misfired on top.
  if (ctrl && !e.shiftKey && e.key === 's' && !isCodeMirrorTarget(e)) {
    e.preventDefault()
    store.saveActiveFile()
    return
  }
}

function onMouseMove(e: MouseEvent) {
  if (isSideResizing.value) {
    const fromRight = window.innerWidth - e.clientX
    sidebarWidth.value = Math.max(160, Math.min(520, fromRight))
  }
  if (isBottomResizing.value) {
    const fromBottom = window.innerHeight - e.clientY
    bottomHeight.value = Math.max(80, Math.min(600, fromBottom))
  }
}

function onMouseUp() {
  isSideResizing.value = false
  isBottomResizing.value = false
}

// Run current file via terminal
const RUN_MAP: Record<string, string> = {
  python:     'python3 "{file}"',
  javascript: 'node "{file}"',
  typescript: 'ts-node "{file}"',
  go:         'go run "{file}"',
  php:        'php "{file}"',
  rust:       'cargo run',
  cpp:        'g++ "{file}" -o /tmp/_ide_out && /tmp/_ide_out',
  sql:        'mysql < "{file}"',
}

const activeTab = computed(() => store.activeTab())
const canRun = computed(() => {
  const tab = activeTab.value
  return tab?.type === 'code' && tab.language in RUN_MAP
})

async function runFile() {
  const tab = activeTab.value
  if (!tab || !canRun.value) return
  await store.saveActiveFile()
  openPanel()
  await new Promise(r => setTimeout(r, 120))
  const cmd = RUN_MAP[tab.language].replace('{file}', tab.path)
  activeTermRef()?.runCommand(cmd)
  activeTermRef()?.focus()
}

function openPanel() {
  showPanel.value = true
  setTimeout(() => { activeTermRef()?.fit(); activeTermRef()?.focus() }, 60)
}

function onAiRunTerminal(cmd: string) {
  openPanel()
  setTimeout(() => { activeTermRef()?.runCommand(cmd); activeTermRef()?.focus() }, 150)
}

function togglePanel() {
  showPanel.value = !showPanel.value
  if (showPanel.value) setTimeout(() => { activeTermRef()?.fit(); activeTermRef()?.focus() }, 60)
}

function onTerminalOpenFile(path: string) {
  store.openFile(path)
}

function onDbOpenTable(connId: string, tableName: string, connName: string, driver: string) {
  store.openDbTable(connId, tableName, connName, driver)
}
</script>

<template>
  <GlobalContextMenu />
  <TitleBar />
  <div
    class="ide-root"
    @mousemove="onMouseMove"
    @mouseup="onMouseUp"
    @mouseleave="onMouseUp"
  >
    <!-- ── Center column ── -->
    <div class="center-col">

      <!-- Toolbar: solo visible cuando hay un archivo ejecutable -->
      <div v-if="canRun" class="run-bar">
        <button class="run-btn" @click="runFile" :title="`Run ${activeTab?.name}`">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
            <path d="M11.596 8.697l-6.363 3.692c-.54.313-1.233-.066-1.233-.697V4.308c0-.63.692-1.01 1.233-.696l6.363 3.692a.802.802 0 010 1.393z"/>
          </svg>
          {{ t('run') }} {{ activeTab?.name }}
        </button>
        <div class="run-bar-spacer" />
      </div>

      <!-- Editor area -->
      <div class="editor-wrap">
        <EditorArea @save-api-request="onSaveApiRequest" />
      </div>

      <!-- Bottom resize handle -->
      <div v-show="showPanel" class="bottom-resize" @mousedown.prevent="isBottomResizing = true" />

      <!-- Bottom panel — v-show keeps PTY sessions alive when hidden -->
      <div v-show="showPanel" class="bottom-panel" :style="{ height: bottomHeight + 'px' }">
        <!-- Panel tab bar -->
        <div class="bottom-panel-tabs">
          <svg class="term-icon" width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
            <path d="M0 3a2 2 0 012-2h12a2 2 0 012 2v10a2 2 0 01-2 2H2a2 2 0 01-2-2V3zm9.5 5.5h-3a.5.5 0 000 1h3a.5.5 0 000-1zm-6.354-.354a.5.5 0 000 .708l1.5 1.5a.5.5 0 00.708-.708L3.707 9l1.647-1.646a.5.5 0 10-.708-.708l-2 2z"/>
          </svg>
          <!-- Terminal tabs -->
          <button
            v-for="t in terminals" :key="t.id"
            class="panel-tab"
            :class="{ active: activeTermId === t.id }"
            @click="switchTerminal(t.id)"
            @dblclick.stop="startRenameTerminal(t)"
          >
            <template v-if="renamingTermId === t.id">
              <input
                v-model="renamingTermName"
                class="term-name-input"
                @keydown.enter="confirmRenameTerminal"
                @keydown.esc="renamingTermId = null"
                @blur="confirmRenameTerminal"
                @click.stop
                autofocus
              />
            </template>
            <span v-else class="term-tab-label">{{ t.name }}</span>
            <button v-if="terminals.length > 1" class="term-tab-close" @click.stop="closeTerminal(t.id)">×</button>
          </button>
          <!-- Add terminal button -->
          <button class="term-add-btn" title="New Terminal" @click="addTerminal">
            <svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor">
              <path d="M8 4a.5.5 0 01.5.5v3h3a.5.5 0 010 1h-3v3a.5.5 0 01-1 0v-3h-3a.5.5 0 010-1h3v-3A.5.5 0 018 4z"/>
            </svg>
          </button>
          <div class="panel-tab-spacer" />
          <button class="panel-close-btn" @click="showPanel = false">×</button>
        </div>

        <!-- Panel content: all terminals mounted, only active visible -->
        <div class="panel-content">
          <Terminal
            v-for="t in terminals"
            :key="t.id"
            v-show="activeTermId === t.id"
            :ref="(el) => setTermRef(t.id, el)"
            :cwd="store.state.rootPath || '/'"
            @open-file="onTerminalOpenFile"
          />
        </div>
      </div>
    </div>

    <!-- Sidebar resize handle -->
    <div v-if="showSidebar" class="resize-handle" @mousedown.prevent="isSideResizing = true" />

    <!-- Right sidebar -->
    <aside v-if="showSidebar" class="sidebar" :style="{ width: sidebarWidth + 'px' }">
      <FileExplorer v-if="sidebarView === 'explorer'" />
      <SearchPanel v-else-if="sidebarView === 'search'" />
      <AiPanel v-else-if="sidebarView === 'ai'" @run-terminal="onAiRunTerminal" />
      <DatabasePanel v-else-if="sidebarView === 'database'" @open-table="onDbOpenTable" />
      <FtpPanel v-else-if="sidebarView === 'ftp'" />
      <RedisPanel v-else-if="sidebarView === 'redis'" />
      <GitPanel v-else-if="sidebarView === 'git'" />
      <ContainerPanel v-else-if="sidebarView === 'containers'" />
      <ApiCollections v-else-if="sidebarView === 'api'" ref="apiCollRef" />
    </aside>

    <!-- Activity bar -->
    <div class="activity-bar">
      <button
        class="activity-btn"
        :class="{ active: showSidebar && sidebarView === 'explorer' }"
        :title="t('explorer') + ' (Ctrl+B)'"
        @click="sidebarView === 'explorer' ? showSidebar = !showSidebar : (sidebarView = 'explorer', showSidebar = true)"
      >
        <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M3 7a2 2 0 012-2h5l2 2h7a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V7z"/>
        </svg>
      </button>
      <button
        class="activity-btn"
        :class="{ active: showSidebar && sidebarView === 'search' }"
        :title="t('search') + ' (Ctrl+Shift+F)'"
        @click="sidebarView = 'search'; showSidebar = true"
      >
        <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <circle cx="11" cy="11" r="8"/><path d="M21 21l-4.35-4.35"/>
        </svg>
      </button>
      <button
        class="activity-btn"
        :class="{ active: showSidebar && sidebarView === 'git' }"
        title="Source Control"
        @click="sidebarView === 'git' ? showSidebar = !showSidebar : (sidebarView = 'git', showSidebar = true)"
      >
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/>
          <path d="M6 15V9a6 6 0 006 6h3"/>
        </svg>
      </button>
      <button
        class="activity-btn"
        :class="{ active: showSidebar && sidebarView === 'database' }"
        :title="t('database')"
        @click="sidebarView === 'database' ? showSidebar = !showSidebar : (sidebarView = 'database', showSidebar = true)"
      >
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <ellipse cx="12" cy="5" rx="9" ry="3"/>
          <path d="M3 5v4c0 1.66 4.03 3 9 3s9-1.34 9-3V5"/>
          <path d="M3 9v4c0 1.66 4.03 3 9 3s9-1.34 9-3V9"/>
          <path d="M3 13v4c0 1.66 4.03 3 9 3s9-1.34 9-3v-4"/>
        </svg>
      </button>
      <button
        class="activity-btn"
        :class="{ active: showSidebar && sidebarView === 'ftp' }"
        :title="t('ftp')"
        @click="sidebarView === 'ftp' ? showSidebar = !showSidebar : (sidebarView = 'ftp', showSidebar = true)"
      >
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M8 7h11M8 7l4-4M8 7l4 4M16 17H5M16 17l-4-4m4 4l-4 4" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </button>
      <button
        class="activity-btn"
        :class="{ active: showSidebar && sidebarView === 'redis' }"
        :title="t('redis')"
        @click="sidebarView === 'redis' ? showSidebar = !showSidebar : (sidebarView = 'redis', showSidebar = true)"
      >
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4"/>
        </svg>
      </button>
      <button
        class="activity-btn"
        :class="{ active: showSidebar && sidebarView === 'containers' }"
        :title="t('containers')"
        @click="sidebarView === 'containers' ? showSidebar = !showSidebar : (sidebarView = 'containers', showSidebar = true)"
      >
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <rect x="2" y="7" width="20" height="13" rx="2"/><path d="M6 7V4a2 2 0 012-2h8a2 2 0 012 2v3"/><path d="M2 11h20"/>
        </svg>
      </button>
      <button
        class="activity-btn"
        :class="{ active: store.activeTab()?.type === 'browser' }"
        :title="t('browserPreviewTitle')"
        @click="store.openBrowserTab()"
      >
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <circle cx="12" cy="12" r="10"/><path d="M2 12h20M12 2a15 15 0 010 20 15 15 0 010-20z"/>
        </svg>
      </button>
      <button
        class="activity-btn"
        :class="{ active: store.activeTab()?.type === 'design' }"
        :title="t('designTitle')"
        @click="store.openDesignTab()"
      >
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M12 2a10 10 0 100 20 3 3 0 003-3 2 2 0 012-2h1a3 3 0 003-3 10 10 0 00-9-12z"/>
          <circle cx="7.5" cy="10.5" r="1.2" fill="currentColor" stroke="none"/>
          <circle cx="12" cy="7" r="1.2" fill="currentColor" stroke="none"/>
          <circle cx="16.5" cy="10.5" r="1.2" fill="currentColor" stroke="none"/>
          <circle cx="9" cy="15" r="1.2" fill="currentColor" stroke="none"/>
        </svg>
      </button>
      <button
        class="activity-btn"
        :class="{ active: showSidebar && sidebarView === 'api' }"
        :title="t('api')"
        @click="sidebarView === 'api' ? showSidebar = !showSidebar : (sidebarView = 'api', showSidebar = true)"
      >
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M13 10V3L4 14h7v7l9-11h-7z"/>
        </svg>
      </button>
      <button
        class="activity-btn"
        :class="{ active: showPanel }"
        :title="t('terminal') + ' (Ctrl+`)'"
        @click="togglePanel()"
      >
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <rect x="2" y="3" width="20" height="18" rx="2"/>
          <path d="M8 10l3 3-3 3M13 16h3"/>
        </svg>
      </button>
      <!-- AI — one grouped entry point: chat assistant + every AI CLI -->
      <button
        ref="cliMenuBtnEl"
        class="activity-btn activity-btn--ai-group"
        :class="{ active: (showSidebar && sidebarView === 'ai') || store.state.activeTabPath === 'agent://main' || AI_CLIS.some(c => store.state.activeTabPath === `cli://${c.id}`) }"
        :title="t('ai') + ' (Ctrl+Shift+A)'"
        @click.stop="toggleCliMenu"
        style="position:relative"
      >
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M12 2a10 10 0 100 20A10 10 0 0012 2zm0 6v4m0 4h.01"/>
        </svg>
      </button>
      <div class="activity-spacer" />
      <button class="activity-btn" title="Settings" @click.stop="showSettingsMenu = !showSettingsMenu" style="position:relative">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <circle cx="12" cy="12" r="3"/>
          <path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 010 2.83 2 2 0 01-2.83 0l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06A1.65 1.65 0 0019.4 9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z"/>
        </svg>
      </button>
    </div>
  </div>

  <!-- AI menu: chat assistant + CLI picker, grouped in one place -->
  <Teleport to="body">
    <div v-if="showCliMenu" class="settings-overlay" @click="showCliMenu = false">
      <div class="settings-popup cli-menu-popup" :style="{ top: cliMenuPos.top + 'px', right: cliMenuPos.right + 'px' }" @click.stop>
        <button
          class="settings-popup-item"
          @click="sidebarView = 'ai'; showSidebar = true; showCliMenu = false"
        >
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 2a10 10 0 100 20A10 10 0 0012 2zm0 6v4m0 4h.01"/></svg>
          {{ t('ai') }}
        </button>
        <button
          class="settings-popup-item"
          @click="store.openAgentTab(); showCliMenu = false"
        >
          <span style="color:#a6e3a1;font-weight:900;font-size:13px">⬡</span>
          Agente (Ollama)
        </button>
        <div class="settings-popup-sep" />
        <div class="settings-popup-header">CLI</div>
        <button
          v-for="c in AI_CLIS" :key="c.id"
          class="settings-popup-item"
          @click="openCli(c.id); showCliMenu = false"
        >
          <span :style="{ color: c.color, fontWeight: 900, fontSize: '13px' }">{{ c.icon }}</span>
          {{ c.label }}
        </button>
      </div>
    </div>
  </Teleport>

  <!-- Settings menu popup -->
  <Teleport to="body">
    <div v-if="showSettingsMenu" class="settings-overlay" @click="showSettingsMenu = false">
      <div class="settings-popup" @click.stop>
        <div class="settings-popup-header">OweeCode</div>
        <button class="settings-popup-item" @click="showAbout = true; showSettingsMenu = false">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M8 15A7 7 0 118 1a7 7 0 010 14zm0 1A8 8 0 108 0a8 8 0 000 16z"/><path d="M5.255 5.786a.237.237 0 00.241.247h.825c.138 0 .248-.113.266-.25.09-.656.54-1.134 1.342-1.134.686 0 1.314.343 1.314 1.168 0 .635-.374.927-.965 1.371-.673.489-1.206 1.06-1.168 1.987l.003.217a.25.25 0 00.25.246h.811a.25.25 0 00.25-.25v-.105c0-.718.273-.927 1.01-1.486.609-.463 1.244-.977 1.244-2.056 0-1.511-1.276-2.241-2.673-2.241-1.267 0-2.655.59-2.75 2.286zm1.557 5.763c0 .533.425.927 1.01.927.609 0 1.028-.394 1.028-.927 0-.552-.42-.94-1.029-.94-.584 0-1.009.388-1.009.94z"/></svg>
          {{ t('settingsAbout') }}
        </button>
        <div class="settings-popup-sep" />
        <button class="settings-popup-item" @click="showPreferences = true; showSettingsMenu = false">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M8 4.754a3.246 3.246 0 100 6.492 3.246 3.246 0 000-6.492zM5.754 8a2.246 2.246 0 114.492 0 2.246 2.246 0 01-4.492 0z"/><path d="M9.796 1.343c-.527-1.79-3.065-1.79-3.592 0l-.094.319a.873.873 0 01-1.255.52l-.292-.16c-1.64-.892-3.433.902-2.54 2.541l.159.292a.873.873 0 01-.52 1.255l-.319.094c-1.79.527-1.79 3.065 0 3.592l.319.094a.873.873 0 01.52 1.255l-.16.292c-.892 1.64.901 3.434 2.541 2.54l.292-.159a.873.873 0 011.255.52l.094.319c.527 1.79 3.065 1.79 3.592 0l.094-.319a.873.873 0 011.255-.52l.292.16c1.64.893 3.434-.902 2.54-2.541l-.159-.292a.873.873 0 01.52-1.255l.319-.094c1.79-.527 1.79-3.065 0-3.592l-.319-.094a.873.873 0 01-.52-1.255l.16-.292c.893-1.64-.902-3.433-2.541-2.54l-.292.159a.873.873 0 01-1.255-.52l-.094-.319zm-2.633.283c.246-.835 1.428-.835 1.674 0l.094.319a1.873 1.873 0 002.693 1.115l.291-.16c.764-.415 1.6.42 1.184 1.185l-.159.292a1.873 1.873 0 001.116 2.692l.318.094c.835.246.835 1.428 0 1.674l-.319.094a1.873 1.873 0 00-1.115 2.693l.16.291c.415.764-.42 1.6-1.185 1.184l-.291-.159a1.873 1.873 0 00-2.693 1.116l-.094.318c-.246.835-1.428.835-1.674 0l-.094-.319a1.873 1.873 0 00-2.692-1.115l-.292.16c-.764.415-1.6-.42-1.184-1.185l.159-.291A1.873 1.873 0 001.945 8.93l-.319-.094c-.835-.246-.835-1.428 0-1.674l.319-.094A1.873 1.873 0 003.06 4.377l-.16-.292c-.415-.764.42-1.6 1.185-1.184l.292.159a1.873 1.873 0 002.692-1.115l.094-.319z"/></svg>
          {{ t('settingsPrefs') }}
        </button>
        <button class="settings-popup-item" @click="showShortcuts = true; showSettingsMenu = false">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M14 5a1 1 0 011 1v5a1 1 0 01-1 1H2a1 1 0 01-1-1V6a1 1 0 011-1h12zM2 4a2 2 0 00-2 2v5a2 2 0 002 2h12a2 2 0 002-2V6a2 2 0 00-2-2H2z"/><path d="M13 10.25a.25.25 0 01.25-.25h.5a.25.25 0 01.25.25v.5a.25.25 0 01-.25.25h-.5a.25.25 0 01-.25-.25v-.5zm0-2a.25.25 0 01.25-.25h.5a.25.25 0 01.25.25v.5a.25.25 0 01-.25.25h-.5a.25.25 0 01-.25-.25v-.5zm-5 0A.25.25 0 018.25 8h.5a.25.25 0 01.25.25v.5a.25.25 0 01-.25.25h-.5A.25.25 0 018 8.75v-.5zm2 0a.25.25 0 01.25-.25h.5a.25.25 0 01.25.25v.5a.25.25 0 01-.25.25h-.5a.25.25 0 01-.25-.25v-.5zm-6 0A.25.25 0 014.25 8h.5a.25.25 0 01.25.25v.5a.25.25 0 01-.25.25h-.5A.25.25 0 014 8.75v-.5zm2 0a.25.25 0 01.25-.25h.5a.25.25 0 01.25.25v.5a.25.25 0 01-.25.25h-.5a.25.25 0 01-.25-.25v-.5zm4 2a.25.25 0 01.25-.25h.5a.25.25 0 01.25.25v.5a.25.25 0 01-.25.25h-.5a.25.25 0 01-.25-.25v-.5zm-2 0a.25.25 0 01.25-.25h.5a.25.25 0 01.25.25v.5a.25.25 0 01-.25.25h-.5a.25.25 0 01-.25-.25v-.5zm-4 0a.25.25 0 01.25-.25h1.5a.25.25 0 01.25.25v.5a.25.25 0 01-.25.25h-1.5a.25.25 0 01-.25-.25v-.5z"/></svg>
          {{ t('settingsShortcuts') }}
        </button>
        <button class="settings-popup-item" @click="sidebarView = 'ai'; showSidebar = true; showSettingsMenu = false">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M8 1a7 7 0 100 14A7 7 0 008 1zm0 1a6 6 0 110 12A6 6 0 018 2zm0 2a1 1 0 100 2 1 1 0 000-2zm-.5 3h1v5h-1V7z"/></svg>
          {{ t('settingsAi') }}
        </button>
      </div>
    </div>

    <!-- About modal -->
    <div v-if="showAbout" class="about-overlay" @click.self="showAbout = false">
      <div class="about-modal">
        <button class="about-close" @click="showAbout = false">×</button>
        <div class="about-logo">
          <img src="/oweedev.png" width="72" height="72" style="border-radius:16px;display:block;margin:0 auto;" />
        </div>
        <h2 class="about-name">OweeCode</h2>
        <p class="about-version">{{ t('aboutVersion') }} {{ appVersion }}</p>
        <p class="about-desc" style="white-space:pre-line">{{ t('aboutDesc') }}</p>
        <div class="about-meta">
          <div class="about-row">
            <span class="about-label">{{ t('aboutAuthor') }}</span>
            <span class="about-value">Hector Martinez</span>
          </div>
          <div class="about-row">
            <span class="about-label">{{ t('aboutWebsite') }}</span>
            <a class="about-link" href="#" @click.prevent="openUrl('https://oweeme.com')">oweeme.com</a>
          </div>
          <div class="about-row">
            <span class="about-label">{{ t('aboutSupport') }}</span>
            <a class="about-link" href="#" @click.prevent="openUrl('mailto:hector@oweeme.com')">hector@oweeme.com</a>
          </div>
          <div class="about-row">
            <span class="about-label">{{ t('aboutRuntime') }}</span>
            <span class="about-value">Tauri 2 + Rust + Vue 3</span>
          </div>
          <div class="about-row">
            <span class="about-label">{{ t('aboutLicense') }}</span>
            <span class="about-value">MIT</span>
          </div>
        </div>

        <!-- Donation -->
        <a class="about-donate" href="#" @click.prevent="openUrl('https://www.paypal.com/paypalme/oweeandme')">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor" style="color:#009cde">
            <path d="M7.076 21.337H2.47a.641.641 0 01-.633-.74L4.944.901C5.026.382 5.474 0 5.998 0h7.46c2.57 0 4.578.543 5.69 1.81 1.01 1.15 1.304 2.42 1.012 4.287-.023.143-.047.288-.077.437-.983 5.05-4.349 6.797-8.647 6.797h-2.19c-.524 0-.968.382-1.05.9l-1.12 7.106zm14.146-14.42a3.35 3.35 0 00-.607-.541c-.013.076-.026.175-.041.254-.93 4.778-4.005 7.201-9.138 7.201h-2.19a.563.563 0 00-.556.479l-1.187 7.527h-.506l-.24 1.516a.56.56 0 00.554.647h3.882c.46 0 .85-.334.922-.788.06-.26.76-4.852.816-5.09a.932.932 0 01.923-.788h.58c3.76 0 6.705-1.528 7.565-5.946.36-1.847.174-3.388-.777-4.471z"/>
          </svg>
          {{ t('aboutDonate') }}
        </a>

        <div class="about-footer">
          <span>{{ t('aboutLogin') }}</span>
          <span class="about-soon">{{ t('aboutSoon') }}</span>
        </div>
      </div>
    </div>
    <!-- Preferences modal -->
    <PreferencesModal v-if="showPreferences" @close="showPreferences = false" />
    <!-- Shortcuts modal -->
    <ShortcutsModal v-if="showShortcuts" @close="showShortcuts = false" />
  </Teleport>

  <StatusBar />
  <UpdateBanner />
</template>

<style>
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

/* Native <select> elements otherwise render with OS-native chrome (light,
   regardless of theme) instead of the app's dark background — this strips
   that so the CSS-defined background/color actually apply. */
select {
  appearance: none;
  -webkit-appearance: none;
  -moz-appearance: none;
  background-image: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='10' height='6' viewBox='0 0 10 6'><path d='M1 1l4 4 4-4' stroke='%238a8a9a' stroke-width='1.4' fill='none' stroke-linecap='round' stroke-linejoin='round'/></svg>");
  background-repeat: no-repeat;
  background-position: right 7px center;
  padding-right: 20px !important;
}

:root {
  --bg-darkest:  #0d0d0f;
  --bg-dark:     #141416;
  --bg-mid:      #1a1b1e;
  --bg-panel:    #202023;
  --bg-hover:    #2a2b2f;
  --bg-active:   #37373d;
  --border:      #2d2d32;
  /* Koi palette accents */
  --accent:      #d0d0d8;
  --accent-fg:   #14151a;
  --accent2:     #d94f30;
  --koi-teal:    #1d6b5c;
  --koi-coral:   #d94f30;
  --koi-cream:   #f0e4a0;
  --koi-dark:    #2d2926;
  --fg-muted:    #5a5a6a;
  --fg-dim:      #8a8a9a;
  --fg:          #cdd6f4;
  --fg-bright:   #e8e9f0;
  --red:         #f38ba8;
  --green:       #a6e3a1;
  --yellow:      #f9e2af;
  --font-ui:     'Segoe UI', system-ui, -apple-system, sans-serif;
  --font-mono:   'JetBrains Mono', 'Fira Code', 'Cascadia Code', Consolas, monospace;
  --tab-height:  36px;
  --statusbar-h: 22px;
  --titlebar-h:  30px;
  --activity-w:  48px;
  --runbar-h:    34px;
}

/* Grey theme — lighter graphite dark palette */
:root[data-theme="grey"] {
  --bg-darkest:  #232428;
  --bg-dark:     #292a2e;
  --bg-mid:      #303136;
  --bg-panel:    #38393e;
  --bg-hover:    #434449;
  --bg-active:   #4e4f55;
  --border:      #46474c;
  --accent:      #c9c9d2;
  --accent-fg:   #1d1e22;
  --fg-muted:    #8c8c95;
  --fg-dim:      #b3b3bc;
  --fg:          #e6e6ec;
  --fg-bright:   #f7f7fa;
}

/* White theme — light, Mac-like palette */
:root[data-theme="white"] {
  --bg-darkest:  #f2f2f5;
  --bg-dark:     #ffffff;
  --bg-mid:      #f7f7f9;
  --bg-panel:    #ffffff;
  --bg-hover:    #ececef;
  --bg-active:   #dfdfe4;
  --border:      #dcdce1;
  --accent:      #55555f;
  --accent-fg:   #ffffff;
  --koi-cream:   #8a6d00;
  --fg-muted:    #9a9aa1;
  --fg-dim:      #5a5a62;
  --fg:          #242429;
  --fg-bright:   #08080a;
  --red:         #d1354f;
  --green:       #1f9d55;
  --yellow:      #a5730a;
}

html, body, #app { height: 100%; overflow: hidden; background: var(--bg-darkest); }
#app { display: flex; flex-direction: column; }
</style>

<style scoped>
.ide-root {
  display: flex;
  flex: 1;
  height: calc(100vh - var(--statusbar-h) - var(--titlebar-h));
  overflow: hidden;
}

/* Center */
.center-col { flex: 1; display: flex; flex-direction: column; overflow: hidden; min-width: 0; }

/* Toolbar */
.run-bar {
  height: var(--runbar-h);
  background: var(--bg-dark);
  border-bottom: 1px solid var(--border);
  display: flex; align-items: center;
  padding: 0 8px; gap: 6px; flex-shrink: 0;
}
.run-btn {
  display: flex; align-items: center; gap: 5px;
  background: rgba(166,227,161,0.12);
  border: 1px solid rgba(166,227,161,0.3);
  color: var(--green); border-radius: 5px;
  font-size: 12px; font-family: var(--font-ui); font-weight: 600;
  padding: 3px 10px; cursor: pointer; transition: background 0.12s;
}
.run-btn:hover { background: rgba(166,227,161,0.2); }
.run-bar-spacer { flex: 1; }
.panel-toggle-btn {
  display: flex; align-items: center; gap: 5px;
  background: none; border: 1px solid var(--border);
  color: var(--fg-muted); border-radius: 5px;
  font-size: 11px; font-family: var(--font-ui);
  padding: 3px 9px; cursor: pointer; transition: all 0.12s;
}
.panel-toggle-btn:hover { color: var(--fg); border-color: var(--fg-muted); }
.panel-toggle-btn.active { color: var(--accent); border-color: var(--accent); background: rgba(208,208,216,0.1); }

/* Editor */
.editor-wrap { flex: 1; overflow: hidden; min-height: 0; }

/* Bottom resize */
.bottom-resize {
  height: 4px; background: var(--border);
  cursor: row-resize; flex-shrink: 0; transition: background 0.15s;
}
.bottom-resize:hover { background: var(--accent); }

/* Bottom panel */
.bottom-panel {
  display: flex; flex-direction: column;
  background: var(--bg-darkest);
  border-top: 1px solid var(--border);
  flex-shrink: 0; min-height: 80px;
}
.bottom-panel-tabs {
  display: flex; align-items: stretch;
  background: var(--bg-dark);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0; height: 32px; overflow: hidden;
}
.term-icon {
  align-self: center; margin: 0 6px 0 10px; flex-shrink: 0;
  color: var(--fg-muted);
}
.panel-tab {
  display: flex; align-items: center; gap: 4px;
  padding: 0 10px; background: none; border: none;
  border-bottom: 2px solid transparent;
  color: var(--fg-muted); font-size: 11px; font-weight: 600;
  font-family: var(--font-ui); cursor: pointer;
  transition: color 0.12s, border-color 0.12s;
  white-space: nowrap; max-width: 160px;
}
.panel-tab:hover { color: var(--fg-dim); background: var(--bg-hover); }
.panel-tab.active { color: var(--fg); border-bottom-color: var(--accent); background: var(--bg-darkest); }
.term-tab-label { overflow: hidden; text-overflow: ellipsis; max-width: 100px; }
.term-tab-close {
  background: none; border: none; color: var(--fg-muted); font-size: 13px;
  padding: 0 2px; cursor: pointer; line-height: 1; opacity: 0;
  transition: opacity 0.1s; display: flex; align-items: center;
}
.panel-tab:hover .term-tab-close,
.panel-tab.active .term-tab-close { opacity: 1; }
.term-tab-close:hover { color: var(--fg); }
.term-name-input {
  background: var(--bg-mid); border: 1px solid var(--accent); border-radius: 3px;
  color: var(--fg); font-size: 11px; font-family: var(--font-ui);
  padding: 1px 5px; outline: none; width: 90px; height: 20px;
}
.term-add-btn {
  background: none; border: none; color: var(--fg-muted);
  padding: 0 8px; cursor: pointer; display: flex; align-items: center;
  border-bottom: 2px solid transparent; transition: color 0.1s;
}
.term-add-btn:hover { color: var(--accent); }
.panel-tab-spacer { flex: 1; }
.panel-close-btn {
  background: none; border: none; color: var(--fg-muted);
  font-size: 16px; padding: 0 12px; cursor: pointer;
  display: flex; align-items: center; transition: color 0.12s;
  border-bottom: 2px solid transparent;
}
.panel-close-btn:hover { color: var(--fg); }
.panel-content { flex: 1; overflow: hidden; position: relative; }

/* Sidebar resize */
.resize-handle {
  width: 3px; cursor: col-resize;
  background: var(--border); flex-shrink: 0; transition: background 0.15s;
}
.resize-handle:hover, .resize-handle:active { background: var(--accent); }

/* Sidebar */
.sidebar {
  background: var(--bg-dark);
  border-left: 1px solid var(--border);
  display: flex; flex-direction: column;
  flex-shrink: 0; min-width: 160px; max-width: 520px; overflow: hidden;
}

/* Activity bar */
.activity-bar {
  width: var(--activity-w);
  background: var(--bg-darkest);
  border-left: 1px solid var(--border);
  display: flex; flex-direction: column;
  align-items: center; padding-top: 8px;
  gap: 2px; flex-shrink: 0;
}
.activity-spacer { flex: 1; }
.activity-btn {
  width: 38px; height: 38px;
  background: none; border: none; border-radius: 8px;
  color: var(--fg-muted); cursor: pointer;
  display: flex; align-items: center; justify-content: center;
  transition: color 0.15s, background 0.15s; margin-bottom: 2px;
}
.activity-btn:hover { color: var(--fg-bright); background: var(--bg-hover); }
.activity-btn.active { color: var(--koi-cream); background: rgba(208,208,216,0.18); }

/* Grouped AI button — assistant + all CLIs live behind this one entry point */
.activity-btn--ai-group.active { color: #fbbf24; background: rgba(217,119,6,0.18); }

/* Settings popup */
.settings-overlay { position: fixed; inset: 0; z-index: 9999; }
.settings-popup {
  position: fixed; bottom: 50px; right: 52px;
  background: var(--bg-panel); border: 1px solid var(--border);
  border-radius: 10px; padding: 4px;
  min-width: 180px; box-shadow: 0 12px 40px rgba(0,0,0,0.6);
  z-index: 10000;
}
.settings-popup-header {
  font-size: 11px; font-weight: 700; color: var(--fg-muted);
  letter-spacing: 1px; padding: 6px 12px 4px;
}
.settings-popup-item {
  display: flex; align-items: center; gap: 8px;
  width: 100%; background: none; border: none;
  color: var(--fg); font-size: 12.5px; font-family: var(--font-ui);
  padding: 7px 12px; border-radius: 6px; cursor: pointer; text-align: left;
  transition: background 0.1s;
}
.settings-popup-item:hover { background: var(--bg-hover); }
.settings-popup-sep { height: 1px; background: var(--border); margin: 4px 0; }
.cli-menu-popup { bottom: auto; left: auto; max-height: 80vh; overflow-y: auto; }

/* About modal */
.about-overlay {
  position: fixed; inset: 0; z-index: 99999;
  background: rgba(0,0,0,0.65); backdrop-filter: blur(4px);
  display: flex; align-items: center; justify-content: center;
}
.about-modal {
  background: var(--bg-panel); border: 1px solid var(--border);
  border-radius: 16px; padding: 32px 28px 24px;
  width: 380px; position: relative;
  box-shadow: 0 24px 80px rgba(0,0,0,0.8);
  font-family: var(--font-ui);
}
.about-close {
  position: absolute; top: 12px; right: 14px;
  background: none; border: none; color: var(--fg-muted);
  font-size: 20px; cursor: pointer; width: 28px; height: 28px;
  border-radius: 6px; display: flex; align-items: center; justify-content: center;
  transition: all 0.12s;
}
.about-close:hover { background: var(--bg-hover); color: var(--fg); }
.about-logo { text-align: center; margin-bottom: 12px; }
.about-name { text-align: center; font-size: 22px; font-weight: 800; color: var(--fg-bright); }
.about-version { text-align: center; font-size: 11.5px; color: var(--accent); margin: 4px 0 10px; }
.about-desc { text-align: center; font-size: 12px; color: var(--fg-dim); line-height: 1.5; margin-bottom: 18px; }
.about-meta { background: var(--bg-mid); border-radius: 8px; padding: 10px 14px; margin-bottom: 14px; }
.about-row { display: flex; justify-content: space-between; align-items: center; padding: 5px 0; font-size: 12px; border-bottom: 1px solid var(--border); }
.about-row:last-child { border-bottom: none; }
.about-label { color: var(--fg-muted); }
.about-value { color: var(--fg); font-weight: 500; }
.about-link { color: var(--accent); text-decoration: none; }
.about-link:hover { text-decoration: underline; }
.about-footer {
  display: flex; align-items: center; justify-content: space-between;
  border: 1px solid var(--border); border-radius: 8px;
  padding: 8px 14px; font-size: 12px; color: var(--fg-dim);
  background: var(--bg-mid);
}
.about-soon { font-size: 11px; color: var(--accent); font-weight: 600; }
.about-donate {
  display: flex; align-items: center; justify-content: center; gap: 7px;
  margin: 10px 0 8px;
  background: linear-gradient(135deg, #003087 0%, #009cde 100%);
  border-radius: 10px; padding: 9px 16px;
  color: #fff; font-size: 12.5px; font-weight: 600;
  text-decoration: none; transition: opacity 0.15s;
}
.about-donate:hover { opacity: 0.88; }
</style>
