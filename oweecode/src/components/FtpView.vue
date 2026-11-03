<script lang="ts">
// one transfer queue per connection, shared by every mount of this tab
import type { createTransferQueue as CTQ } from '../composables/transferQueue'
const queues = new Map<string, ReturnType<typeof CTQ>>()
// the newest mounted tab of a connection is the one to refresh when its queue finishes
const idleHandlers = new Map<string, (s: { done: number; failed: number }) => void>()
</script>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch, computed, nextTick, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { ask } from '@tauri-apps/plugin-dialog'
import { useEditorStore } from '../composables/useEditorStore'
import { useI18n } from '../composables/useI18n'
import { createTransferQueue, type QItem } from '../composables/transferQueue'
import { DEFAULT_IGNORE, isIgnored, neededDirs, planSync, type SyncFile, type SyncPlan } from '../composables/folderSync'
import { applyPolicy, destinationDirs, findConflicts, nameOf, type Conflict, type Existing, type Policy, type XferFile } from '../composables/conflicts'
import { useFtpStore } from '../composables/useFtpStore'
import { connectFtp, disconnectFtp } from '../composables/useFtpConnect'
import { baseName, enterAction, nextIndex, parentPath, rangeSet, summarize, typeAheadIndex, type NavItem } from '../composables/fileNav'

interface Props { connId: string; connName: string; protocol: string }
interface RemoteEntry { name: string; path: string; is_dir: boolean; size: number; modified: string }
interface LocalEntry  { name: string; path: string; is_dir: boolean; size: number; modified: string }

const props = defineProps<Props>()
const store = useEditorStore()
const { t } = useI18n()

// ── Remote state ───────────────────────────────────────────────────────────────
const remotePath       = ref('/')
const remoteStack      = ref<string[]>([])
const remoteItems      = ref<RemoteEntry[]>([])
const remoteLoading    = ref(false)
const remoteError      = ref('')
const remoteSelected   = ref(new Set<string>())   // multi-select by path
const remotePreviewEntry = ref<RemoteEntry | null>(null)
const remoteContent    = ref<string | null>(null)
const remotePreview    = ref(false)
const isConnected      = ref(true)

// ── Local state ────────────────────────────────────────────────────────────────
const localPath    = ref(store.state.rootPath || '')
const localStack   = ref<string[]>([])
const localItems   = ref<LocalEntry[]>([])
const localLoading = ref(false)
// Multi-select: Set of paths
const localSelected = ref(new Set<string>())
// Local preview
const localPreviewEntry = ref<LocalEntry | null>(null)
const localContent      = ref<string | null>(null)
const localPreview      = ref(false)

// ── Transfer queue ─────────────────────────────────────────────────────────────
// One queue per connection, kept at module level so it keeps running if the tab is switched away.
let unlistenTransferProgress: UnlistenFn | null = null
const queue = getQueue()
const qs = queue.state
const qCounts = computed(() => queue.counts())
const qBytes = computed(() => queue.bytes())
const queueTab = ref<'queued' | 'error' | 'done'>('queued')
const queueOpen = ref(true)
const transferring = computed(() => qs.running || qCounts.value.queued > 0)
// kept for the top bar: the file in progress and the batch counters
const uploadProgress = computed(() => {
  const c = qCounts.value
  if (!qs.items.length || (!qs.running && c.queued === 0)) return null
  return { done: c.done + c.error, total: c.queued + c.active + c.done + c.error, currentName: qs.current?.name ?? '', hasError: c.error > 0 }
})
const filePercent = computed(() => qs.current ? Math.round((qs.current.sent / Math.max(1, qs.current.size)) * 100) : 0)
const byteLabel = computed(() => qs.current ? `${formatSize(qs.current.sent) || '0 B'} / ${formatSize(qs.current.size) || '0 B'}` : '')
const speedLabel = computed(() => {
  if (!qs.running || !qs.speed) return ''
  const left = Math.max(0, qBytes.value.total - qBytes.value.sent)
  const eta = left / qs.speed
  return `${formatSize(qs.speed)}/s${eta > 1 ? ` · ${t('fEta', { x: eta < 90 ? Math.round(eta) + ' s' : Math.round(eta / 60) + ' min' })}` : ''}`
})
const queueRows = computed(() => qs.items.filter(i => queueTab.value === 'queued' ? (i.status === 'queued' || i.status === 'active') : queueTab.value === 'error' ? (i.status === 'error' || i.status === 'canceled') : i.status === 'done'))
const dirIcon = (i: QItem) => (i.dir === 'up' ? '⬆' : '⬇')

// ── Keyboard navigation (FileZilla-like) ───────────────────────────────────────
type Side = 'local' | 'remote'
const localFocusIdx  = ref(-1)
const remoteFocusIdx = ref(-1)
const localBodyEl    = ref<HTMLElement | null>(null)
const remoteBodyEl   = ref<HTMLElement | null>(null)
const anchors: Record<Side, number> = { local: -1, remote: -1 }          // where a Shift-range starts
const pendingFocus: Record<Side, string> = { local: '', remote: '' }    // after going up, land on the folder we came from
const typed = { text: '', timer: 0 as ReturnType<typeof setTimeout> | 0 }

function pane(side: Side) {
  return side === 'local'
    ? { items: localItems as Ref<NavItem[]>, focus: localFocusIdx, sel: localSelected, body: localBodyEl, path: localPath }
    : { items: remoteItems as Ref<NavItem[]>, focus: remoteFocusIdx, sel: remoteSelected, body: remoteBodyEl, path: remotePath }
}

function scrollRowIntoView(el: HTMLElement | null, idx: number) {
  if (!el) return
  const row = el.querySelectorAll<HTMLElement>('.pane-row')[idx]
  row?.scrollIntoView({ block: 'nearest' })
}

/** ↑ ↓ Home End PageUp PageDown: arrows select the row, Shift extends a range, Ctrl moves without selecting */
function moveFocus(side: Side, key: string, e: KeyboardEvent) {
  const p = pane(side), items = p.items.value
  const page = Math.max(1, Math.floor((p.body.value?.clientHeight ?? 300) / 27) - 1)
  const idx = nextIndex(key, p.focus.value, items.length, page)
  if (idx === null) return
  const before = p.focus.value < 0 ? 0 : p.focus.value
  p.focus.value = idx
  if (e.ctrlKey || e.metaKey) { /* focus only */ }
  else if (e.shiftKey) { if (anchors[side] < 0) anchors[side] = before; p.sel.value = rangeSet(items, anchors[side], idx) }
  else { anchors[side] = idx; p.sel.value = new Set([items[idx].path]) }
  scrollRowIntoView(p.body.value, idx)
}

function focusPane(side: Side) {
  const p = pane(side)
  if (p.focus.value < 0 && p.items.value.length) p.focus.value = 0
  p.body.value?.focus()
}

function refreshPane(side: Side) { return side === 'local' ? loadLocal(localPath.value) : loadRemote(remotePath.value) }
function goBack(side: Side) { side === 'local' ? localBack() : remoteBack() }
function goUp(side: Side) {
  const p = pane(side)
  if (p.path.value === '/' || p.path.value === '') return
  pendingFocus[side] = baseName(p.path.value)
  if (side === 'local') localUp(); else remoteUp()
}
function remoteUp() {
  remoteStack.value.push(remotePath.value)
  loadRemote(parentPath(remotePath.value))
}
function enterDir(side: Side, entry: NavItem) {
  if (side === 'local') { localStack.value.push(localPath.value); loadLocal(entry.path) }
  else { remoteStack.value.push(remotePath.value); loadRemote(entry.path) }
}
async function transferSelection(side: Side, entries?: NavItem[]) {
  const p = pane(side)
  if (entries && entries.length) p.sel.value = new Set(entries.map(x => x.path))
  else if (p.sel.value.size === 0 && p.items.value[p.focus.value]) p.sel.value = new Set([p.items.value[p.focus.value].path])
  if (side === 'local') await uploadSelected(); else await downloadSelected()
}
function editEntry(side: Side, entry: NavItem) {
  if (entry.is_dir) return
  if (side === 'local') store.openFile(entry.path); else previewRemote(entry as RemoteEntry)
}
function renameFocused(side: Side) {
  const entry = pane(side).items.value[pane(side).focus.value]
  if (!entry) return
  if (side === 'local') startLocalRename(entry as LocalEntry); else startRename(entry as RemoteEntry)
}
function deleteSelection(side: Side) { return side === 'local' ? deleteLocalSelected() : deleteRemoteSelected() }

function typeAhead(side: Side, ch: string) {
  const p = pane(side)
  typed.text += ch
  if (typed.timer) clearTimeout(typed.timer)
  typed.timer = setTimeout(() => { typed.text = '' }, 900)
  const idx = typeAheadIndex(p.items.value, typed.text, p.focus.value)
  if (idx >= 0) { p.focus.value = idx; anchors[side] = idx; p.sel.value = new Set([p.items.value[idx].path]); scrollRowIntoView(p.body.value, idx) }
}

function onPaneKey(side: Side, e: KeyboardEvent) {
  if ((side === 'local' ? localRenaming.value : renaming.value) || pathEdit.value) return
  if ((e.target as HTMLElement)?.tagName === 'INPUT' || (e.target as HTMLElement)?.tagName === 'TEXTAREA') return
  const p = pane(side), items = p.items.value
  const k = e.key, mod = e.ctrlKey || e.metaKey
  const stop = () => e.preventDefault()

  if (k === 'Tab') { stop(); focusPane(side === 'local' ? 'remote' : 'local'); return }
  if (e.altKey && k === 'ArrowLeft') { stop(); goBack(side); return }
  if (e.altKey && k === 'ArrowUp') { stop(); goUp(side); return }
  if (mod && k.toLowerCase() === 'l') { stop(); startPathEdit(side); return }
  if (mod && k.toLowerCase() === 'a') { stop(); p.sel.value = new Set(items.map(i => i.path)); return }
  if (k === 'F5' || (mod && k.toLowerCase() === 'r')) { stop(); refreshPane(side); return }
  if (k === 'Backspace' || k === 'ArrowLeft') { stop(); goUp(side); return }
  if (k === 'Escape') { p.sel.value = new Set(); typed.text = ''; ctx.value = null; return }
  if (k === 'F7') { stop(); showNewDir.value = side === 'local' ? 'local' : 'remote'; newDirName.value = ''; return }
  if (['ArrowUp', 'ArrowDown', 'Home', 'End', 'PageUp', 'PageDown'].includes(k)) { stop(); moveFocus(side, k, e); return }
  if (items.length === 0) return

  const focused = items[p.focus.value]
  if (k === 'ArrowRight') { stop(); if (focused?.is_dir) enterDir(side, focused); return }
  if (k === ' ') {
    stop()
    if (!focused) return
    const s = new Set(p.sel.value)
    s.has(focused.path) ? s.delete(focused.path) : s.add(focused.path)
    p.sel.value = s; anchors[side] = p.focus.value
    return
  }
  if (k === 'Enter') {
    stop()
    const act = enterAction(items, p.sel.value, p.focus.value)
    if (act.kind === 'enter') enterDir(side, act.entry)
    else if (act.kind === 'transfer') transferSelection(side, act.entries)
    return
  }
  if (k === 'F2') { stop(); renameFocused(side); return }
  if (k === 'F4') { stop(); if (focused) editEntry(side, focused); return }
  if (k === 'Delete') { stop(); if (p.sel.value.size === 0 && focused) p.sel.value = new Set([focused.path]); deleteSelection(side); return }
  // anything printable jumps to the next name that starts with it
  if (k.length === 1 && !mod && !e.altKey) { stop(); typeAhead(side, k) }
}
const onLocalKeydown = (e: KeyboardEvent) => onPaneKey('local', e)
const onRemoteKeydown = (e: KeyboardEvent) => onPaneKey('remote', e)

// After a reload: land on the folder we just left (when going up), else on the first row
function afterLoad(side: Side) {
  const p = pane(side), items = p.items.value
  let idx = items.length ? 0 : -1
  if (pendingFocus[side]) { const i = items.findIndex(x => x.name === pendingFocus[side]); if (i >= 0) idx = i; pendingFocus[side] = '' }
  p.focus.value = idx; anchors[side] = idx
  if (idx >= 0 && pendingFocus[side] === '' && items[idx]) { /* keep the selection the load already cleared */ }
  nextTick(() => scrollRowIntoView(p.body.value, idx))
}
watch(localItems, () => afterLoad('local'))
watch(remoteItems, () => { remoteSelected.value = new Set(); afterLoad('remote') })

// Ctrl+L / ✎: type a path and press Enter, like FileZilla's address bar
const pathEdit = ref<{ side: Side; text: string } | null>(null)
const pathInputEl = ref<HTMLInputElement | null>(null)
function startPathEdit(side: Side) {
  pathEdit.value = { side, text: pane(side).path.value }
  nextTick(() => { pathInputEl.value?.focus(); pathInputEl.value?.select() })
}
async function commitPathEdit() {
  const pe = pathEdit.value; if (!pe) return
  pathEdit.value = null
  const target = pe.text.trim() || '/'
  if (target === pane(pe.side).path.value) { focusPane(pe.side); return }
  if (pe.side === 'local') { localStack.value.push(localPath.value); await loadLocal(target) }
  else { remoteStack.value.push(remotePath.value); await loadRemote(target) }
  focusPane(pe.side)
}
function cancelPathEdit() { const s = pathEdit.value?.side; pathEdit.value = null; if (s) focusPane(s) }

