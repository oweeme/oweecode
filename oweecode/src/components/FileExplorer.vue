<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, provide, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { useEditorStore } from '../composables/useEditorStore'
import { useI18n } from '../composables/useI18n'
import TreeNode, { type TreeNodeData } from './TreeNode.vue'

const { t } = useI18n()

const store = useEditorStore()
const tree = ref<TreeNodeData[]>([])
const rootLabel = ref('')

// Keyboard navigation state (declared early — used in provide() below)
const focusedPath = ref<string | null>(null)
const explorerEl = ref<HTMLElement | null>(null)
const explorerFocused = ref(false)
const rootInput = ref('')
const showInput = ref(false)
const loading = ref(false)
const error = ref('')

// Context menu
const ctxMenu = ref<{ x: number; y: number; node: TreeNodeData } | null>(null)

// Track last right-clicked or selected folder
const selectedFolderPath = ref('')

// Inline creation
const creating = ref<{ parentPath: string; type: 'file' | 'folder'; name: string } | null>(null)

// Hidden files toggle
const showHidden = ref(localStorage.getItem('explorer_show_hidden') === 'true')

function toggleHidden() {
  showHidden.value = !showHidden.value
  localStorage.setItem('explorer_show_hidden', String(showHidden.value))
  if (store.state.rootPath) setRoot(store.state.rootPath)
}

async function loadDir(path: string): Promise<TreeNodeData[]> {
  try {
    const entries = await invoke<{ name: string; path: string; is_dir: boolean }[]>('list_dir', { path, showHidden: showHidden.value })
    return entries.map(e => ({
      ...e,
      children: e.is_dir ? [] : undefined,
      expanded: false,
      loaded: false,
    }))
  } catch (e) {
    error.value = String(e)
    return []
  }
}

async function setRoot(path: string) {
  error.value = ''
  loading.value = true
  rootLabel.value = path.split('/').filter(Boolean).pop() ?? path
  rootInput.value = path
  tree.value = await loadDir(path)
  loading.value = false
  // Persist so next launch restores this project, not home
  localStorage.setItem('last_project_path', path)
}

watch(() => store.state.rootPath, (p) => { if (p) setRoot(p) }, { immediate: true })

// Native folder picker
async function pickFolder() {
  const selected = await openDialog({ directory: true, multiple: false, title: 'Open Project Folder' })
  if (selected && typeof selected === 'string') {
    store.setRootPath(selected)
  }
}

// Tracks the currently active file path for TreeNode highlighting
const activeFilePath = ref<string | null>(null)

async function revealFile(filePath: string | null) {
  if (!filePath || !store.state.rootPath) return
  if (!filePath.startsWith(store.state.rootPath + '/')) return
  const relative = filePath.slice(store.state.rootPath.length + 1)
  const parts = relative.split('/')
  let currentNodes = tree.value
  let currentPath = store.state.rootPath
  for (let i = 0; i < parts.length - 1; i++) {
    currentPath = currentPath + '/' + parts[i]
    const node = currentNodes.find(n => n.path === currentPath && n.is_dir)
    if (!node) break
    if (!node.expanded) {
      node.expanded = true
      if (!node.loaded) { node.children = await loadDir(currentPath); node.loaded = true }
    }
    currentNodes = node.children ?? []
  }
  await nextTick()
  activeFilePath.value = filePath
}

// Watch active tab: expand parent folders and highlight in tree
watch(() => store.state.activeTabPath, async (filePath) => {
  if (!filePath || !store.state.rootPath) return
  // Ignore virtual paths (CLI, DB, FTP, API tabs) — they are not real files
  if (filePath.includes('://')) return
  if (!filePath.startsWith(store.state.rootPath + '/')) {
    // File is outside current project — auto-switch to its parent folder
    const parent = filePath.split('/').slice(0, -1).join('/')
    store.setRootPath(parent)
    return
  }
  await revealFile(filePath)
})

async function toggle(node: TreeNodeData) {
  if (!node.is_dir) {
    store.openFile(node.path)
    return
  }
  // Update selected folder when clicking a directory
  selectedFolderPath.value = node.path
  node.expanded = !node.expanded
  if (node.expanded && !node.loaded) {
    node.children = await loadDir(node.path)
    node.loaded = true
  }
}

async function changeRoot() {
  if (!rootInput.value.trim()) return
  showInput.value = false
  store.setRootPath(rootInput.value.trim())
}

function openCtxMenu(e: MouseEvent, node: TreeNodeData) {
  e.preventDefault()
  e.stopPropagation()
  ctxMenu.value = { x: e.clientX, y: e.clientY, node }
  selectedFolderPath.value = node.is_dir ? node.path : (node.path.split('/').slice(0, -1).join('/') || store.state.rootPath)
}

function closeCtx() { ctxMenu.value = null }

async function injectCreateNode(parentPath: string, type: 'file' | 'folder') {
  const pseudo: TreeNodeData = { path: '_creating_', name: '', is_dir: type === 'folder', isCreating: true }
  creating.value = { parentPath, type, name: '' }
  if (parentPath === store.state.rootPath) {
    tree.value.unshift(pseudo)
  } else {
    const node = findNode(tree.value, parentPath)
    if (node && node.is_dir) {
      if (!node.loaded) { node.children = await loadDir(parentPath); node.loaded = true }
      node.expanded = true
      node.children!.unshift(pseudo)
    }
  }
}

async function startCreate(type: 'file' | 'folder') {
  const parentPath = selectedFolderPath.value || store.state.rootPath
  closeCtx()
  await injectCreateNode(parentPath, type)
}

async function startCreateAtRoot(type: 'file' | 'folder') {
  const parentPath = selectedFolderPath.value || store.state.rootPath
  await injectCreateNode(parentPath, type)
}

// ── Smart refresh: reload only a specific folder node, preserving the rest ──
async function refreshNodeChildren(nodes: TreeNodeData[], parentPath: string): Promise<boolean> {
  for (const node of nodes) {
    if (node.path === parentPath && node.is_dir) {
      node.children = await loadDir(parentPath)
      node.loaded = true
      return true
    }
    if (node.is_dir && node.expanded && node.children) {
      if (await refreshNodeChildren(node.children, parentPath)) return true
    }
  }
  return false
}

async function refreshParent(parentPath: string) {
  // If parent is the root, refresh root entries (but keep expanded dirs unchanged)
  if (parentPath === store.state.rootPath) {
    const fresh = await loadDir(parentPath)
    // Merge: preserve expanded/loaded state for existing dirs
    const map = new Map(tree.value.map(n => [n.path, n]))
    tree.value = fresh.map(n => {
      const existing = map.get(n.path)
      return existing ? { ...n, expanded: existing.expanded, children: existing.children, loaded: existing.loaded } : n
    })
    return
  }
  // Otherwise find and refresh just that folder
  const found = await refreshNodeChildren(tree.value, parentPath)
  if (!found) {
    // Fallback: refresh root
    const fresh = await loadDir(store.state.rootPath)
    const map = new Map(tree.value.map(n => [n.path, n]))
    tree.value = fresh.map(n => {
      const existing = map.get(n.path)
      return existing ? { ...n, expanded: existing.expanded, children: existing.children, loaded: existing.loaded } : n
    })
  }
}


// Delete modal state
const deleteModal = ref<{ node: TreeNodeData; count?: number } | null>(null)

async function deleteNode() {
  if (!ctxMenu.value) return
  const node = ctxMenu.value.node
  closeCtx()
  // Show safe delete modal
  deleteModal.value = { node }
}

async function confirmDelete() {
  const node = deleteModal.value?.node
  if (!node) return
  deleteModal.value = null
  try {
    // Always move to trash — never permanent delete from UI
    await invoke('move_to_trash', { path: node.path })
    const parentPath = node.path.split('/').slice(0, -1).join('/') || store.state.rootPath
    await refreshParent(parentPath)
    store.closeTabByPath(node.path)
  } catch (e: any) {
    error.value = String(e)
  }
}