// What double-click does on a file: transfer it (FileZilla) or open it in the editor
const dblAction = ref<'transfer' | 'edit'>((() => { try { return localStorage.getItem('ftp_dbl_action') === 'edit' ? 'edit' : 'transfer' } catch { return 'transfer' } })())
function toggleDblAction() {
  dblAction.value = dblAction.value === 'transfer' ? 'edit' : 'transfer'
  try { localStorage.setItem('ftp_dbl_action', dblAction.value) } catch { /* ignore */ }
}

// Right-click menu (replaces the generic cut/copy/paste one)
const ctx = ref<{ side: Side; x: number; y: number; entry: NavItem | null } | null>(null)
function openCtx(side: Side, entry: NavItem | null, e: MouseEvent) {
  e.preventDefault(); e.stopPropagation()
  const p = pane(side)
  if (entry && !p.sel.value.has(entry.path)) {
    p.sel.value = new Set([entry.path])
    p.focus.value = p.items.value.findIndex(i => i.path === entry.path)
    anchors[side] = p.focus.value
  }
  ctx.value = { side, x: Math.min(e.clientX, window.innerWidth - 230), y: Math.min(e.clientY, window.innerHeight - 330), entry }
}
function ctxDo(action: string) {
  const c = ctx.value; if (!c) return
  ctx.value = null
  const side = c.side, p = pane(side)
  const selCount = p.sel.value.size
  switch (action) {
    case 'open': if (c.entry?.is_dir) enterDir(side, c.entry); break
    case 'transfer': transferSelection(side); break
    case 'edit': if (c.entry) editEntry(side, c.entry); break
    case 'rename': renameFocused(side); break
    case 'delete': if (selCount) deleteSelection(side); break
    case 'mkdir': showNewDir.value = side === 'local' ? 'local' : 'remote'; newDirName.value = ''; break
    case 'newfile': showNewDir.value = side === 'local' ? 'local-file' : 'remote-file'; newDirName.value = ''; break
    case 'refresh': refreshPane(side); break
    case 'up': goUp(side); break
    case 'copypath': { const path = c.entry?.path ?? p.path.value; navigator.clipboard?.writeText(path).then(() => setStatus(t('fCopiedPath', { path }), 'ok')).catch(() => {}); break }
    case 'selectall': p.sel.value = new Set(p.items.value.map(i => i.path)); break
  }
}
const closeCtx = () => { ctx.value = null }
const showKeys = ref(false)
const localSummary = computed(() => summarize(localItems.value, localSelected.value, t))
const remoteSummary = computed(() => summarize(remoteItems.value, remoteSelected.value, t))

// ── UI state ───────────────────────────────────────────────────────────────────
const splitPos     = ref(50)
const isDragging   = ref(false)
const statusMsg    = ref('')
const statusType   = ref<'ok' | 'err' | ''>('')
const showNewDir   = ref<'remote' | 'remote-file' | 'local' | 'local-file' | null>(null)
const newDirName   = ref('')
const renaming     = ref<{ entry: RemoteEntry; name: string } | null>(null)
const localRenaming = ref<{ entry: LocalEntry; name: string } | null>(null)

// ── Splitter ──────────────────────────────────────────────────────────────────
function onSplitMousedown(e: MouseEvent) {
  isDragging.value = true
  const container = (e.currentTarget as HTMLElement).closest('.ftp-main') as HTMLElement
  const onMove = (ev: MouseEvent) => {
    const rect = container.getBoundingClientRect()
    splitPos.value = Math.max(22, Math.min(75, ((ev.clientX - rect.left) / rect.width) * 100))
  }
  const onUp = () => { isDragging.value = false; window.removeEventListener('mousemove', onMove); window.removeEventListener('mouseup', onUp) }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
}

// ── Local navigation ───────────────────────────────────────────────────────────
async function loadLocal(path: string) {
  localLoading.value = true
  localSelected.value = new Set()
  localPreview.value = false
  try {
    localItems.value = await invoke<LocalEntry[]>('list_dir', { path, showHidden: true })
    localPath.value  = path
  } catch (e) { setStatus(String(e), 'err') }
  finally { localLoading.value = false }
}

// Single click always just selects — folders used to navigate straight in on
// click, which made it impossible to pick a folder (to download it, say)
// without landing inside it. Double click is what navigates/opens now, same
// as FileZilla.
function localClick(entry: LocalEntry, e: MouseEvent) {
  const idx = localItems.value.findIndex(i => i.path === entry.path)
  if (e.shiftKey && anchors.local >= 0) { localSelected.value = rangeSet(localItems.value, anchors.local, idx); return }
  anchors.local = idx
  if (e.ctrlKey || e.metaKey) {
    const s = new Set(localSelected.value)
    if (s.has(entry.path)) s.delete(entry.path)
    else s.add(entry.path)
    localSelected.value = s
  } else {
    localSelected.value = new Set([entry.path])
  }
}

function toggleLocalCheck(entry: LocalEntry) {
  const s = new Set(localSelected.value)
  if (s.has(entry.path)) s.delete(entry.path)
  else s.add(entry.path)
  localSelected.value = s
}

function localDblclick(entry: LocalEntry) {
  if (entry.is_dir) { localStack.value.push(localPath.value); loadLocal(entry.path); return }
  if (dblAction.value === 'transfer') { transferSelection('local', [entry]); return }
  store.openFile(entry.path)
}

// ── Local rename / delete (mirrors the remote pane's) ──────────────────────────
function startLocalRename(entry: LocalEntry) { localRenaming.value = { entry, name: entry.name } }

async function confirmLocalRename() {
  if (!localRenaming.value) return
  const { entry, name } = localRenaming.value
  if (name === entry.name) { localRenaming.value = null; return }
  const newPath = entry.path.split('/').slice(0, -1).join('/') + '/' + name
  try {
    await invoke('rename_entry', { oldPath: entry.path, newPath })
    localRenaming.value = null; setStatus(`${t('renamedPrefix')} ${name}`, 'ok')
    await loadLocal(localPath.value)
  } catch (e: any) { setStatus(String(e), 'err') }
}

async function localDelete(entry: LocalEntry) {
  if (!confirm(t('fDeleteOne', { name: entry.name }))) return
  try {
    await invoke('move_to_trash', { path: entry.path })
    if (localPreviewEntry.value?.path === entry.path) { localPreviewEntry.value = null; localPreview.value = false }
    setStatus(`${t('deletedPrefix')}: ${entry.name}`, 'ok')
    await loadLocal(localPath.value)
  } catch (e: any) { setStatus(String(e), 'err') }
}

async function deleteLocalSelected() {
  const toDelete = localItems.value.filter(e => localSelected.value.has(e.path))
  if (toDelete.length === 0) return
  if (!confirm(t('fDeleteMany', { n: toDelete.length }))) return
  let errors = 0
  for (const entry of toDelete) {
    try { await invoke('move_to_trash', { path: entry.path }) }
    catch { errors++ }
  }
  if (localPreviewEntry.value && localSelected.value.has(localPreviewEntry.value.path)) {
    localPreviewEntry.value = null; localPreview.value = false
  }
  if (errors === 0) setStatus(`${t('deletedCountPrefix')} ${toDelete.length} ${t('itemsWord')}`, 'ok')
  else setStatus(`${toDelete.length - errors} ${t('deletedCountPrefix').toLowerCase()}, ${errors} ${t('errorsWord')}`, 'err')
  await loadLocal(localPath.value)
}

function localBack()  { const p = localStack.value.pop(); if (p) loadLocal(p) }
function localHome()  { localStack.value = []; loadLocal(store.state.rootPath || localPath.value) }
function localUp()    {
  const parent = localPath.value.split('/').slice(0, -1).join('/') || '/'
  localStack.value.push(localPath.value)
  loadLocal(parent)
}

// ── Local preview ──────────────────────────────────────────────────────────────
async function openLocalPreview(entry: LocalEntry) {
  localPreviewEntry.value = entry
  localPreview.value = true
  localContent.value = null
  try {
    localContent.value = await invoke<string>('open_file', { path: entry.path })
  } catch (e: any) { localContent.value = `[${t('readErrorPrefix')}: ${e}]` }
}

async function saveLocal() {
  if (!localPreviewEntry.value || localContent.value === null) return
  try {
    await invoke('save_file', { path: localPreviewEntry.value.path, content: localContent.value })
    setStatus(`${t('savedPrefix')}: ${localPreviewEntry.value.name}`, 'ok')
  } catch (e: any) { setStatus(String(e), 'err') }
}

// ── Remote navigation ──────────────────────────────────────────────────────────
async function loadRemote(path: string) {
  remoteLoading.value = true
  remoteError.value = ''
  remoteContent.value = null
  remotePreview.value = false
  remoteSelected.value = new Set()
  try {
    remoteItems.value = await invoke<RemoteEntry[]>('remote_list_dir', { id: props.connId, path })
    remotePath.value  = path
  } catch (e: any) { remoteError.value = String(e) }
  finally { remoteLoading.value = false }
}

// Same as the local pane: single click only selects (folders included), so a
// folder can be picked for download without being forced to enter it. Double
// click is what navigates into folders / opens files.
function remoteClick(entry: RemoteEntry, e: MouseEvent) {
  const idx = remoteItems.value.findIndex(i => i.path === entry.path)
  if (e.shiftKey && anchors.remote >= 0) { remoteSelected.value = rangeSet(remoteItems.value, anchors.remote, idx); return }
  anchors.remote = idx
  if (e.ctrlKey || e.metaKey) {
    const s = new Set(remoteSelected.value)
    if (s.has(entry.path)) s.delete(entry.path)
    else s.add(entry.path)
    remoteSelected.value = s
  } else {
    remoteSelected.value = new Set([entry.path])
    remotePreviewEntry.value = entry
  }
}

function toggleRemoteCheck(entry: RemoteEntry) {
  const s = new Set(remoteSelected.value)
  if (s.has(entry.path)) s.delete(entry.path)
  else s.add(entry.path)
  remoteSelected.value = s
}

function remoteDblclick(entry: RemoteEntry) {
  if (entry.is_dir) { remoteStack.value.push(remotePath.value); loadRemote(entry.path); return }
  if (dblAction.value === 'transfer') { transferSelection('remote', [entry]); return }
  previewRemote(entry)
}

function remoteBack() { const p = remoteStack.value.pop(); if (p) loadRemote(p) }

const remoteBreadcrumbs = computed(() => {
  const parts = remotePath.value.split('/').filter(Boolean)
  const r = [{ label: '/', path: '/' }]
  let cur = ''
  for (const p of parts) { cur += '/' + p; r.push({ label: p, path: cur }) }
  return r
})

const localBreadcrumbs = computed(() => {
  const parts = localPath.value.split('/').filter(Boolean)
  const r = [{ label: '/', path: '/' }]
  let cur = ''
  for (const p of parts) { cur += '/' + p; r.push({ label: p, path: cur }) }
  return r
})

function goToLocal(path: string) {
  if (path === localPath.value) return
  localStack.value.push(localPath.value)
  loadLocal(path)
}

// ── Remote preview → open as editor tab ────────────────────────────────────────
async function previewRemote(entry: RemoteEntry) {
  if (entry.is_dir) return
  remoteSelected.value = new Set([entry.path])
  try {
    const content = await invoke<string>('remote_read_file', { id: props.connId, path: entry.path })
    store.openFtpFile(props.connId, entry.path, entry.name, content)
    setStatus(`${t('openedPrefix')}: ${entry.name}`, 'ok')
  } catch (e: any) {
    // Fallback to inline preview for binary/unreadable files
    remotePreviewEntry.value = entry
    remotePreview.value = true
    remoteContent.value = `[${t('readErrorPrefix')}: ${e}]`
    setStatus(String(e), 'err')
  }
}

async function saveRemote() {
  if (!remotePreviewEntry.value || remoteContent.value === null) return
  try {
    await invoke('remote_write_file', { id: props.connId, path: remotePreviewEntry.value.path, content: remoteContent.value })
    setStatus(`${t('savedServerPrefix')}: ${remotePreviewEntry.value.name}`, 'ok')
  } catch (e: any) { setStatus(String(e), 'err') }
}

// ── Multi-file/folder Upload ───────────────────────────────────────────────────
// Collect all files recursively from a local entry
async function collectFiles(entry: LocalEntry, remoteBase: string): Promise<{ localPath: string; remotePath: string; size: number }[]> {
  if (!entry.is_dir) {
    return [{ localPath: entry.path, remotePath: remoteBase + '/' + entry.name, size: entry.size }]
  }
  const children = await invoke<LocalEntry[]>('list_dir', { path: entry.path, showHidden: true })
  const results: { localPath: string; remotePath: string; size: number }[] = []
  const subBase = remoteBase + '/' + entry.name
  for (const child of children) {
    results.push(...await collectFiles(child, subBase))
  }
  return results
}

// Collect all files recursively from a remote entry (mirrors collectFiles()
// above, in the opposite direction) — lets downloadSelected() pull down a
// whole folder instead of silently skipping it.
async function collectRemoteFiles(entry: RemoteEntry, localBase: string): Promise<{ remotePath: string; localPath: string; size: number }[]> {
  if (!entry.is_dir) {
    return [{ remotePath: entry.path, localPath: localBase + '/' + entry.name, size: entry.size }]
  }
  const children = await invoke<RemoteEntry[]>('remote_list_dir', { id: props.connId, path: entry.path })
  const results: { remotePath: string; localPath: string; size: number }[] = []
  const subBase = localBase + '/' + entry.name
  for (const child of children) {
    results.push(...await collectRemoteFiles(child, subBase))
  }
  return results
}