function cancelDelete() { deleteModal.value = null }

// ── Rename (inline in tree via provide/inject) ───────────────
const renaming = ref<{ node: TreeNodeData; name: string } | null>(null)

function startRename() {
  if (!ctxMenu.value) return
  const node = ctxMenu.value.node
  closeCtx()
  renaming.value = { node, name: node.name }
}

async function confirmRename() {
  if (!renaming.value) return
  const { node, name } = renaming.value
  const trimmed = name.trim()
  if (!trimmed || trimmed === node.name) { renaming.value = null; return }
  const parentPath = node.path.split('/').slice(0, -1).join('/') || store.state.rootPath
  const newPath = `${parentPath}/${trimmed}`
  try {
    await invoke('rename_entry', { oldPath: node.path, newPath })
    store.renameTab(node.path, newPath, trimmed)
    await refreshParent(parentPath)
  } catch (e) {
    error.value = String(e)
  }
  renaming.value = null
}

function cancelRename() { renaming.value = null }

// ── Inline create: inject pseudo-node into tree ──────────────
function findNode(nodes: TreeNodeData[], path: string): TreeNodeData | null {
  for (const n of nodes) {
    if (n.path === path) return n
    if (n.children) { const f = findNode(n.children, path); if (f) return f }
  }
  return null
}

function removePseudoNode() {
  const idx = tree.value.findIndex(n => n.path === '_creating_')
  if (idx !== -1) { tree.value.splice(idx, 1); return }
  function sweep(nodes: TreeNodeData[]) {
    for (const n of nodes) {
      if (n.children) {
        const ci = n.children.findIndex(c => c.path === '_creating_')
        if (ci !== -1) { n.children.splice(ci, 1); return }
        sweep(n.children)
      }
    }
  }
  sweep(tree.value)
}

async function confirmCreate(name: string) {
  if (!creating.value || !name.trim()) { cancelCreate(); return }
  const { parentPath, type } = creating.value
  const fullPath = `${parentPath}/${name.trim()}`
  removePseudoNode()
  creating.value = null
  try {
    if (type === 'file') await invoke('create_file', { path: fullPath })
    else await invoke('create_dir_cmd', { path: fullPath })
    await refreshParent(parentPath)
    if (type === 'file') store.openFile(fullPath)
  } catch (e) {
    error.value = String(e)
  }
}

function cancelCreate() { removePseudoNode(); creating.value = null }

// ── Drag & drop move ─────────────────────────────────────────
const draggedNode = ref<any>(null)

async function onDropNode(targetDir: any) {
  const src = draggedNode.value
  if (!src) return
  draggedNode.value = null
  const name = src.name
  const newPath = `${targetDir.path}/${name}`
  if (newPath === src.path) return
  const oldParent = src.path.split('/').slice(0, -1).join('/') || store.state.rootPath
  try {
    await invoke('rename_entry', { oldPath: src.path, newPath })
    if (src.path in (store.state as any)) store.renameTab?.(src.path, newPath, name)
    await refreshParent(oldParent)
    await refreshParent(targetDir.path)
  } catch (e: any) {
    error.value = String(e)
  }
}

// Provide to TreeNode descendants
provide('activeFilePath', activeFilePath)
provide('focusedPath', focusedPath)
provide('explorerRenaming', renaming)
provide('explorerConfirmRename', confirmRename)
provide('explorerCancelRename', cancelRename)
provide('explorerConfirmCreate', confirmCreate)
provide('explorerCancelCreate', cancelCreate)
provide('draggedNode', draggedNode)
provide('onDropNode', onDropNode)

// ── Open in file manager ──────────────────────────────────────
async function openInFileManager() {
  if (!ctxMenu.value) return
  const node = ctxMenu.value.node
  const dir = node.is_dir ? node.path : node.path.split('/').slice(0, -1).join('/')
  closeCtx()
  try {
    await invoke('open_in_file_manager', { path: dir })
  } catch (e) {
    error.value = String(e)
  }
}