// ── Drag & drop between panes (FileZilla style): local → remote uploads,
// remote → local downloads. Dragging an unselected row selects it first.
const dragSource = ref<'local' | 'remote' | null>(null)
const dropHover = ref<'local' | 'remote' | null>(null)
function onDragStart(side: 'local' | 'remote', entry: { path: string }, ev: DragEvent) {
  const sel = side === 'local' ? localSelected : remoteSelected
  if (!sel.value.has(entry.path)) sel.value = new Set([entry.path])
  dragSource.value = side
  ev.dataTransfer?.setData('text/plain', entry.path)
  if (ev.dataTransfer) ev.dataTransfer.effectAllowed = 'copy'
}
function onDragOver(side: 'local' | 'remote', ev: DragEvent) {
  if (dragSource.value && dragSource.value !== side) { ev.preventDefault(); dropHover.value = side }
}
async function onDrop(side: 'local' | 'remote') {
  const from = dragSource.value
  dragSource.value = null; dropHover.value = null
  if (!from || from === side || transferring.value) return
  if (side === 'remote') await uploadSelected()
  else await downloadSelected()
}

// ── Files that already exist at the destination: ask what to do (like FileZilla) ──
const overwriteAsk = ref<{ conflicts: Conflict[]; total: number; verb: string; resolve: (p: Policy | null) => void } | null>(null)
function askPolicy(conflicts: Conflict[], total: number, verb: string): Promise<Policy | null> {
  return new Promise(resolve => { overwriteAsk.value = { conflicts, total, verb, resolve } })
}
function answerOverwrite(p: Policy | null) { overwriteAsk.value?.resolve(p); overwriteAsk.value = null }

/** Lists the destination folders, finds files that would be replaced and asks once for the whole batch. null = cancelled */
async function resolveConflicts(files: XferFile[], side: 'remote' | 'local'): Promise<XferFile[] | null> {
  const existing: Existing = new Map()
  for (const dir of destinationDirs(files)) {
    try {
      const list = side === 'remote'
        ? await invoke<RemoteEntry[]>('remote_list_dir', { id: props.connId, path: dir })
        : await invoke<LocalEntry[]>('list_dir', { path: dir, showHidden: true })
      existing.set(dir, new Map(list.filter(e => !e.is_dir).map(e => [e.name, e.size])))
    } catch { /* the folder does not exist yet: nothing to replace */ }
  }
  const conflicts = findConflicts(files, existing)
  if (!conflicts.length) return files
  const policy = await askPolicy(conflicts, files.length, side === 'remote' ? t('fOnServer') : t('fOnPc'))
  if (!policy) return null
  const out = applyPolicy(files, conflicts, policy, existing)
  const skipped = files.length - out.length
  if (skipped) setStatus(t('fSkipped', { n: skipped }), 'ok')
  return out
}

function cancelActiveTransfer() { queue.cancelAll() }

// The runner the queue calls for each file: the real upload/download
async function runTransfer(item: QItem) {
  if (item.dir === 'up') await invoke('remote_upload_file', { id: props.connId, localPath: item.local, remotePath: item.remote })
  else await invoke('remote_download_file', { id: props.connId, remotePath: item.remote, localPath: item.local, totalSize: item.size })
}

function getQueue() {
  const key = props.connId
  let q = queues.get(key)
  if (!q) {
    q = createTransferQueue(runTransfer, s => idleHandlers.get(key)?.(s))
    queues.set(key, q)
  }
  idleHandlers.set(key, ({ done, failed }) => {
    refreshAfterTransfer()
    if (failed === 0) setStatus(t('fTransferred', { n: done }), 'ok')
    else setStatus(t('fTransferredErr', { done, failed }), 'err')
    if (failed > 0) { queueTab.value = 'error'; queueOpen.value = true }
  })
  return q
}
// panes show what the transfers produced
function refreshAfterTransfer() { void loadRemote(remotePath.value); void loadLocal(localPath.value) }

async function uploadSelected() {
  if (localSelected.value.size === 0) { setStatus(t('selectFilesHint'), 'err'); return }
  const selectedEntries = localItems.value.filter(e => localSelected.value.has(e.path))
  const remoteBase = remotePath.value === '/' ? '' : remotePath.value
  const allFiles: { localPath: string; remotePath: string; size: number }[] = []
  for (const entry of selectedEntries) {
    try { allFiles.push(...await collectFiles(entry, remoteBase)) }
    catch (e: any) { setStatus(String(e), 'err') }
  }
  // remote folders must exist before their files go up
  const dirs = new Set<string>()
  for (const f of allFiles) dirs.add(f.remotePath.split('/').slice(0, -1).join('/') || '/')
  for (const dir of [...dirs].sort()) {
    if (dir === '/' || dir === remoteBase) continue
    try { await invoke('remote_mkdir', { id: props.connId, path: dir }) } catch { /* exists */ }
  }
  if (dirs.size > 0) await loadRemote(remotePath.value)
  const final = await resolveConflicts(allFiles.map(f => ({ src: f.localPath, dst: f.remotePath, size: f.size })), 'remote')
  if (!final) { setStatus(t('fUploadCanceled'), 'err'); return }
  if (final.length === 0) { setStatus(t('fNothingUp'), 'ok'); return }
  queue.enqueue(final.map(f => ({ dir: 'up' as const, name: f.dst.slice(remoteBase.length + 1) || f.dst, local: f.src, remote: f.dst, size: f.size })))
  queueTab.value = 'queued'; queueOpen.value = true
  setStatus(t('fQueued', { n: final.length }), 'ok')
}

async function downloadSelected() {
  const selectedEntries = remoteItems.value.filter(e => remoteSelected.value.has(e.path))
  if (selectedEntries.length === 0) { setStatus(t('selectRemoteFilesHint'), 'err'); return }
  const base = localPath.value.endsWith('/') ? localPath.value.slice(0, -1) : localPath.value
  const allFiles: { remotePath: string; localPath: string; size: number }[] = []
  for (const entry of selectedEntries) {
    try { allFiles.push(...await collectRemoteFiles(entry, base)) }
    catch (e: any) { setStatus(String(e), 'err') }
  }
  const final = await resolveConflicts(allFiles.map(f => ({ src: f.remotePath, dst: f.localPath, size: f.size })), 'local')
  if (!final) { setStatus(t('fDownloadCanceled'), 'err'); return }
  if (final.length === 0) { setStatus(t('fNothingDown'), 'ok'); return }
  queue.enqueue(final.map(f => ({ dir: 'down' as const, name: f.dst.slice(base.length + 1) || f.dst, local: f.dst, remote: f.src, size: f.size })))
  queueTab.value = 'queued'; queueOpen.value = true
  setStatus(t('fQueued', { n: final.length }), 'ok')
}

// ── Folder sync: compare the two open folders, review the differences, queue what changes ──
const syncOpen = ref(false)
const syncDir = ref<'up' | 'down'>('up')
const syncDelete = ref(false)
const syncIgnore = ref(DEFAULT_IGNORE.join(', '))
const syncBusy = ref('')
const syncPlan = ref<SyncPlan | null>(null)
const syncBases = ref({ local: '', remote: '' })
let syncStop = false
const syncList = () => syncIgnore.value.split(/[,\n]/).map(x => x.trim()).filter(Boolean)

async function scanTree(root: string, list: (p: string) => Promise<{ name: string; path: string; is_dir: boolean; size: number }[]>, ignore: string[]): Promise<SyncFile[]> {
  const out: SyncFile[] = []
  const stack = [{ abs: root, rel: '' }]
  while (stack.length) {
    if (syncStop) throw new Error('stop')
    const { abs, rel } = stack.pop()!
    let items: { name: string; path: string; is_dir: boolean; size: number }[]
    try { items = await list(abs) } catch { continue }
    for (const e of items) {
      const r = rel ? `${rel}/${e.name}` : e.name
      if (isIgnored(r, ignore)) continue
      if (e.is_dir) { if (r.split('/').length < 30) stack.push({ abs: e.path, rel: r }) }
      else out.push({ rel: r, size: e.size })
    }
    syncBusy.value = t('ySyncScanning', { n: out.length })
    if (out.length > 50000) throw new Error(t('ySyncTooMany'))
  }
  return out
}

async function syncCompare() {
  syncStop = false; syncPlan.value = null
  syncBases.value = { local: localPath.value.replace(/\/$/, ''), remote: remotePath.value === '/' ? '' : remotePath.value }
  const ignore = syncList()
  try {
    syncBusy.value = t('ySyncScanning', { n: 0 })
    const loc = await scanTree(localPath.value, p => invoke('list_dir', { path: p, showHidden: true }), ignore)
    const rem = await scanTree(remotePath.value, p => invoke('remote_list_dir', { id: props.connId, path: p }), ignore)
    syncPlan.value = syncDir.value === 'up'
      ? planSync(loc, rem, { deleteExtra: syncDelete.value, ignore })
      : planSync(rem, loc, { ignore })
  } catch (e) {
    if (!syncStop) setStatus(t('ySyncErr', { e: String(e instanceof Error ? e.message : e) }), 'err')
  } finally { syncBusy.value = '' }
}

async function syncApply() {
  const plan = syncPlan.value
  if (!plan) return
  const { local: lb, remote: rb } = syncBases.value
  if (syncDir.value === 'up' && plan.remove.length &&
      !(await ask(t('ySyncAskDelete', { n: plan.remove.length }), { title: t('ySyncAskTitle'), kind: 'warning', okLabel: t('fCtxDelete'), cancelLabel: t('fOwCancel') }))) return
  syncOpen.value = false
  if (syncDir.value === 'up') {
    for (const dir of neededDirs(plan.copy, rb)) { try { await invoke('remote_mkdir', { id: props.connId, path: dir }) } catch { /* exists */ } }
    let deleted = 0, failed = 0
    for (const f of plan.remove) {
      try { await invoke('remote_delete', { id: props.connId, path: `${rb}/${f.rel}`, isDir: false }); deleted++ } catch { failed++ }
    }
    if (plan.remove.length) setStatus(failed ? t('ySyncDelErr', { n: failed }) : t('ySyncDeleted', { n: deleted }), failed ? 'err' : 'ok')
    if (plan.copy.length) queue.enqueue(plan.copy.map(f => ({ dir: 'up' as const, name: f.rel, local: `${lb}/${f.rel}`, remote: `${rb}/${f.rel}`, size: f.size })))
    else void loadRemote(remotePath.value)
  } else if (plan.copy.length) {
    queue.enqueue(plan.copy.map(f => ({ dir: 'down' as const, name: f.rel, local: `${lb}/${f.rel}`, remote: `${rb}/${f.rel}`, size: f.size })))
  }
  if (plan.copy.length) { queueTab.value = 'queued'; queueOpen.value = true; setStatus(t('fQueued', { n: plan.copy.length }), 'ok') }
}
const syncCounts = computed(() => {
  const p = syncPlan.value
  return p ? { nw: p.copy.filter(c => c.reason === 'new').length, ch: p.copy.filter(c => c.reason === 'changed').length, rm: p.remove.length, same: p.same } : null
})
const syncRows = computed(() => {
  const p = syncPlan.value; if (!p) return []
  return [...p.copy.map(c => ({ rel: c.rel, size: c.size, tag: c.reason })), ...p.remove.map(r => ({ rel: r.rel, size: r.size, tag: 'remove' as const }))]
})

// ── Remote actions ─────────────────────────────────────────────────────────────
async function remoteDelete(entry: RemoteEntry) {
  if (!confirm(t('fDeleteOne', { name: entry.name }))) return
  try {
    await invoke('remote_delete', { id: props.connId, path: entry.path, isDir: entry.is_dir })
    if (remotePreviewEntry.value?.path === entry.path) { remotePreviewEntry.value = null; remotePreview.value = false }
    setStatus(`${t('deletedPrefix')}: ${entry.name}`, 'ok')
    await loadRemote(remotePath.value)
  } catch (e: any) { setStatus(String(e), 'err') }
}

async function deleteRemoteSelected() {
  const toDelete = remoteItems.value.filter(e => remoteSelected.value.has(e.path))
  if (toDelete.length === 0) return
  if (!confirm(t('fDeleteMany', { n: toDelete.length }))) return
  let errors = 0
  for (const entry of toDelete) {
    try { await invoke('remote_delete', { id: props.connId, path: entry.path, isDir: entry.is_dir }) }
    catch { errors++ }
  }
  if (remotePreviewEntry.value && remoteSelected.value.has(remotePreviewEntry.value.path)) {
    remotePreviewEntry.value = null; remotePreview.value = false
  }
  if (errors === 0) setStatus(`${t('deletedCountPrefix')} ${toDelete.length} ${t('itemsWord')}`, 'ok')
  else setStatus(`${toDelete.length - errors} ${t('deletedCountPrefix').toLowerCase()}, ${errors} ${t('errorsWord')}`, 'err')
  await loadRemote(remotePath.value)
}

function startRename(entry: RemoteEntry) { renaming.value = { entry, name: entry.name } }

async function confirmRename() {
  if (!renaming.value) return
  const { entry, name } = renaming.value
  if (name === entry.name) { renaming.value = null; return }
  const newPath = entry.path.split('/').slice(0, -1).join('/') + '/' + name
  try {
    await invoke('remote_rename', { id: props.connId, oldPath: entry.path, newPath })
    renaming.value = null; setStatus(`${t('renamedPrefix')} ${name}`, 'ok')
    await loadRemote(remotePath.value)
  } catch (e: any) { setStatus(String(e), 'err') }
}

async function createRemoteDir() {
  if (!newDirName.value.trim()) return
  const name = newDirName.value.trim(); newDirName.value = ''; showNewDir.value = null
  const newPath = (remotePath.value === '/' ? '' : remotePath.value) + '/' + name
  try {
    await invoke('remote_mkdir', { id: props.connId, path: newPath })
    setStatus(`${t('folderCreatedPrefix')}: ${name}`, 'ok'); await loadRemote(remotePath.value)
  } catch (e: any) { setStatus(String(e), 'err') }
}

async function createRemoteFile() {
  if (!newDirName.value.trim()) return
  const name = newDirName.value.trim(); newDirName.value = ''; showNewDir.value = null
  const newPath = (remotePath.value === '/' ? '' : remotePath.value) + '/' + name
  try {
    await invoke('remote_write_file', { id: props.connId, path: newPath, content: '' })
    setStatus(`${t('fileCreatedPrefix')}: ${name}`, 'ok'); await loadRemote(remotePath.value)
  } catch (e: any) { setStatus(String(e), 'err') }
}

async function createLocalDir() {
  if (!newDirName.value.trim()) return
  const name = newDirName.value.trim(); newDirName.value = ''; showNewDir.value = null
  const base = localPath.value.endsWith('/') ? localPath.value.slice(0, -1) : localPath.value
  try {
    await invoke('create_dir_cmd', { path: `${base}/${name}` })
    setStatus(`${t('folderCreatedPrefix')}: ${name}`, 'ok'); await loadLocal(localPath.value)
  } catch (e: any) { setStatus(String(e), 'err') }
}

async function createLocalFile() {
  if (!newDirName.value.trim()) return
  const name = newDirName.value.trim(); newDirName.value = ''; showNewDir.value = null
  const base = localPath.value.endsWith('/') ? localPath.value.slice(0, -1) : localPath.value
  try {
    await invoke('create_file', { path: `${base}/${name}` })
    setStatus(`${t('fileCreatedPrefix')}: ${name}`, 'ok'); await loadLocal(localPath.value)
  } catch (e: any) { setStatus(String(e), 'err') }
}

function confirmNewDirBar() {
  if (showNewDir.value === 'remote') createRemoteDir()
  else if (showNewDir.value === 'remote-file') createRemoteFile()
  else if (showNewDir.value === 'local') createLocalDir()
  else if (showNewDir.value === 'local-file') createLocalFile()
}

// ── Helpers ────────────────────────────────────────────────────────────────────
function setStatus(msg: string, type: 'ok' | 'err') {
  statusMsg.value = msg; statusType.value = type
  setTimeout(() => { statusMsg.value = ''; statusType.value = '' }, 5000)
}

function formatSize(n: number) {
  if (n === 0) return ''
  if (n < 1024) return `${n} B`
  if (n < 1048576) return `${(n / 1024).toFixed(1)} KB`
  return `${(n / 1048576).toFixed(1)} MB`
}

function fileColor(name: string) {
  const ext = name.split('.').pop()?.toLowerCase() ?? ''
  const m: Record<string, string> = {
    php:'#8892be', js:'#f7df1e', ts:'#3178c6', vue:'#42b883', html:'#e44d26',
    htm:'#e44d26', css:'#264de4', scss:'#cc6699', json:'#cbcb41', yaml:'#cbcb41',
    yml:'#cbcb41', sql:'#e38c00', py:'#3572a5', rs:'#ce422b', go:'#00add8',
    md:'#aaa', txt:'#888', jpg:'#ff79a8', jpeg:'#ff79a8', png:'#ff79a8',
    gif:'#ff79a8', svg:'#ffb86c', zip:'#bd93f9', tar:'#bd93f9', gz:'#bd93f9',
    htaccess:'#50fa7b', env:'#f1fa8c', xml:'#e37933',
    ada:'#02f88c', adb:'#02f88c', ads:'#02f88c',
    jsx:'#61dafb', tsx:'#61dafb', astro:'#ff5d01', svelte:'#ff3e00', hbs:'#f0772b', handlebars:'#f0772b',
  }
  return m[ext] ?? '#888'
}

// ── Disconnect / reconnect ─────────────────────────────────────────────────────
const { connections: ftpConns } = useFtpStore()
const reconnecting = ref(false)
async function doDisconnect() {
  if (transferring.value && !(await ask(t('fDiscAsk'), { title: t('fDiscTitle'), kind: 'warning', okLabel: t('fDiscOk'), cancelLabel: t('fStayConnected') }))) return
  queue.cancelAll()
  await disconnectFtp(props.connId)
}
async function doReconnect() {
  const c = ftpConns.value.find(x => x.id === props.connId)
  if (!c) { setStatus(t('fNoConn'), 'err'); return }
  reconnecting.value = true
  try {
    await connectFtp(c)
    isConnected.value = true
    remoteStack.value = []
    await loadRemote('/')
    setStatus(t('fReconnected', { name: c.name }), 'ok')
  } catch (e: any) { setStatus(String(e), 'err') }
  finally { reconnecting.value = false }
}

function onFtpDisconnected(e: Event) {
  if ((e as CustomEvent).detail === props.connId) isConnected.value = false
}

function onFtpConnected(e: Event) {
  if ((e as CustomEvent).detail !== props.connId) return
  isConnected.value = true
  remoteStack.value = []
  loadRemote('/')
}

onMounted(async () => {
  window.addEventListener('click', closeCtx)
  window.addEventListener('ftp-disconnected', onFtpDisconnected)
  window.addEventListener('ftp-connected', onFtpConnected)
  unlistenTransferProgress = await listen<{ done: number; total: number }>('ftp-transfer-progress', (ev) => {
    if (qs.current) queue.progress(ev.payload.done)
  })
  if (store.state.rootPath) await loadLocal(store.state.rootPath)
  await loadRemote('/')
})

onBeforeUnmount(() => {
  window.removeEventListener('click', closeCtx)
  window.removeEventListener('ftp-disconnected', onFtpDisconnected)
  window.removeEventListener('ftp-connected', onFtpConnected)
  unlistenTransferProgress?.()
})

watch(() => props.connId, () => { isConnected.value = true; loadRemote('/') })
</script>