// ── Keyboard navigation ──────────────────────────────────────

// Build flat list of all currently visible nodes (respects expanded state)
function flatVisible(nodes: TreeNodeData[]): TreeNodeData[] {
  const result: TreeNodeData[] = []
  for (const n of nodes) {
    if (n.path === '_creating_') continue
    result.push(n)
    if (n.is_dir && n.expanded && n.children) {
      result.push(...flatVisible(n.children))
    }
  }
  return result
}

function scrollToFocused() {
  nextTick(() => {
    const el = explorerEl.value?.querySelector(`[data-path="${CSS.escape(focusedPath.value ?? '')}"]`) as HTMLElement | null
    el?.scrollIntoView({ block: 'nearest' })
  })
}

async function onExplorerKeydown(e: KeyboardEvent) {
  // Only handle when explorer has focus and no modal/input is open
  if (!explorerFocused.value) return
  if (creating.value || renaming.value || showInput.value) return

  const flat = flatVisible(tree.value)
  if (flat.length === 0) return

  const currentIdx = focusedPath.value ? flat.findIndex(n => n.path === focusedPath.value) : -1

  if (e.key === 'ArrowDown') {
    e.preventDefault()
    const next = currentIdx < flat.length - 1 ? currentIdx + 1 : 0
    focusedPath.value = flat[next].path
    scrollToFocused()
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    const prev = currentIdx > 0 ? currentIdx - 1 : flat.length - 1
    focusedPath.value = flat[prev].path
    scrollToFocused()
  } else if (e.key === 'ArrowRight') {
    e.preventDefault()
    const node = flat[currentIdx]
    if (!node) return
    if (node.is_dir && !node.expanded) {
      node.expanded = true
      if (!node.loaded) { node.children = await loadDir(node.path); node.loaded = true }
    }
  } else if (e.key === 'ArrowLeft') {
    e.preventDefault()
    const node = flat[currentIdx]
    if (!node) return
    if (node.is_dir && node.expanded) {
      node.expanded = false
    } else {
      // Move focus to parent folder
      const parentPath = node.path.split('/').slice(0, -1).join('/')
      const parentNode = flat.find(n => n.path === parentPath)
      if (parentNode) focusedPath.value = parentNode.path
      scrollToFocused()
    }
  } else if (e.key === 'Enter') {
    e.preventDefault()
    const node = flat[currentIdx]
    if (!node) return
    if (node.is_dir) {
      node.expanded = !node.expanded
      if (node.expanded && !node.loaded) { node.children = await loadDir(node.path); node.loaded = true }
    } else {
      store.openFile(node.path)
    }
  } else if (e.key === 'Escape') {
    closeCtx(); cancelCreate(); cancelRename()
    deleteModal.value = null; showInput.value = false
  } else if (e.key === 'F2') {
    const node = flat[currentIdx]
    if (node) { ctxMenu.value = { x: 0, y: 0, node }; startRename() }
  }
}

// ── Global ESC to close panels ───────────────────────────────
function onGlobalKey(e: KeyboardEvent) {
  if (explorerFocused.value) return  // handled by onExplorerKeydown
  if (e.key === 'Escape') {
    closeCtx()
    cancelCreate()
    cancelRename()
    deleteModal.value = null
    showInput.value = false
  }
  if (e.key === 'F2' && ctxMenu.value) {
    startRename()
  }
}

onMounted(() => window.addEventListener('keydown', onGlobalKey))
onUnmounted(() => window.removeEventListener('keydown', onGlobalKey))

async function refreshTree() {
  const fresh = await loadDir(store.state.rootPath)
  const map = new Map(tree.value.map(n => [n.path, n]))
  tree.value = fresh.map(n => {
    const ex = map.get(n.path)
    return ex ? { ...n, expanded: ex.expanded, children: ex.children, loaded: ex.loaded } : n
  })
}
</script>

<template>
  <div
    ref="explorerEl"
    class="explorer"
    data-explorer
    tabindex="0"
    @click="closeCtx; explorerFocused = true"
    @focus="explorerFocused = true"
    @blur="explorerFocused = false"
    @keydown="onExplorerKeydown"
    @contextmenu.prevent
  >
    <div class="explorer-header">
      <span class="explorer-title">{{ t('explorer') }}</span>
      <div class="header-actions">
        <button class="hdr-btn" @click.stop="startCreateAtRoot('file')" :title="t('newFile')">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
            <path d="M9.5 1.143A.5.5 0 009.146 1H3.5A1.5 1.5 0 002 2.5v11A1.5 1.5 0 003.5 15h9a1.5 1.5 0 001.5-1.5V5.854a.5.5 0 00-.146-.354L9.5 1.143zM9 2.207L13.793 7H10a1 1 0 01-1-1V2.207z"/>
            <path d="M8 9.5a.5.5 0 011 0V11h1.5a.5.5 0 010 1H9v1.5a.5.5 0 01-1 0V12H6.5a.5.5 0 010-1H8V9.5z"/>
          </svg>
        </button>
        <button class="hdr-btn" @click.stop="startCreateAtRoot('folder')" :title="t('newFolder')">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
            <path d="M1.75 1A1.75 1.75 0 000 2.75v10.5C0 14.22.784 15 1.75 15h12.5A1.75 1.75 0 0016 13.25v-8.5A1.75 1.75 0 0014.25 3H7.5L6.34 1.5A1.75 1.75 0 004.99 1H1.75z"/>
            <path d="M8 6.5a.5.5 0 011 0V8h1.5a.5.5 0 010 1H9v1.5a.5.5 0 01-1 0V9H6.5a.5.5 0 010-1H8V6.5z" fill="#0f1012"/>
          </svg>
        </button>
        <button class="hdr-btn" @click.stop="refreshTree" :title="t('refresh')">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 3a5 5 0 104.546 2.914.5.5 0 01.908-.417A6 6 0 118 2v1z"/>
            <path d="M8 4.466V.534a.25.25 0 01.41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 018 4.466z"/>
          </svg>
        </button>
        <button class="hdr-btn" :class="{ 'hdr-btn--active': showHidden }" @click.stop="toggleHidden" :title="t('showHidden')">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
            <circle cx="12" cy="12" r="3"/>
          </svg>
        </button>
      </div>
    </div>

    <!-- Root folder row -->
    <div class="folder-root" :title="rootInput">
      <svg class="folder-icon" viewBox="0 0 16 16" fill="currentColor">
        <path d="M1.75 1A1.75 1.75 0 000 2.75v10.5C0 14.22.784 15 1.75 15h12.5A1.75 1.75 0 0016 13.25v-8.5A1.75 1.75 0 0014.25 3H7.5L6.34 1.5A1.75 1.75 0 004.99 1H1.75z"/>
      </svg>
      <span class="folder-name" :title="rootInput">{{ rootLabel || t('noFolderOpen') }}</span>
      <button class="open-folder-btn" @click.stop="pickFolder" :title="t('openFolder')">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
          <path d="M1 3.5A1.5 1.5 0 012.5 2h2.764c.958 0 1.76.56 2.311 1.184C7.985 3.648 8.48 4 9 4h4.5A1.5 1.5 0 0115 5.5v1h1v1h-1v5.5A1.5 1.5 0 0113.5 14h-11A1.5 1.5 0 011 12.5v-9z"/>
        </svg>
      </button>
    </div>

    <!-- Empty state with big open button -->
    <div v-if="!rootLabel" class="explorer-empty-state">
      <div class="empty-icon">
        <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1">
          <path d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V7z"/>
        </svg>
      </div>
      <p class="empty-label">{{ t('noFolderOpen') }}</p>
      <button class="open-folder-big" @click="pickFolder">
        <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
          <path d="M1 3.5A1.5 1.5 0 012.5 2h2.764c.958 0 1.76.56 2.311 1.184C7.985 3.648 8.48 4 9 4h4.5A1.5 1.5 0 0115 5.5v1h1v1h-1v5.5A1.5 1.5 0 0113.5 14h-11A1.5 1.5 0 011 12.5v-9z"/>
        </svg>
        {{ t('openFolder') }}
      </button>
      <p class="empty-hint">{{ t('pathManualHint') }}</p>
      <div class="path-input-wrap" @click.stop>
        <input
          v-model="rootInput"
          class="path-input"
          placeholder="/home/user/project"
          @keydown.enter="changeRoot"
          @keydown.esc="showInput = false"
        />
        <button class="path-btn" @click="changeRoot">{{ t('goBtn') }}</button>
      </div>
    </div>

    <div v-if="error" class="explorer-error">{{ error }}</div>
    <div v-if="loading" class="explorer-loading">
      <span class="spinner" /> Loading…
    </div>

    <!-- Tree -->
    <div v-else class="tree-scroll">
      <div v-if="tree.length === 0 && !loading && rootLabel" class="explorer-empty">
        Folder is empty
      </div>
      <TreeNode
        v-for="node in tree"
        :key="node.path"
        :node="node"
        :depth="0"
        @toggle="toggle"
        @contextmenu="(e, n) => openCtxMenu(e, n)"
      />
    </div>

    <!-- Delete confirmation modal -->
    <Teleport to="body">
      <div v-if="deleteModal" class="delete-overlay" @click.self="cancelDelete">
        <div class="delete-modal">
          <div class="delete-icon">
            <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="#f38ba8" stroke-width="1.5">
              <path d="M12 9v4m0 4h.01M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/>
            </svg>
          </div>
          <h3 class="delete-title">{{ t('moveToTrash') }}</h3>
          <p class="delete-warn">
            <strong>"{{ deleteModal.node.name }}"</strong> {{ t('deleteWarnPart1') }}
            <code>~/.local/share/Trash</code>.<br>
            {{ t('deleteWarnPart2') }}
          </p>
          <div class="delete-actions">
            <button class="delete-cancel" @click="cancelDelete">{{ t('cancel') }}</button>
            <button class="delete-confirm" @click="confirmDelete()">{{ t('moveToTrash') }}</button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Context menu -->
    <Teleport to="body">
      <div
        v-if="ctxMenu"
        class="ctx-menu"
        :style="{ left: ctxMenu.x + 'px', top: ctxMenu.y + 'px' }"
        @click.stop
      >
        <button class="ctx-item" @click="startCreate('file')">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M2 1.75C2 .784 2.784 0 3.75 0h6.586c.464 0 .909.184 1.237.513l2.914 2.914c.329.328.513.773.513 1.237v9.586A1.75 1.75 0 0113.25 16h-9.5A1.75 1.75 0 012 14.25V1.75z"/></svg>
          {{ t('newFile') }}
        </button>
        <button class="ctx-item" @click="startCreate('folder')">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="#e8a838"><path d="M1.75 1A1.75 1.75 0 000 2.75v10.5C0 14.22.784 15 1.75 15h12.5A1.75 1.75 0 0016 13.25v-8.5A1.75 1.75 0 0014.25 3H7.5L6.34 1.5A1.75 1.75 0 004.99 1H1.75z"/></svg>
          {{ t('newFolder') }}
        </button>
        <div class="ctx-sep" />
        <button class="ctx-item" @click="startRename">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M12.854.146a.5.5 0 00-.707 0L10.5 1.793 14.207 5.5l1.647-1.646a.5.5 0 000-.708l-3-3zm.646 6.061L9.793 2.5 3.293 9H3.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.207l6.5-6.5zm-7.468 7.468A.5.5 0 016 13.5V13h-.5a.5.5 0 01-.5-.5V12h-.5a.5.5 0 01-.5-.5V11h-.5a.5.5 0 01-.5-.5V10h-.5a.499.499 0 01-.175-.032l-.179.178a.5.5 0 00-.11.168l-2 5a.5.5 0 00.65.65l5-2a.5.5 0 00.168-.11l.178-.178z"/></svg>
          {{ t('rename') }}  <span style="opacity:.4;margin-left:auto;font-size:10px">F2</span>
        </button>
        <button class="ctx-item" @click="openInFileManager">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M1 3.5A1.5 1.5 0 012.5 2h2.764c.958 0 1.76.56 2.311 1.184C7.985 3.648 8.48 4 9 4h4.5A1.5 1.5 0 0115 5.5v7a1.5 1.5 0 01-1.5 1.5h-11A1.5 1.5 0 011 12.5v-9z"/></svg>
          {{ t('openInFileManager') }}
        </button>
        <div class="ctx-sep" />
        <button class="ctx-item ctx-item--danger" @click="deleteNode">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M5.5 5.5A.5.5 0 016 6v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm2.5 0a.5.5 0 01.5.5v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm3 .5a.5.5 0 00-1 0v6a.5.5 0 001 0V6z"/><path fill-rule="evenodd" d="M14.5 3a1 1 0 01-1 1H13v9a2 2 0 01-2 2H5a2 2 0 01-2-2V4h-.5a1 1 0 01-1-1V2a1 1 0 011-1H6a1 1 0 011-1h2a1 1 0 011 1h3.5a1 1 0 011 1v1zM4.118 4L4 4.059V13a1 1 0 001 1h6a1 1 0 001-1V4.059L11.882 4H4.118zM2.5 3V2h11v1h-11z"/></svg>
          {{ t('moveToTrash') }}
        </button>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.explorer {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
  font-family: var(--font-ui);
  color: var(--fg);
  position: relative;
  outline: none;  /* focus ring gestionado por tree-row--focused */
}