<template>
  <div class="ftp-view" :class="{ dragging: isDragging }">

    <!-- Disconnected overlay -->
    <div v-if="!isConnected" class="disconnected-overlay">
      <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" opacity=".4">
        <path d="M18.364 5.636a9 9 0 010 12.728M15.536 8.464a5 5 0 010 7.072M6.343 17.657a9 9 0 010-12.728M9.172 14.828a5 5 0 010-7.07"/>
        <line x1="2" y1="2" x2="22" y2="22"/>
      </svg>
      <div class="disc-title">{{ t('disconnectedTitle') }}</div>
      <div class="disc-sub">{{ t('disconnectedSub1') }} "{{ connName }}" {{ t('disconnectedSub2') }}<br>{{ t('disconnectedSub3') }}</div>
      <button class="ftp-reconnect" :disabled="reconnecting" @click="doReconnect">{{ reconnecting ? t('fConnecting') : t('fReconnectNow') }}</button>
    </div>

    <!-- Right-click menu -->
    <div v-if="ctx" class="ftp-ctx" :style="{ left: ctx.x + 'px', top: ctx.y + 'px' }" @click.stop @contextmenu.prevent>
      <template v-if="ctx.entry">
        <button v-if="ctx.entry.is_dir" @click="ctxDo('open')">{{ t('fCtxOpenFolder') }}<kbd>Enter</kbd></button>
        <button @click="ctxDo('transfer')">{{ ctx.side === 'local' ? t('fCtxUpload') : t('fCtxDownload') }}<kbd>{{ ctx.entry.is_dir ? '' : 'Enter' }}</kbd></button>
        <button v-if="!ctx.entry.is_dir" @click="ctxDo('edit')">{{ t('fCtxEdit') }}<kbd>F4</kbd></button>
        <div class="ctx-sep" />
        <button @click="ctxDo('rename')">{{ t('fCtxRename') }}<kbd>F2</kbd></button>
        <button class="danger" @click="ctxDo('delete')">{{ t('fCtxDelete') }}<kbd>{{ t('fKeyDel') }}</kbd></button>
        <div class="ctx-sep" />
      </template>
      <button @click="ctxDo('mkdir')">{{ t('fCtxMkdir') }}<kbd>F7</kbd></button>
      <button @click="ctxDo('newfile')">{{ t('fCtxNewFile') }}</button>
      <button @click="ctxDo('refresh')">{{ t('fCtxRefresh') }}<kbd>F5</kbd></button>
      <button @click="ctxDo('up')">{{ t('fCtxUp') }}<kbd>{{ t('fKeyBack') }}</kbd></button>
      <button @click="ctxDo('selectall')">{{ t('fCtxSelAll') }}<kbd>Ctrl+A</kbd></button>
      <button @click="ctxDo('copypath')">{{ t('fCtxCopyPath') }}</button>
    </div>
    <div v-if="showKeys" class="ftp-keys" @click.stop>
      <b>{{ t('fShortcuts') }}</b>
      <div><kbd>↑ ↓ {{ t('fShKeys') }}</kbd> {{ t('fShMove') }}</div>
      <div><kbd>{{ t('fKeyShift') }} + ↑↓</kbd> / click + <kbd>{{ t('fKeyShift') }}</kbd> {{ t('fShRange') }}</div>
      <div><kbd>Ctrl + ↑↓</kbd> + <kbd>{{ t('fKeySpace') }}</kbd> / <kbd>Ctrl</kbd>+click {{ t('fShPick') }}</div>
      <div><kbd>Enter</kbd> <span v-html="t('fShEnter')" /></div>
      <div><kbd>{{ t('fKeyBack') }}</kbd> / <kbd>←</kbd> / <kbd>Alt+↑</kbd> {{ t('fShUp') }} · <kbd>→</kbd> {{ t('fShIn') }} · <kbd>Alt+←</kbd> {{ t('fShBackNav') }}</div>
      <div><kbd>Tab</kbd> {{ t('fShSwitch') }}</div>
      <div><kbd>Ctrl+L</kbd> {{ t('fShPath') }} · <kbd>F5</kbd> {{ t('fShRefresh') }} · <kbd>Ctrl+A</kbd> {{ t('fShAll') }}</div>
      <div><kbd>F2</kbd> {{ t('fShRename') }} · <kbd>F4</kbd> {{ t('fShEditor') }} · <kbd>F7</kbd> {{ t('fShMkdir') }} · <kbd>{{ t('fKeyDel') }}</kbd> {{ t('fShDel') }}</div>
      <div>{{ t('fShType') }} · <kbd>Esc</kbd> {{ t('fShDesel') }}</div>
    </div>

    <!-- Top bar -->
    <div class="ftp-topbar">
      <div class="ftp-conn-badge">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M9 3H5a2 2 0 00-2 2v4m6-6h10a2 2 0 012 2v4M9 3v18m0 0h10a2 2 0 002-2V9M9 21H5a2 2 0 01-2-2V9m0 0h18"/>
        </svg>
        <span class="ftp-badge-name">{{ connName }}</span>
        <span class="ftp-badge-proto">{{ protocol.toUpperCase() }}</span>
      </div>

      <div class="ftp-transfer-actions">
        <!-- Transfer progress bar — FileZilla-style: current file name, its
             own byte progress (%, MB/MB), plus the overall file count. -->
        <template v-if="uploadProgress">
          <span v-if="uploadProgress.currentName" class="upload-current-name" :title="uploadProgress.currentName">{{ uploadProgress.currentName }}</span>
          <div class="upload-progress">
            <div
              class="upload-progress-bar" :class="{ 'has-error': uploadProgress.hasError }"
              :style="{ width: filePercent + '%' }"
            />
          </div>
          <span class="upload-percent" :class="{ 'has-error': uploadProgress.hasError }">{{ filePercent }}%</span>
          <span v-if="byteLabel" class="upload-bytes">{{ byteLabel }}</span>
          <span class="upload-count" :class="{ 'has-error': uploadProgress.hasError }">{{ uploadProgress.done }}/{{ uploadProgress.total }}</span>
          <span v-if="speedLabel" class="upload-bytes">{{ speedLabel }}</span>
          <button class="ftp-cancel-btn" @click="qs.paused ? queue.resume() : queue.pause()" :title="qs.paused ? t('fResumeTip') : t('fPauseTip')">{{ qs.paused ? t('fResume') : t('fPause') }}</button>
          <button class="ftp-cancel-btn" @click="cancelActiveTransfer" :title="t('cancelTransferTitle')">{{ t('cancel') }}</button>
        </template>
        <template v-else>
          <button
            class="ftp-xfer-btn upload"
            @click="uploadSelected"
            :disabled="localSelected.size === 0 || transferring"
            :title="`${t('uploadedPrefix')} ${localSelected.size} ${t('itemsWord')}`"
          >
            <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
              <path d="M.5 9.9a.5.5 0 01.5.5v2.5a1 1 0 001 1h12a1 1 0 001-1v-2.5a.5.5 0 011 0v2.5a2 2 0 01-2 2H2a2 2 0 01-2-2v-2.5a.5.5 0 01.5-.5z"/>
              <path d="M7.646 1.146a.5.5 0 01.708 0l3 3a.5.5 0 01-.708.708L8.5 2.707V11.5a.5.5 0 01-1 0V2.707L5.354 4.854a.5.5 0 11-.708-.708l3-3z"/>
            </svg>
            {{ t('uploadedPrefix') }}{{ localSelected.size > 0 ? ` (${localSelected.size})` : '' }}
          </button>
          <button
            class="ftp-xfer-btn download"
            @click="downloadSelected"
            :disabled="remoteSelected.size === 0 || transferring"
            :title="`${t('downloadedPrefix')} ${remoteSelected.size} ${t('filesWord2')}`"
          >
            <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
              <path d="M.5 9.9a.5.5 0 01.5.5v2.5a1 1 0 001 1h12a1 1 0 001-1v-2.5a.5.5 0 011 0v2.5a2 2 0 01-2 2H2a2 2 0 01-2-2v-2.5a.5.5 0 01.5-.5z"/>
              <path d="M7.646 10.854a.5.5 0 00.708 0l3-3a.5.5 0 00-.708-.708L8.5 9.293V1.5a.5.5 0 00-1 0v7.793L5.354 7.146a.5.5 0 10-.708.708l3 3z"/>
            </svg>
            {{ t('downloadedPrefix') }}{{ remoteSelected.size > 0 ? ` (${remoteSelected.size})` : '' }}
          </button>
        </template>
        <span v-if="transferring && !uploadProgress" class="xfer-spin" />
      </div>

      <button class="ftp-xfer-btn" @click="toggleDblAction" :title="dblAction === 'transfer' ? t('fDblTransferTip') : t('fDblEditTip')">
        {{ dblAction === 'transfer' ? t('fDblTransfer') : t('fDblEdit') }}
      </button>
      <button class="ftp-xfer-btn" :disabled="!isConnected" @click.stop="syncOpen = true; syncPlan = null" :title="t('ySyncTip')">{{ t('ySyncBtn') }}</button>
      <button class="ftp-xfer-btn" @click.stop="showKeys = !showKeys" :title="t('fShortcutsTip')">{{ t('fShortcutsBtn') }}</button>
      <button v-if="isConnected" class="ftp-xfer-btn ftp-disc" @click="doDisconnect" :title="t('fDiscTip')">{{ t('fDiscBtn') }}</button>
      <button v-else class="ftp-xfer-btn ftp-recon" :disabled="reconnecting" @click="doReconnect" :title="t('fReconTip')">{{ t('fReconBtn') }}</button>
      <div class="ftp-status" v-if="statusMsg" :class="statusType">{{ statusMsg }}</div>
    </div>

    <!-- Dual pane -->
    <div class="ftp-main">

      <!-- ── LOCAL panel ── -->
      <div class="ftp-pane" :style="{ width: splitPos + '%' }">
        <div class="ftp-pane-header">
          <div class="pane-label">{{ t('localLabel') }}</div>
          <div class="pane-nav">
            <button class="pane-nav-btn" @click="localBack" :disabled="localStack.length === 0" :title="t('backTitle')">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M11.354 1.646a.5.5 0 010 .708L5.707 8l5.647 5.646a.5.5 0 01-.708.708l-6-6a.5.5 0 010-.708l6-6a.5.5 0 01.708 0z"/></svg>
            </button>
            <button class="pane-nav-btn" @click="localUp" :title="t('parentFolderTitle')">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M8 15a.5.5 0 00.5-.5V2.707l3.146 3.147a.5.5 0 00.708-.708l-4-4a.5.5 0 00-.708 0l-4 4a.5.5 0 10.708.708L7.5 2.707V14.5a.5.5 0 00.5.5z"/></svg>
            </button>
            <button class="pane-nav-btn" @click="localHome" :title="t('projectRootTitle')">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M8.354 1.146a.5.5 0 00-.708 0l-6 6A.5.5 0 002 8v7a.5.5 0 00.5.5h4a.5.5 0 00.5-.5v-4h2v4a.5.5 0 00.5.5h4a.5.5 0 00.5-.5V8a.5.5 0 00-.146-.354L13 6.5V2a.5.5 0 00-.5-.5h-1a.5.5 0 00-.5.5v1.293L8.354 1.146z"/></svg>
            </button>
            <button class="pane-nav-btn" @click="loadLocal(localPath)" :title="t('reload')">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M8 3a5 5 0 104.546 2.914.5.5 0 01.908-.417A6 6 0 118 2v1z"/><path d="M8 4.466V.534a.25.25 0 01.41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 018 4.466z"/></svg>
            </button>
            <button class="pane-nav-btn" @click="showNewDir = 'local'; newDirName = ''" :title="t('newLocalFolderTitle')">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
                <path d="M1.75 1A1.75 1.75 0 000 2.75v10.5C0 14.22.784 15 1.75 15h12.5A1.75 1.75 0 0016 13.25v-8.5A1.75 1.75 0 0014.25 3H7.5L6.34 1.5A1.75 1.75 0 004.99 1H1.75z"/>
                <path d="M8 6.5a.5.5 0 011 0V8h1.5a.5.5 0 010 1H9v1.5a.5.5 0 01-1 0V9H6.5a.5.5 0 010-1H8V6.5z" fill="#0f1012"/>
              </svg>
            </button>
            <button class="pane-nav-btn" @click="showNewDir = 'local-file'; newDirName = ''" :title="t('newLocalFileTitle')">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
                <path d="M9.293 0H2a1 1 0 00-1 1v14a1 1 0 001 1h12a1 1 0 001-1V4.707A1 1 0 0014.707 4L10 -0.707A1 1 0 009.293 0zM9.5 3.5V1l4 4h-3a1 1 0 01-1-1.5z"/>
                <path d="M8 8.5a.5.5 0 011 0V10h1.5a.5.5 0 010 1H9v1.5a.5.5 0 01-1 0V11H6.5a.5.5 0 010-1H8V8.5z" fill="#0f1012"/>
              </svg>
            </button>
          </div>
          <div class="pane-summary" :title="localSummary">{{ localSummary }}</div>
          <input v-if="pathEdit?.side === 'local'" :ref="el => (pathInputEl = el as HTMLInputElement)" v-model="pathEdit.text" class="path-input"
            @keydown.enter.prevent="commitPathEdit" @keydown.esc.prevent="cancelPathEdit" @blur="cancelPathEdit" spellcheck="false" />
          <div v-else class="pane-breadcrumb" @dblclick.self="startPathEdit('local')" :title="t('fPathTip')">
            <button v-for="(c, i) in localBreadcrumbs" :key="i"
              class="crumb-btn" :class="{ active: i === localBreadcrumbs.length - 1 }"
              @click="goToLocal(c.path)">{{ c.label }}</button>
            <button class="crumb-edit" @click="startPathEdit('local')" :title="t('fPathEdit')">✎</button>
          </div>
        </div>

        <div v-if="showNewDir === 'local' || showNewDir === 'local-file'" class="new-dir-bar">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" style="color:#dcb67a"><path d="M1.75 1A1.75 1.75 0 000 2.75v10.5C0 14.22.784 15 1.75 15h12.5A1.75 1.75 0 0016 13.25v-8.5A1.75 1.75 0 0014.25 3H7.5L6.34 1.5A1.75 1.75 0 004.99 1H1.75z"/></svg>
          <input v-model="newDirName" class="new-dir-input" :placeholder="showNewDir === 'local' ? t('newFolderPlaceholder') : t('newFilePlaceholder')"
            @keydown.enter="confirmNewDirBar" @keydown.escape="showNewDir = null" autofocus />
          <button class="new-dir-ok" @click="confirmNewDirBar">OK</button>
          <button class="new-dir-cancel" @click="showNewDir = null">×</button>
        </div>


        <div
          ref="localBodyEl"
          class="pane-body" :class="{ 'pane-body--split': localPreview, 'drop-hover': dropHover === 'local' }"
          @dragover="onDragOver('local', $event)" @dragleave="dropHover = null" @drop.prevent="onDrop('local')"
          tabindex="0"
          @keydown="onLocalKeydown"
          @contextmenu="openCtx('local', null, $event)"
          @focus="localFocusIdx < 0 && localItems.length ? localFocusIdx = 0 : null"
        >
          <div v-if="localLoading" class="pane-loading"><span class="mini-spin" /> {{ t('loading') }}</div>
          <template v-else>
            <div
              v-for="(e, i) in localItems" :key="e.path"
              class="pane-row" draggable="true"
              @dragstart="onDragStart('local', e, $event)"
              :class="{ selected: localSelected.has(e.path), is_dir: e.is_dir, focused: i === localFocusIdx }"
              @click="localFocusIdx = i; localClick(e, $event)"
              @dblclick="localDblclick(e)"
              @contextmenu="openCtx('local', e, $event)"
            >
              <template v-if="localRenaming?.entry.path === e.path">
                <input v-model="localRenaming.name" class="rename-input"
                  @keydown.enter="confirmLocalRename" @keydown.escape="localRenaming = null" @click.stop autofocus />
                <button class="rename-ok" @click.stop="confirmLocalRename">✓</button>
                <button class="rename-cancel" @click.stop="localRenaming = null">×</button>
              </template>
              <template v-else>
                <span class="row-check" :class="{ checked: localSelected.has(e.path) }" @click.stop="localFocusIdx = i; toggleLocalCheck(e)">
                  <svg v-if="localSelected.has(e.path)" width="9" height="9" viewBox="0 0 12 12" fill="currentColor"><path d="M10.28 2.28L4 8.56l-2.28-2.28a1 1 0 00-1.44 1.44l3 3a1 1 0 001.44 0l7-7a1 1 0 00-1.44-1.44z"/></svg>
                </span>
                <svg v-if="e.is_dir" class="row-icon" width="13" height="13" viewBox="0 0 16 16" fill="currentColor" style="color:#dcb67a">
                  <path d="M1.75 1A1.75 1.75 0 000 2.75v10.5C0 14.22.784 15 1.75 15h12.5A1.75 1.75 0 0016 13.25v-8.5A1.75 1.75 0 0014.25 3H7.5L6.34 1.5A1.75 1.75 0 004.99 1H1.75z"/>
                </svg>
                <svg v-else class="row-icon" width="11" height="13" viewBox="0 0 16 16" fill="currentColor" :style="{ color: fileColor(e.name) }">
                  <path d="M14 4.5V14a2 2 0 01-2 2H4a2 2 0 01-2-2V2a2 2 0 012-2h5.5L14 4.5zm-3 0A1.5 1.5 0 019.5 3V1H4a1 1 0 00-1 1v12a1 1 0 001 1h8a1 1 0 001-1V4.5h-2z"/>
                </svg>
                <span class="row-name">{{ e.name }}</span>
                <span class="row-modified">{{ e.modified }}</span>
                <span class="row-size">{{ formatSize(e.size) }}</span>
                <div class="row-actions">
                  <button v-if="!e.is_dir" class="row-act-btn" @click.stop="openLocalPreview(e)" :title="t('viewEditTitle')">
                    <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M16 8s-3-5.5-8-5.5S0 8 0 8s3 5.5 8 5.5S16 8 16 8zM1.173 8a13.133 13.133 0 011.66-2.043C4.12 4.668 5.88 3.5 8 3.5c2.12 0 3.879 1.168 5.168 2.457A13.133 13.133 0 0114.828 8c-.058.087-.122.183-.195.288-.335.48-.83 1.12-1.465 1.755C11.879 11.332 10.119 12.5 8 12.5c-2.12 0-3.879-1.168-5.168-2.457A13.144 13.144 0 011.172 8z"/><circle cx="8" cy="8" r="2"/></svg>
                  </button>
                  <button class="row-act-btn" @click.stop="startLocalRename(e)" :title="t('rename')">
                    <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M12.854.146a.5.5 0 00-.707 0L10.5 1.793 14.207 5.5l1.647-1.646a.5.5 0 000-.708l-3-3zm.646 6.061L9.793 2.5 3.293 9H3.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.207l6.5-6.5zm-7.468 7.468A.5.5 0 015.5 14H5v-.5a.5.5 0 01-.5-.5H4v-.5a.5.5 0 01-.5-.5H3v-.5a.5.5 0 01-.5-.5H2.5v-.5a.5.5 0 01.646-.473l.853.214 1.832-1.832 3.709 3.709-1.832 1.832.214.853z"/></svg>
                  </button>
                  <button class="row-act-btn del" @click.stop="localDelete(e)" :title="t('delete')">
                    <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M5.5 5.5A.5.5 0 016 6v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm2.5 0a.5.5 0 01.5.5v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm3 .5a.5.5 0 00-1 0v6a.5.5 0 001 0V6z"/><path fill-rule="evenodd" d="M14.5 3a1 1 0 01-1 1H13v9a2 2 0 01-2 2H5a2 2 0 01-2-2V4h-.5a1 1 0 010-2h4a1 1 0 011-1h2a1 1 0 011 1h4a1 1 0 011 1zM4.118 4L4 4.059V13a1 1 0 001 1h6a1 1 0 001-1V4.059L11.882 4H4.118zM2.5 3a.5.5 0 000 1h11a.5.5 0 000-1h-11z"/></svg>
                  </button>
                </div>
              </template>
            </div>
          </template>
        </div>

        <!-- Local file preview / editor -->
        <div v-if="localPreview && localPreviewEntry" class="file-preview">
          <div class="preview-header">
            <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" :style="{ color: fileColor(localPreviewEntry.name) }">
              <path d="M14 4.5V14a2 2 0 01-2 2H4a2 2 0 01-2-2V2a2 2 0 012-2h5.5L14 4.5zm-3 0A1.5 1.5 0 019.5 3V1H4a1 1 0 00-1 1v12a1 1 0 001 1h8a1 1 0 001-1V4.5h-2z"/>
            </svg>
            <span class="preview-filename">{{ localPreviewEntry.name }}</span>
            <div style="flex:1"/>
            <button class="preview-save-btn" @click="saveLocal">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M2 1a1 1 0 00-1 1v12a1 1 0 001 1h12a1 1 0 001-1V4.5a.5.5 0 00-.146-.354l-3-3A.5.5 0 0011.5 1H2zm5 1h1v3.5a.5.5 0 01-.5.5h-4a.5.5 0 01-.5-.5V2h4zm-2 8a2 2 0 114 0 2 2 0 01-4 0z"/></svg>
              {{ t('save') }}
            </button>
            <button class="preview-close-btn" @click="localPreview = false; localContent = null">×</button>
          </div>
          <div v-if="localContent === null" class="preview-loading"><span class="mini-spin" /> {{ t('loading') }}</div>
          <textarea v-else v-model="localContent" class="preview-textarea" spellcheck="false" />
        </div>
      </div>

      <!-- Splitter -->
      <div class="ftp-splitter" @mousedown="onSplitMousedown"><div class="splitter-handle" /></div>

      <!-- ── REMOTE panel ── -->
      <div class="ftp-pane ftp-pane-remote" :style="{ width: (100 - splitPos) + '%' }">
        <div class="ftp-pane-header">
          <div class="pane-label remote-label">{{ t('serverLabel') }}</div>
          <div class="pane-nav">
            <button class="pane-nav-btn" @click="remoteBack" :disabled="remoteStack.length === 0" :title="t('backTitle')">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M11.354 1.646a.5.5 0 010 .708L5.707 8l5.647 5.646a.5.5 0 01-.708.708l-6-6a.5.5 0 010-.708l6-6a.5.5 0 01.708 0z"/></svg>
            </button>
            <button class="pane-nav-btn" @click="goUp('remote')" :disabled="remotePath === '/'" :title="t('parentFolderTitle') + ' (' + t('fKeyBack') + ')'">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M8 15a.5.5 0 00.5-.5V2.707l3.146 3.147a.5.5 0 00.708-.708l-4-4a.5.5 0 00-.708 0l-4 4a.5.5 0 10.708.708L7.5 2.707V14.5a.5.5 0 00.5.5z"/></svg>
            </button>
            <button class="pane-nav-btn" @click="loadRemote('/')" :title="t('serverRootTitle')">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M8.354 1.146a.5.5 0 00-.708 0l-6 6A.5.5 0 002 8v7a.5.5 0 00.5.5h4a.5.5 0 00.5-.5v-4h2v4a.5.5 0 00.5.5h4a.5.5 0 00.5-.5V8a.5.5 0 00-.146-.354L13 6.5V2a.5.5 0 00-.5-.5h-1a.5.5 0 00-.5.5v1.293L8.354 1.146z"/></svg>
            </button>
            <button class="pane-nav-btn" @click="loadRemote(remotePath)" :title="t('reload')">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M8 3a5 5 0 104.546 2.914.5.5 0 01.908-.417A6 6 0 118 2v1z"/><path d="M8 4.466V.534a.25.25 0 01.41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 018 4.466z"/></svg>
            </button>
            <button class="pane-nav-btn" @click="showNewDir = 'remote'; newDirName = ''" :title="t('newRemoteFolderTitle')">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
                <path d="M1.75 1A1.75 1.75 0 000 2.75v10.5C0 14.22.784 15 1.75 15h12.5A1.75 1.75 0 0016 13.25v-8.5A1.75 1.75 0 0014.25 3H7.5L6.34 1.5A1.75 1.75 0 004.99 1H1.75z"/>
                <path d="M8 6.5a.5.5 0 011 0V8h1.5a.5.5 0 010 1H9v1.5a.5.5 0 01-1 0V9H6.5a.5.5 0 010-1H8V6.5z" fill="#0f1012"/>
              </svg>
            </button>
            <button class="pane-nav-btn" @click="showNewDir = 'remote-file'; newDirName = ''" :title="t('newRemoteFileTitle')">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
                <path d="M9.293 0H2a1 1 0 00-1 1v14a1 1 0 001 1h12a1 1 0 001-1V4.707A1 1 0 0014.707 4L10 -0.707A1 1 0 009.293 0zM9.5 3.5V1l4 4h-3a1 1 0 01-1-1.5z"/>
                <path d="M8 8.5a.5.5 0 011 0V10h1.5a.5.5 0 010 1H9v1.5a.5.5 0 01-1 0V11H6.5a.5.5 0 010-1H8V8.5z" fill="#0f1012"/>
              </svg>
            </button>
          </div>
          <div class="pane-summary" :title="remoteSummary">{{ remoteSummary }}</div>
          <input v-if="pathEdit?.side === 'remote'" :ref="el => (pathInputEl = el as HTMLInputElement)" v-model="pathEdit.text" class="path-input"
            @keydown.enter.prevent="commitPathEdit" @keydown.esc.prevent="cancelPathEdit" @blur="cancelPathEdit" spellcheck="false" />
          <div v-else class="pane-breadcrumb" @dblclick.self="startPathEdit('remote')" :title="t('fPathTip')">
            <button v-for="(c, i) in remoteBreadcrumbs" :key="i"
              class="crumb-btn" :class="{ active: i === remoteBreadcrumbs.length - 1 }"
              @click="loadRemote(c.path)">{{ c.label }}</button>
            <button class="crumb-edit" @click="startPathEdit('remote')" :title="t('fPathEdit')">✎</button>
          </div>
        </div>

        <div v-if="showNewDir === 'remote' || showNewDir === 'remote-file'" class="new-dir-bar">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" style="color:#dcb67a"><path d="M1.75 1A1.75 1.75 0 000 2.75v10.5C0 14.22.784 15 1.75 15h12.5A1.75 1.75 0 0016 13.25v-8.5A1.75 1.75 0 0014.25 3H7.5L6.34 1.5A1.75 1.75 0 004.99 1H1.75z"/></svg>
          <input v-model="newDirName" class="new-dir-input" :placeholder="showNewDir === 'remote' ? t('newFolderPlaceholder') : t('newFilePlaceholder')"
            @keydown.enter="confirmNewDirBar" @keydown.escape="showNewDir = null" autofocus />
          <button class="new-dir-ok" @click="confirmNewDirBar">OK</button>
          <button class="new-dir-cancel" @click="showNewDir = null">×</button>
        </div>

        <div
          ref="remoteBodyEl"
          class="pane-body" :class="{ 'pane-body--split': remotePreview, 'drop-hover': dropHover === 'remote' }"
          @dragover="onDragOver('remote', $event)" @dragleave="dropHover = null" @drop.prevent="onDrop('remote')"
          tabindex="0"
          @keydown="onRemoteKeydown"
          @contextmenu="openCtx('remote', null, $event)"
          @focus="remoteFocusIdx < 0 && remoteItems.length ? remoteFocusIdx = 0 : null"
        >
          <div v-if="remoteLoading" class="pane-loading"><span class="mini-spin" /> {{ t('loading') }}</div>
          <div v-else-if="remoteError" class="pane-error">{{ remoteError }}</div>
          <template v-else>
            <div
              v-for="(e, i) in remoteItems" :key="e.path"
              class="pane-row" draggable="true"
              @dragstart="onDragStart('remote', e, $event)"
              :class="{ selected: remoteSelected.has(e.path), is_dir: e.is_dir, focused: i === remoteFocusIdx }"
              @click="remoteFocusIdx = i; remoteClick(e, $event)"
              @dblclick="remoteDblclick(e)"
              @contextmenu="openCtx('remote', e, $event)"
            >
              <template v-if="renaming?.entry.path === e.path">
                <input v-model="renaming.name" class="rename-input"
                  @keydown.enter="confirmRename" @keydown.escape="renaming = null" @click.stop autofocus />
                <button class="rename-ok" @click.stop="confirmRename">✓</button>
                <button class="rename-cancel" @click.stop="renaming = null">×</button>
              </template>
              <template v-else>
                <span class="row-check" :class="{ checked: remoteSelected.has(e.path) }" @click.stop="remoteFocusIdx = i; toggleRemoteCheck(e)">
                  <svg v-if="remoteSelected.has(e.path)" width="9" height="9" viewBox="0 0 12 12" fill="currentColor"><path d="M10.28 2.28L4 8.56l-2.28-2.28a1 1 0 00-1.44 1.44l3 3a1 1 0 001.44 0l7-7a1 1 0 00-1.44-1.44z"/></svg>
                </span>
                <svg v-if="e.is_dir" class="row-icon" width="13" height="13" viewBox="0 0 16 16" fill="currentColor" style="color:#dcb67a">
                  <path d="M1.75 1A1.75 1.75 0 000 2.75v10.5C0 14.22.784 15 1.75 15h12.5A1.75 1.75 0 0016 13.25v-8.5A1.75 1.75 0 0014.25 3H7.5L6.34 1.5A1.75 1.75 0 004.99 1H1.75z"/>
                </svg>
                <svg v-else class="row-icon" width="11" height="13" viewBox="0 0 16 16" fill="currentColor" :style="{ color: fileColor(e.name) }">
                  <path d="M14 4.5V14a2 2 0 01-2 2H4a2 2 0 01-2-2V2a2 2 0 012-2h5.5L14 4.5zm-3 0A1.5 1.5 0 019.5 3V1H4a1 1 0 00-1 1v12a1 1 0 001 1h8a1 1 0 001-1V4.5h-2z"/>
                </svg>
                <span class="row-name">{{ e.name }}</span>
                <span class="row-modified">{{ e.modified }}</span>
                <span class="row-size">{{ formatSize(e.size) }}</span>
                <div class="row-actions">
                  <button v-if="!e.is_dir" class="row-act-btn" @click.stop="previewRemote(e)" :title="t('viewEditTitle')">
                    <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M16 8s-3-5.5-8-5.5S0 8 0 8s3 5.5 8 5.5S16 8 16 8zM1.173 8a13.133 13.133 0 011.66-2.043C4.12 4.668 5.88 3.5 8 3.5c2.12 0 3.879 1.168 5.168 2.457A13.133 13.133 0 0114.828 8c-.058.087-.122.183-.195.288-.335.48-.83 1.12-1.465 1.755C11.879 11.332 10.119 12.5 8 12.5c-2.12 0-3.879-1.168-5.168-2.457A13.144 13.144 0 011.172 8z"/><circle cx="8" cy="8" r="2"/></svg>
                  </button>
                  <button class="row-act-btn" @click.stop="startRename(e)" :title="t('rename')">
                    <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M12.854.146a.5.5 0 00-.707 0L10.5 1.793 14.207 5.5l1.647-1.646a.5.5 0 000-.708l-3-3zm.646 6.061L9.793 2.5 3.293 9H3.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.207l6.5-6.5zm-7.468 7.468A.5.5 0 015.5 14H5v-.5a.5.5 0 01-.5-.5H4v-.5a.5.5 0 01-.5-.5H3v-.5a.5.5 0 01-.5-.5H2.5v-.5a.5.5 0 01.646-.473l.853.214 1.832-1.832 3.709 3.709-1.832 1.832.214.853z"/></svg>
                  </button>
                  <button class="row-act-btn del" @click.stop="remoteDelete(e)" :title="t('delete')">
                    <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M5.5 5.5A.5.5 0 016 6v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm2.5 0a.5.5 0 01.5.5v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm3 .5a.5.5 0 00-1 0v6a.5.5 0 001 0V6z"/><path fill-rule="evenodd" d="M14.5 3a1 1 0 01-1 1H13v9a2 2 0 01-2 2H5a2 2 0 01-2-2V4h-.5a1 1 0 010-2h4a1 1 0 011-1h2a1 1 0 011 1h4a1 1 0 011 1zM4.118 4L4 4.059V13a1 1 0 001 1h6a1 1 0 001-1V4.059L11.882 4H4.118zM2.5 3a.5.5 0 000 1h11a.5.5 0 000-1h-11z"/></svg>
                  </button>
                </div>
              </template>
            </div>
          </template>
        </div>

        <!-- Remote file preview / editor -->
        <div v-if="remotePreview && remotePreviewEntry" class="file-preview">
          <div class="preview-header">
            <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" :style="{ color: fileColor(remotePreviewEntry.name) }">
              <path d="M14 4.5V14a2 2 0 01-2 2H4a2 2 0 01-2-2V2a2 2 0 012-2h5.5L14 4.5zm-3 0A1.5 1.5 0 019.5 3V1H4a1 1 0 00-1 1v12a1 1 0 001 1h8a1 1 0 001-1V4.5h-2z"/>
            </svg>
            <span class="preview-filename">{{ remotePreviewEntry.name }}</span>
            <span class="preview-size">{{ formatSize(remotePreviewEntry.size) }}</span>
            <div style="flex:1"/>
            <button class="preview-save-btn" @click="saveRemote">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M2 1a1 1 0 00-1 1v12a1 1 0 001 1h12a1 1 0 001-1V4.5a.5.5 0 00-.146-.354l-3-3A.5.5 0 0011.5 1H2zm5 1h1v3.5a.5.5 0 01-.5.5h-4a.5.5 0 01-.5-.5V2h4zm-2 8a2 2 0 114 0 2 2 0 01-4 0z"/></svg>
              {{ t('saveToServerBtn') }}
            </button>
            <button class="preview-close-btn" @click="remotePreview = false; remoteContent = null">×</button>
          </div>
          <div v-if="remoteContent === null" class="preview-loading"><span class="mini-spin" /> {{ t('loading') }}</div>
          <textarea v-else v-model="remoteContent" class="preview-textarea" spellcheck="false" />
        </div>
      </div>
    </div>

    <!-- Synchronize the two open folders -->
    <div v-if="syncOpen" class="ow-overlay" @keydown.esc="syncOpen = false" tabindex="-1">
      <div class="ow-box sy-box" role="dialog">
        <div class="ow-title">{{ t('ySyncTitle') }}</div>
        <div class="sy-paths" :title="localPath + ' ⇄ ' + remotePath">{{ t('ySyncFolders', { local: localPath, remote: remotePath }) }}</div>
        <label class="sy-opt"><input type="radio" value="up" v-model="syncDir" @change="syncPlan = null" /> {{ t('ySyncUp') }}</label>
        <label class="sy-opt"><input type="radio" value="down" v-model="syncDir" @change="syncPlan = null" /> {{ t('ySyncDown') }}</label>
        <label v-if="syncDir === 'up'" class="sy-opt sy-danger"><input type="checkbox" v-model="syncDelete" @change="syncPlan = null" /> {{ t('ySyncDelete') }}</label>
        <div class="sy-opt"><span>{{ t('ySyncIgnore') }}</span><input v-model="syncIgnore" class="sy-ignore" :title="t('ySyncIgnoreTip')" spellcheck="false" @input="syncPlan = null" /></div>
        <div class="ow-actions" style="margin:8px 0">
          <button v-if="!syncBusy" class="ow-btn rec" @click="syncCompare">{{ t('ySyncCompare') }}</button>
          <button v-else class="ow-btn" @click="syncStop = true">{{ t('ySyncStop') }}</button>
          <span v-if="syncBusy" class="sy-busy"><span class="mini-spin" /> {{ syncBusy }}</span>
        </div>
        <template v-if="syncPlan && syncCounts">
          <div class="sy-sum">{{ t('ySyncSummary', syncCounts) }}</div>
          <div v-if="!syncRows.length" class="sy-ok">{{ t('ySyncInSync') }}</div>
          <div v-else class="ow-list">
            <div v-for="r in syncRows.slice(0, 200)" :key="r.tag + r.rel" class="ow-row">
              <span class="ow-name" :title="r.rel">{{ r.rel }}</span>
              <span class="ow-sizes">{{ formatSize(r.size) || '0 B' }}</span>
              <span class="ow-tag" :class="r.tag === 'new' ? 'same' : r.tag === 'changed' ? 'diff' : 'rm'">{{ r.tag === 'new' ? t('ySyncNew') : r.tag === 'changed' ? t('ySyncChanged') : t('ySyncRemove') }}</span>
            </div>
            <div v-if="syncRows.length > 200" class="ow-more">{{ t('ySyncMore', { n: syncRows.length - 200 }) }}</div>
          </div>
        </template>
        <div class="ow-note">{{ t('ySyncMkdirNote') }}</div>
        <div class="ow-actions" style="margin-top:8px">
          <button v-if="syncPlan && syncRows.length" class="ow-btn rec" @click="syncApply">{{ t('ySyncApply', { n: syncRows.length }) }}</button>
          <button class="ow-btn cancel" @click="syncOpen = false; syncStop = true">{{ t('ySyncClose') }}</button>
        </div>
      </div>
    </div>

    <!-- A file with this name already exists -->
    <div v-if="overwriteAsk" class="ow-overlay" @keydown.esc="answerOverwrite(null)" tabindex="-1">
      <div class="ow-box" role="dialog">
        <div class="ow-title">⚠ {{ overwriteAsk.conflicts.length === 1 ? t('fOwOne') : t('fOwMany', { n: overwriteAsk.conflicts.length }) }} {{ overwriteAsk.verb }}</div>
        <div class="ow-list">
          <div v-for="c in overwriteAsk.conflicts.slice(0, 8)" :key="c.file.dst" class="ow-row">
            <span class="ow-name" :title="c.file.dst">{{ nameOf(c.file.dst) }}</span>
            <span class="ow-sizes">{{ t('fOwSizes', { a: formatSize(c.file.size) || '0 B', b: formatSize(c.dstSize) || '0 B' }) }}</span>
            <span class="ow-tag" :class="c.sameSize ? 'same' : 'diff'">{{ c.sameSize ? t('fOwSame') : t('fOwDiff') }}</span>
          </div>
          <div v-if="overwriteAsk.conflicts.length > 8" class="ow-more">{{ t('fOwMore', { n: overwriteAsk.conflicts.length - 8 }) }}</div>
        </div>
        <div class="ow-actions">
          <button class="ow-btn rec" autofocus @click="answerOverwrite('different')" :title="t('fOwDifferentTip')">{{ t('fOwDifferent') }}</button>
          <button class="ow-btn" @click="answerOverwrite('overwrite')">{{ t('fOwAll') }}</button>
          <button class="ow-btn" @click="answerOverwrite('skip')">{{ t('fOwSkip') }}</button>
          <button class="ow-btn" @click="answerOverwrite('rename')" :title="t('fOwRenameTip')">{{ t('fOwRename') }}</button>
          <button class="ow-btn cancel" @click="answerOverwrite(null)">{{ t('fOwCancel') }}</button>
        </div>
        <div class="ow-note">Se aplica a los {{ overwriteAsk.total }} archivos de esta tanda. Los que no existen se transfieren siempre.</div>
      </div>
    </div>

    <!-- Transfer queue (FileZilla style) -->
    <div class="ftp-queue" :class="{ collapsed: !queueOpen }">
      <div class="q-head">
        <button class="q-toggle" @click="queueOpen = !queueOpen" :title="queueOpen ? t('fQHide') : t('fQShow')">{{ queueOpen ? '▾' : '▸' }} {{ t('fQTitle') }}</button>
        <div class="q-tabs">
          <button :class="{ on: queueTab === 'queued' }" @click="queueTab = 'queued'; queueOpen = true">{{ t('fQTabQueued', { n: qCounts.queued + qCounts.active }) }}</button>
          <button :class="{ on: queueTab === 'error', bad: qCounts.error + qCounts.canceled > 0 }" @click="queueTab = 'error'; queueOpen = true">{{ t('fQTabFailed', { n: qCounts.error + qCounts.canceled }) }}</button>
          <button :class="{ on: queueTab === 'done' }" @click="queueTab = 'done'; queueOpen = true">{{ t('fQTabDone', { n: qCounts.done }) }}</button>
        </div>
        <div class="q-spacer" />
        <span v-if="qs.running && qBytes.total" class="q-total">{{ formatSize(qBytes.sent) || '0 B' }} / {{ formatSize(qBytes.total) }}</span>
        <button class="q-btn" :disabled="!qs.running && !qs.paused" @click="qs.paused ? queue.resume() : queue.pause()">{{ qs.paused ? t('fResume') : t('fPause') }}</button>
        <button class="q-btn" :disabled="qCounts.queued === 0" @click="queue.cancelAll()" :title="t('fQCancelTip')">{{ t('fQCancel') }}</button>
        <button class="q-btn" :disabled="qCounts.error + qCounts.canceled === 0" @click="queue.retryFailed()">{{ t('fQRetry') }}</button>
        <button class="q-btn" :disabled="qCounts.done === 0" @click="queue.clearFinished()">{{ t('fQClear') }}</button>
      </div>
      <div v-if="queueOpen" class="q-body">
        <div v-if="queueRows.length === 0" class="q-empty">{{ queueTab === 'queued' ? t('fQEmpty') : queueTab === 'error' ? t('fQNoFail') : t('fQNoDone') }}</div>
        <div v-for="i in queueRows" :key="i.id" class="q-row" :class="i.status">
          <span class="q-dir" :class="i.dir">{{ dirIcon(i) }}</span>
          <span class="q-name" :title="i.local + '  ⇄  ' + i.remote">{{ i.name }}</span>
          <span class="q-size">{{ formatSize(i.size) || '—' }}</span>
          <span class="q-state">
            <template v-if="i.status === 'active'"><span class="q-bar"><i :style="{ width: Math.round((i.sent / Math.max(1, i.size)) * 100) + '%' }" /></span>{{ Math.round((i.sent / Math.max(1, i.size)) * 100) }}%</template>
            <template v-else-if="i.status === 'queued'">{{ i.tries ? t('fQRetrying') : t('fQWaiting') }}</template>
            <template v-else-if="i.status === 'done'">✔ {{ i.ms ? (i.ms / 1000).toFixed(1) + ' s' : '' }}</template>
            <template v-else-if="i.status === 'canceled'">{{ t('fQCanceled') }}</template>
            <span v-else class="q-err" :title="i.error">✖ {{ i.error }}</span>
          </span>
          <button v-if="i.status !== 'active'" class="q-x" @click="queue.remove(i.id)" :title="t('fQRemove')">✕</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ftp-topbar { flex-wrap: wrap; row-gap: 4px; }