.explorer-header {
  padding: 6px 8px 4px 14px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-shrink: 0;
}
.explorer-title {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 1.5px;
  color: var(--fg-muted);
}
.header-actions { display: flex; gap: 2px; }
.hdr-btn {
  width: 24px;
  height: 24px;
  background: none;
  border: none;
  color: var(--fg-muted);
  border-radius: 4px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background 0.12s, color 0.12s;
}
.hdr-btn:hover { background: var(--bg-hover); color: var(--fg); }
.hdr-btn--active { color: var(--accent) !important; }

.folder-root {
  display: flex; align-items: center; gap: 6px;
  padding: 5px 8px 5px 10px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0; user-select: none;
}
.folder-icon { width: 14px; height: 14px; color: #e8a838; flex-shrink: 0; }
.folder-name { flex: 1; font-size: 12px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.open-folder-btn {
  width: 22px; height: 22px; background: none; border: none;
  color: var(--fg-muted); cursor: pointer; border-radius: 4px;
  display: flex; align-items: center; justify-content: center;
  flex-shrink: 0; transition: all 0.12s;
}
.open-folder-btn:hover { background: var(--bg-hover); color: var(--accent); }

/* Empty state */
.explorer-empty-state {
  display: flex; flex-direction: column; align-items: center;
  padding: 28px 16px 20px; gap: 10px; text-align: center;
}
.empty-icon { color: var(--fg-muted); opacity: 0.3; }
.empty-label { font-size: 12px; color: var(--fg-muted); margin: 0; }
.open-folder-big {
  display: flex; align-items: center; gap: 7px;
  background: var(--accent); border: none; border-radius: 8px;
  color: var(--accent-fg); font-size: 12.5px; font-weight: 600;
  padding: 8px 16px; cursor: pointer; transition: opacity 0.12s;
  width: 100%;  justify-content: center;
}
.open-folder-big:hover { opacity: 0.85; }
.empty-hint { font-size: 10.5px; color: var(--fg-muted); margin: 4px 0 0; }
.chevron-icon { width: 10px; height: 10px; color: var(--fg-muted); flex-shrink: 0; }

.path-input-wrap {
  display: flex;
  gap: 4px;
  padding: 6px 8px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
  background: var(--bg-darkest);
}
.path-input {
  flex: 1;
  background: var(--bg-mid);
  border: 1px solid var(--border);
  border-radius: 5px;
  color: var(--fg);
  font-size: 11.5px;
  padding: 4px 8px;
  outline: none;
  font-family: var(--font-mono);
  min-width: 0;
}
.path-input:focus { border-color: var(--accent); }
.path-btn {
  background: var(--accent);
  border: none;
  border-radius: 5px;
  color: #fff;
  font-size: 11px;
  font-weight: 600;
  padding: 4px 10px;
  cursor: pointer;
}
.path-btn:hover { opacity: 0.85; }

.creating-wrap {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 10px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-panel);
  flex-shrink: 0;
}
.create-input {
  flex: 1;
  background: var(--bg-mid);
  border: 1px solid var(--accent);
  border-radius: 4px;
  color: var(--fg);
  font-size: 12px;
  padding: 3px 6px;
  outline: none;
  font-family: var(--font-mono);
}

.explorer-error { font-size: 11px; color: var(--red); padding: 6px 10px; flex-shrink: 0; }
.explorer-loading {
  display: flex; align-items: center; gap: 6px;
  font-size: 12px; color: var(--fg-muted); padding: 8px 12px;
}
.explorer-empty { font-size: 12px; color: var(--fg-muted); padding: 10px 14px; font-style: italic; }

.spinner {
  display: inline-block;
  width: 10px; height: 10px;
  border: 2px solid var(--fg-muted);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin 0.7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

.tree-scroll {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 4px 0;
}
.tree-scroll::-webkit-scrollbar { width: 4px; }
.tree-scroll::-webkit-scrollbar-thumb { background: var(--bg-active); border-radius: 2px; }

.slide-enter-active, .slide-leave-active { transition: all 0.15s ease; }
.slide-enter-from, .slide-leave-to { opacity: 0; transform: translateY(-4px); }
</style>

<style>
/* Context menu - global */
.ctx-menu {
  position: fixed;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 7px;
  padding: 4px;
  z-index: 9999;
  min-width: 150px;
  box-shadow: 0 8px 24px rgba(0,0,0,0.5);
}
.ctx-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  background: none;
  border: none;
  color: var(--fg);
  font-size: 12px;
  font-family: var(--font-ui);
  padding: 6px 10px;
  border-radius: 5px;
  cursor: pointer;
  text-align: left;
  transition: background 0.1s;
}
.ctx-item:hover { background: var(--bg-hover); }
.ctx-item--danger { color: var(--red); }
.ctx-item--danger:hover { background: rgba(243,139,168,0.1); }
.ctx-sep { height: 1px; background: var(--border); margin: 4px 0; }

/* Delete modal */
.delete-overlay {
  position: fixed; inset: 0; z-index: 99999;
  background: rgba(0,0,0,0.6);
  display: flex; align-items: center; justify-content: center;
  backdrop-filter: blur(2px);
}
.delete-modal {
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 24px;
  width: 320px;
  box-shadow: 0 20px 60px rgba(0,0,0,0.7);
  text-align: center;
}
.delete-icon { margin-bottom: 12px; }
.delete-title {
  font-size: 14px; font-weight: 700;
  color: var(--fg-bright); margin-bottom: 10px;
  word-break: break-all;
}
.delete-warn {
  font-size: 12px; color: var(--fg-dim); line-height: 1.5;
  margin-bottom: 18px;
}
.delete-warn strong { color: var(--red); }
.delete-actions { display: flex; gap: 8px; }
.delete-cancel {
  flex: 1; background: var(--bg-hover);
  border: 1px solid var(--border); border-radius: 7px;
  color: var(--fg); font-size: 12px; font-weight: 600;
  padding: 8px; cursor: pointer; transition: background 0.12s;
}
.delete-cancel:hover { background: var(--bg-active); }
.delete-confirm {
  flex: 1; background: rgba(243,139,168,0.15);
  border: 1px solid rgba(243,139,168,0.4); border-radius: 7px;
  color: var(--red); font-size: 12px; font-weight: 700;
  padding: 8px; cursor: pointer; transition: all 0.12s;
}
.delete-confirm:hover { background: rgba(243,139,168,0.25); }
</style>