.ftp-xfer-btn { white-space: nowrap; }
.ow-overlay { position:absolute; inset:0; z-index:300; background:rgba(0,0,0,.55); display:flex; align-items:center; justify-content:center; outline:none; }
.ow-box { width:560px; max-width:92%; background:var(--bg-dark,#141416); border:1px solid var(--border); border-radius:10px; box-shadow:0 16px 48px rgba(0,0,0,.6); padding:14px 16px; }
.ow-title { font-size:13px; font-weight:700; margin-bottom:8px; color:var(--fg-bright); }
.ow-list { border:1px solid var(--border); border-radius:6px; max-height:190px; overflow:auto; margin-bottom:10px; }
.ow-row { display:flex; align-items:center; gap:10px; padding:4px 10px; font-size:12px; border-bottom:1px solid #ffffff08; } .ow-name { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; } .ow-sizes { color:var(--fg-muted); font-size:11px; }
.ow-tag { font-size:10px; padding:1px 7px; border-radius:10px; border:1px solid var(--border); } .ow-tag.same { color:#3fb950; border-color:#3fb95055; } .ow-tag.diff { color:#f0883e; border-color:#f0883e55; } .ow-more { padding:4px 10px; font-size:11px; color:var(--fg-muted); }
.ow-actions { display:flex; flex-wrap:wrap; gap:6px; } .ow-btn { background:none; border:1px solid var(--border); color:var(--fg); font-size:12px; padding:6px 11px; border-radius:6px; cursor:pointer; } .ow-btn:hover { background:var(--bg-hover); }
.ow-btn.rec { background:var(--accent); border-color:var(--accent); color:#06140f; font-weight:700; } .ow-btn.cancel { margin-left:auto; color:var(--fg-muted); }
.sy-box { width:640px; } .sy-paths { font-size:11px; color:var(--fg-muted); margin-bottom:8px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; direction:rtl; text-align:left; }
.sy-opt { display:flex; align-items:center; gap:8px; font-size:12px; margin:4px 0; } .sy-danger { color:#f0883e; }
.sy-ignore { flex:1; background:var(--bg-darkest); border:1px solid var(--border); color:var(--fg); border-radius:4px; padding:3px 7px; font-size:11px; }
.sy-sum { font-size:12px; font-weight:600; margin:6px 0; } .sy-ok { color:#3fb950; font-size:12px; padding:8px 0; } .sy-busy { font-size:11px; color:var(--fg-muted); display:flex; align-items:center; gap:6px; }
.ow-tag.rm { color:#f85149; border-color:#f8514955; }
.ow-note { margin-top:8px; font-size:10.5px; color:var(--fg-muted); }
.ftp-disc { color:#f0883e !important; border-color:#f0883e66 !important; } .ftp-disc:hover { background:#f0883e22 !important; }
.ftp-recon { color:#3fb950 !important; border-color:#3fb95066 !important; }
.ftp-reconnect { background:#3fb950; color:#0b1d0f; border:none; border-radius:6px; padding:8px 18px; font-size:13px; font-weight:700; cursor:pointer; } .ftp-reconnect:disabled { opacity:.6; }
.ftp-queue { flex-shrink:0; border-top:1px solid var(--border); background:var(--bg-dark,#141416); display:flex; flex-direction:column; height:210px; }
.ftp-queue.collapsed { height:auto; }
.q-head { display:flex; align-items:center; gap:6px; padding:4px 10px; border-bottom:1px solid var(--border); flex-wrap:wrap; }
.q-toggle { background:none; border:none; color:var(--fg); font-size:12px; font-weight:600; cursor:pointer; }
.q-tabs { display:flex; gap:2px; } .q-tabs button { background:none; border:1px solid transparent; color:var(--fg-muted); font-size:11px; padding:2px 9px; border-radius:11px; cursor:pointer; } .q-tabs button.on { border-color:var(--border); background:var(--bg-hover); color:var(--fg-bright); } .q-tabs button.bad { color:#f85149; }
.q-spacer { flex:1; } .q-total { font-size:11px; color:var(--fg-muted); }
.q-btn { background:none; border:1px solid var(--border); color:var(--fg); font-size:11px; padding:2px 8px; border-radius:4px; cursor:pointer; } .q-btn:hover:not(:disabled) { background:var(--bg-hover); } .q-btn:disabled { opacity:.35; cursor:default; }
.q-body { flex:1; overflow:auto; }
.q-empty { padding:16px; text-align:center; color:var(--fg-muted); font-size:12px; }
.q-row { display:flex; align-items:center; gap:10px; padding:3px 12px; font-size:12px; border-bottom:1px solid #ffffff08; } .q-row:hover { background:var(--bg-hover); }
.q-dir { width:14px; font-weight:700; } .q-dir.up { color:#58a6ff; } .q-dir.down { color:#3fb950; }
.q-name { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; } .q-size { width:70px; text-align:right; color:var(--fg-muted); font-size:11px; }
.q-state { width:240px; display:flex; align-items:center; gap:6px; font-size:11px; color:var(--fg-muted); overflow:hidden; white-space:nowrap; } .q-row.done .q-state { color:#3fb950; } .q-err { color:#f85149; overflow:hidden; text-overflow:ellipsis; }
.q-bar { flex:1; height:6px; background:var(--bg-darkest); border-radius:3px; overflow:hidden; } .q-bar i { display:block; height:100%; background:var(--accent); }
.q-x { background:none; border:none; color:var(--fg-muted); cursor:pointer; font-size:11px; padding:0 4px; } .q-x:hover { color:#f85149; }
.ftp-view { display:flex; flex-direction:column; height:100%; background:var(--bg-darkest); font-family:var(--font-ui); overflow:hidden; position:relative; }
.disconnected-overlay { position:absolute; inset:0; z-index:50; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:14px; background:rgba(10,12,14,0.88); backdrop-filter:blur(4px); color:var(--fg-muted); }
.disc-title { font-size:18px; font-weight:700; color:var(--fg); }
.disc-sub { font-size:12.5px; color:var(--fg-dim); text-align:center; line-height:1.7; }
.ftp-view.dragging { user-select:none; cursor:col-resize; }

/* Top bar */
.ftp-topbar { display:flex; align-items:center; gap:12px; padding:0 14px; height:40px; flex-shrink:0; background:var(--bg-dark); border-bottom:1px solid var(--border); }
.ftp-conn-badge { display:flex; align-items:center; gap:7px; font-size:13px; font-weight:600; color:var(--fg); }
.ftp-badge-proto { font-size:10px; color:var(--accent); background:rgba(208,208,216,0.12); border:1px solid rgba(208,208,216,0.25); border-radius:10px; padding:1px 7px; font-weight:700; }
.ftp-badge-name { color:var(--fg-bright); }

.ftp-transfer-actions { display:flex; align-items:center; gap:6px; }
.ftp-xfer-btn { display:flex; align-items:center; gap:5px; border:none; border-radius:6px; font-size:11.5px; font-weight:600; padding:5px 12px; cursor:pointer; transition:opacity 0.12s; }
.ftp-xfer-btn.upload { background:rgba(208,208,216,0.2); color:var(--accent); border:1px solid rgba(208,208,216,0.35); }
.ftp-xfer-btn.download { background:rgba(80,130,200,0.15); color:#79b8ff; border:1px solid rgba(80,130,200,0.3); }
.ftp-xfer-btn:disabled { opacity:0.35; cursor:default; }
.ftp-xfer-btn:not(:disabled):hover { opacity:0.8; }

.upload-current-name { font-size:10.5px; color:var(--fg-dim); max-width:160px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.upload-progress { width:120px; height:6px; background:var(--bg-active); border-radius:3px; overflow:hidden; flex-shrink:0; }
.upload-progress-bar { height:100%; background:#3fb950; border-radius:3px; transition:width 0.2s, background 0.2s; }
.upload-progress-bar.has-error { background:#f85149; }
.upload-percent { font-size:11px; color:#3fb950; font-family:var(--font-mono); font-weight:700; flex-shrink:0; min-width:32px; }
.upload-percent.has-error { color:#f85149; }
.upload-bytes { font-size:10.5px; color:var(--fg-muted); font-family:var(--font-mono); flex-shrink:0; white-space:nowrap; }
.upload-count { font-size:11px; color:#3fb950; font-family:var(--font-mono); flex-shrink:0; }
.upload-count.has-error { color:#f85149; }
.ftp-cancel-btn {
  background:rgba(248,81,73,0.12); border:1px solid rgba(248,81,73,0.3); color:#f85149;
  border-radius:5px; font-size:10.5px; font-weight:600; padding:3px 9px; cursor:pointer; flex-shrink:0;
}
.ftp-cancel-btn:hover { background:rgba(248,81,73,0.22); }

.xfer-spin { width:14px; height:14px; border:2px solid rgba(208,208,216,0.3); border-top-color:var(--accent); border-radius:50%; animation:spin 0.7s linear infinite; }
@keyframes spin { to { transform:rotate(360deg); } }

.ftp-status { font-size:11.5px; padding:4px 10px; border-radius:5px; animation:fadein 0.2s; }
.ftp-status.ok { background:rgba(208,208,216,0.12); color:var(--accent); border:1px solid rgba(208,208,216,0.25); }
.ftp-status.err { background:rgba(248,81,73,0.12); color:#f85149; border:1px solid rgba(248,81,73,0.25); }
@keyframes fadein { from { opacity:0; transform:translateY(-2px); } to { opacity:1; } }

/* Layout */
.ftp-main { flex:1; display:flex; overflow:hidden; min-height:0; }
.ftp-pane { display:flex; flex-direction:column; overflow:hidden; flex-shrink:0; }

/* Pane header */
.ftp-pane-header { display:flex; align-items:center; gap:5px; padding:0 10px; height:36px; flex-shrink:0; background:var(--bg-panel); border-bottom:1px solid var(--border); overflow:hidden; }
.pane-label { font-size:9px; font-weight:800; letter-spacing:1.2px; color:var(--fg-muted); text-transform:uppercase; flex-shrink:0; background:var(--bg-mid); border-radius:3px; padding:2px 6px; }
.remote-label { color:var(--accent); }
.pane-nav { display:flex; gap:1px; flex-shrink:0; }
.pane-nav-btn { width:22px; height:22px; background:none; border:none; color:var(--fg-muted); border-radius:4px; cursor:pointer; display:flex; align-items:center; justify-content:center; transition:all 0.1s; }
.pane-nav-btn:hover:not(:disabled) { background:var(--bg-hover); color:var(--fg); }
.pane-nav-btn:disabled { opacity:0.3; cursor:default; }
.pane-sel-count { font-size:10px; background:rgba(208,208,216,0.2); color:var(--accent); border-radius:10px; padding:1px 7px; flex-shrink:0; font-weight:700; }

.pane-summary { font-size:10px; color:var(--fg-muted); white-space:nowrap; flex-shrink:0; max-width:34%; overflow:hidden; text-overflow:ellipsis; }
.path-input { flex:1; min-width:60px; background:var(--bg-darkest); border:1px solid var(--accent); color:var(--fg-bright); font-size:11.5px; padding:2px 6px; border-radius:4px; outline:none; font-family:var(--font-mono, monospace); }
.crumb-edit { background:none; border:none; color:var(--fg-muted); font-size:11px; cursor:pointer; opacity:.5; padding:0 4px; } .crumb-edit:hover { opacity:1; color:var(--accent); }
.ftp-ctx { position:fixed; z-index:500; min-width:210px; background:var(--bg-dark,#141416); border:1px solid var(--border); border-radius:7px; box-shadow:0 10px 30px rgba(0,0,0,.6); padding:4px; }
.ftp-ctx button { display:flex; justify-content:space-between; align-items:center; gap:18px; width:100%; background:none; border:none; color:var(--fg); font-size:12px; padding:5px 10px; border-radius:4px; cursor:pointer; text-align:left; }
.ftp-ctx button:hover { background:var(--bg-hover); } .ftp-ctx button.danger { color:#f85149; } .ftp-ctx kbd, .ftp-keys kbd { font:10px var(--font-mono, monospace); color:var(--fg-muted); background:var(--bg-darkest); border:1px solid var(--border); border-radius:3px; padding:0 4px; }
.ftp-ctx .ctx-sep { height:1px; background:var(--border); margin:3px 4px; }
.ftp-keys { position:absolute; right:12px; top:44px; z-index:450; width:430px; background:var(--bg-dark,#141416); border:1px solid var(--border); border-radius:8px; box-shadow:0 10px 30px rgba(0,0,0,.6); padding:10px 14px; font-size:12px; line-height:1.9; color:var(--fg); }
.pane-breadcrumb { display:flex; align-items:center; flex:1; overflow:hidden; }
.crumb-btn { background:none; border:none; color:var(--fg-muted); font-size:11px; cursor:pointer; padding:2px 3px; border-radius:3px; white-space:nowrap; transition:color 0.1s; }
.crumb-btn:not(.active)::after { content:'/'; color:var(--border); margin-left:1px; }
.crumb-btn:hover { color:var(--fg); }
.crumb-btn.active { color:var(--fg-bright); font-weight:600; cursor:default; }


/* New dir */
.new-dir-bar { display:flex; align-items:center; gap:6px; padding:6px 10px; background:var(--bg-dark); border-bottom:1px solid var(--border); flex-shrink:0; }
.new-dir-input { flex:1; background:var(--bg-mid); border:1px solid var(--accent); border-radius:4px; color:var(--fg); font-size:12px; padding:3px 7px; outline:none; }
.new-dir-ok,.new-dir-cancel { background:none; border:none; cursor:pointer; font-size:13px; padding:2px 6px; border-radius:4px; }
.new-dir-ok { color:var(--accent); }
.new-dir-cancel { color:var(--fg-muted); }

/* Pane body */
.pane-body { flex:1; overflow-y:auto; overflow-x:hidden; outline:none; }
.pane-body--split { flex:none; height:55%; }
.pane-body:focus { box-shadow:inset 0 0 0 1px rgba(208,208,216,0.35); }
.pane-body::-webkit-scrollbar { width:5px; }
.pane-body::-webkit-scrollbar-thumb { background:var(--bg-active); border-radius:3px; }

.pane-loading,.pane-error { display:flex; align-items:center; gap:8px; padding:16px; font-size:12px; color:var(--fg-muted); }
.pane-error { color:#f85149; }
.mini-spin { width:12px; height:12px; border:2px solid rgba(208,208,216,0.3); border-top-color:var(--accent); border-radius:50%; animation:spin 0.7s linear infinite; flex-shrink:0; }

/* Rows */
.pane-body.drop-hover { outline:2px dashed var(--accent, #2e9e87); outline-offset:-4px; background:rgba(46,158,135,0.06); }
.pane-row { display:flex; align-items:center; gap:6px; padding:3px 8px; font-size:12px; color:var(--fg-dim); cursor:pointer; transition:background 0.08s; user-select:none; border-radius:3px; margin:1px 4px; }
.pane-row:hover { background:var(--bg-hover); color:var(--fg); }
.pane-row:hover .row-act-btn { opacity:1; }
.pane-row.selected { background:rgba(208,208,216,0.14); color:var(--fg-bright); }
.pane-row.focused { outline:1px solid rgba(208,208,216,0.5); outline-offset:-1px; }
.pane-row.focused:not(.selected) { background:var(--bg-hover); }
.pane-row.is_dir .row-name { font-weight:600; color:var(--fg); }

.row-check { width:14px; height:14px; border:1px solid var(--border); border-radius:3px; flex-shrink:0; display:flex; align-items:center; justify-content:center; transition:all 0.1s; }
.row-check.checked { background:var(--accent); border-color:var(--accent); color:var(--accent-fg); }
.row-icon { flex-shrink:0; }
.row-name { flex:1; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.row-modified { font-size:10px; color:var(--fg-muted); flex-shrink:0; min-width:78px; text-align:right; white-space:nowrap; }
.row-size { font-size:10px; color:var(--fg-muted); flex-shrink:0; min-width:52px; text-align:right; }

.row-actions { display:flex; gap:1px; opacity:0; transition:opacity 0.1s; flex-shrink:0; }
.row-act-btn { width:20px; height:20px; background:none; border:none; border-radius:3px; color:var(--fg-muted); cursor:pointer; display:flex; align-items:center; justify-content:center; transition:all 0.1s; opacity:0; }
.row-act-btn:hover { background:var(--bg-active); color:var(--fg); }
.row-act-btn.del:hover { color:#f85149; }

/* Rename */
.rename-input { flex:1; background:var(--bg-mid); border:1px solid var(--accent); border-radius:4px; color:var(--fg); font-size:12px; padding:2px 6px; outline:none; }
.rename-ok,.rename-cancel { background:none; border:none; cursor:pointer; font-size:13px; padding:2px 5px; border-radius:3px; }
.rename-ok { color:var(--accent); }
.rename-cancel { color:var(--fg-muted); }

/* Splitter */
.ftp-splitter { width:5px; flex-shrink:0; cursor:col-resize; background:var(--border); display:flex; align-items:center; justify-content:center; transition:background 0.12s; }
.ftp-splitter:hover { background:var(--accent); }
.splitter-handle { width:3px; height:40px; border-radius:2px; background:rgba(255,255,255,0.08); }

/* File preview (shared local + remote) */
.file-preview { border-top:1px solid var(--border); flex-shrink:0; display:flex; flex-direction:column; height:45%; background:var(--bg-darkest); }
.preview-header { display:flex; align-items:center; gap:7px; padding:5px 10px; background:var(--bg-dark); border-bottom:1px solid var(--border); flex-shrink:0; }
.preview-filename { font-size:12px; font-weight:600; color:var(--fg); }
.preview-size { font-size:10.5px; color:var(--fg-muted); }
.preview-save-btn { display:flex; align-items:center; gap:5px; background:rgba(208,208,216,0.15); border:1px solid rgba(208,208,216,0.3); border-radius:5px; color:var(--accent); font-size:11px; font-weight:600; padding:3px 10px; cursor:pointer; transition:opacity 0.12s; }
.preview-save-btn:hover { opacity:0.8; }
.preview-close-btn { background:none; border:none; color:var(--fg-muted); font-size:17px; cursor:pointer; line-height:1; width:22px; height:22px; border-radius:4px; display:flex; align-items:center; justify-content:center; }
.preview-close-btn:hover { background:var(--bg-hover); color:var(--fg); }
.preview-loading { display:flex; align-items:center; gap:8px; padding:16px; font-size:12px; color:var(--fg-muted); }
.preview-textarea { flex:1; background:transparent; border:none; color:var(--fg); font-family:var(--font-mono); font-size:12.5px; line-height:1.6; padding:10px 14px; resize:none; outline:none; overflow:auto; }
.preview-textarea::-webkit-scrollbar { width:6px; }
.preview-textarea::-webkit-scrollbar-thumb { background:var(--bg-active); border-radius:3px; }
</style>
