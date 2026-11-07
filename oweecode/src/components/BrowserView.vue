<script setup lang="ts">
// Preview of your running project: finds the local servers that are up (any port), offers to start the ones this
// project normally uses, can serve a plain HTML folder by itself, and can show two ports side by side.
import { ref, computed, onMounted, onBeforeUnmount, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useI18n } from '../composables/useI18n'
import { rustText } from '../composables/rustText'
import { useEditorStore } from '../composables/useEditorStore'
import { loadBookmarks, saveBookmarks, toggleBookmark, isBookmarked, labelFor, type Bookmark } from '../composables/browserBookmarks'
import { shouldReload, cssHandledInPage, type ReloadMode } from '../composables/previewReload'

const props = defineProps<{ initialUrl: string; path: string }>()
const { t, locale } = useI18n()
const rt = (s: string) => rustText(s, locale.value)
const store = useEditorStore()

interface Device { id: string; label: string; width: number | null; height: number | null }
interface ListeningPort { port: number; address: string; pid: number | null; process: string; cmdline: string; dev: boolean }
interface Suggestion { label: string; command: string; port: number | null; note: string; kind: string }
interface Pane { url: string; key: number; loading: boolean }

const DEVICES: Device[] = [
  { id: 'responsive',  label: 'Responsive',            width: null, height: null },
  { id: 'iphone-se',   label: 'iPhone SE',              width: 375,  height: 667 },
  { id: 'iphone-14',   label: 'iPhone 14 Pro',          width: 393,  height: 852 },
  { id: 'pixel-7',     label: 'Pixel 7',                width: 412,  height: 915 },
  { id: 'galaxy-s20',  label: 'Galaxy S20 Ultra',       width: 412,  height: 915 },
  { id: 'ipad-mini',   label: 'iPad Mini',              width: 768,  height: 1024 },
  { id: 'ipad-air',    label: 'iPad Air',               width: 820,  height: 1180 },
  { id: 'ipad-pro',    label: 'iPad Pro 12.9"',         width: 1024, height: 1366 },
  { id: 'surface-pro', label: 'Surface Pro 7',          width: 912,  height: 1368 },
  { id: 'desktop',     label: 'Desktop 1440p',          width: 1440, height: 900 },
]
const COMMON_PORTS = [80, 3000, 5173, 8000, 8080, 9000]

// ── panes (one, or two side by side) ───────────────────────────────────────────
const panes = ref<Pane[]>([{ url: props.initialUrl, key: 0, loading: false }])
const active = ref(0)
const urlInput = ref(props.initialUrl)
const split = computed(() => panes.value.length > 1)
const cur = computed(() => panes.value[active.value])

const deviceId = ref('responsive')
const landscape = ref(false)
const canvasEl = ref<HTMLElement | null>(null)
const canvasSize = ref({ width: 0, height: 0 })
const selectedDevice = computed(() => DEVICES.find(d => d.id === deviceId.value) ?? DEVICES[0])
const isResponsive = computed(() => selectedDevice.value.width === null || split.value)
const deviceW = computed(() => (isResponsive.value ? 0 : landscape.value ? selectedDevice.value.height! : selectedDevice.value.width!))
const deviceH = computed(() => (isResponsive.value ? 0 : landscape.value ? selectedDevice.value.width! : selectedDevice.value.height!))
const PADDING = 32
const scale = computed(() => {
  if (isResponsive.value || !canvasSize.value.width) return 1
  return Math.min(1, (canvasSize.value.width - PADDING * 2) / deviceW.value, (canvasSize.value.height - PADDING * 2) / deviceH.value)
})

// ── what is running / what the project offers ──────────────────────────────────
const listening = ref<ListeningPort[]>([])
const scanned = ref(false)
const suggestions = ref<Suggestion[]>([])
const staticPorts = ref<number[]>([])
const showMenu = ref(false)
const showFavs = ref(false)
const bookmarks = ref<Bookmark[]>([])
const faved = computed(() => isBookmarked(bookmarks.value, cur.value?.url ?? ''))
function toggleFav() { bookmarks.value = toggleBookmark(bookmarks.value, cur.value?.url ?? ''); saveBookmarks(root.value, bookmarks.value) }
function removeFav(url: string) { bookmarks.value = bookmarks.value.filter(b => b.url !== url); saveBookmarks(root.value, bookmarks.value) }
function openFav(url: string) { showFavs.value = false; navigate(url) }
const showOthers = ref(false)
const notice = ref('')
const waiting = ref<{ label: string; port: number | null; before: Set<number>; until: number } | null>(null)
const root = computed(() => store.state.rootPath)

// ── reload by itself when a project file changes ──
const reloadMode = ref<ReloadMode>((() => { try { const v = localStorage.getItem('browser_reload'); return v === 'always' || v === 'off' ? v : 'auto' } catch { return 'auto' } })())
watch(reloadMode, v => { try { localStorage.setItem('browser_reload', v) } catch { /* ignore */ } })
let lastFp = ''
let reloadTimer: ReturnType<typeof setTimeout> | null = null
async function checkProjectChanges() {
  if (reloadMode.value === 'off' || !root.value || waiting.value) return
  try {
    const fp = await invoke<{ count: number; newest_ms: number; newest_path: string }>('project_fingerprint', { root: root.value })
    const key = `${fp.count}:${fp.newest_ms}`
    if (lastFp && key !== lastFp) {
      const file = fp.newest_path.replace(root.value + '/', '')
      // a burst of saves (formatters, git checkout…) becomes one reload
      if (reloadTimer) clearTimeout(reloadTimer)
      reloadTimer = setTimeout(() => {
        let n = 0
        for (const p of panes.value) if (p.url && !isDown(p) && shouldReload(reloadMode.value, p.url, listening.value) && !cssHandledInPage(file, p.url, staticPorts.value)) { p.key++; p.loading = true; n++ }
        if (n) notice.value = t('bReloaded', { file })
      }, 350)
    }
    lastFp = key
  } catch { /* project folder gone or unreadable */ }
}
const listeningSet = computed(() => new Set(listening.value.map(p => p.port)))
const devPorts = computed(() => listening.value.filter(p => p.dev))
const otherPorts = computed(() => listening.value.filter(p => !p.dev))

function parseLocal(u: string): { port: number } | null {
  try {
    const x = new URL(u)
    if (!['localhost', '127.0.0.1', '0.0.0.0', '[::1]'].includes(x.hostname)) return null
    return { port: Number(x.port || (x.protocol === 'https:' ? 443 : 80)) }
  } catch { return null }
}
/** a local address with nothing listening behind it: show help instead of a blank frame */
function isDown(p: Pane) { const l = parseLocal(p.url); return !!l && scanned.value && !listeningSet.value.has(l.port) }
const processLabel = (p: ListeningPort) => (p.process ? p.process : p.pid ? `pid ${p.pid}` : t('bProcOther'))
const chipPorts = computed(() => [...new Set([...COMMON_PORTS, ...devPorts.value.map(p => p.port), ...staticPorts.value])].sort((a, b) => a - b))

async function scan() {
  try { listening.value = await invoke<ListeningPort[]>('list_listening_ports'); scanned.value = true } catch { scanned.value = true }
  try { staticPorts.value = await invoke<number[]>('static_server_ports') } catch { /* ignore */ }
  checkWaiting()
}
async function loadSuggestions() {
  if (!root.value) { suggestions.value = []; return }
  try { suggestions.value = await invoke<Suggestion[]>('detect_dev_servers', { root: root.value }) } catch { suggestions.value = [] }
}

// ── navigation ────────────────────────────────────────────────────────────────
function normalizeUrl(raw: string): string {
  const v = raw.trim()
  if (!v) return v
  if (/^https?:\/\//i.test(v)) return v
  if (/^:?\d{2,5}$/.test(v)) return `http://localhost:${v.replace(':', '')}`          // "9000" or ":9000"
  if (/^(localhost|\d{1,3}(\.\d{1,3}){3}|\[::1\])(:\d+)?(\/|$)/i.test(v)) return `http://${v}`
  return `https://${v}`
}
function navigate(raw: string, paneIdx = active.value) {
  const next = normalizeUrl(raw)
  const p = panes.value[paneIdx]; if (!p) return
  if (next === p.url) p.key++                      // same address: a real reload
  p.url = next; p.loading = !!next
  if (paneIdx === active.value) urlInput.value = next
  rememberShared()
}
watch([deviceId, landscape], () => rememberShared())
/** Keeps the tab's address and device up to date so the Oweeme panel can share "what I am previewing". */
function rememberShared() {
  const tab = store.state.tabs.find((x) => x.path === props.path)
  if (!tab) return
  tab.browserUrl = cur.value?.url ?? ''
  tab.browserDevice = selectedDevice.value.label + (landscape.value ? ' ↻' : '')
}
const go = () => navigate(urlInput.value)
function reload() { const p = cur.value; p.key++; p.loading = true; void scan() }
function openPort(port: number) { showMenu.value = false; navigate(`http://localhost:${port}`) }
async function openExternal() { try { if (cur.value.url) await openUrl(cur.value.url) } catch { /* ignore */ } }
function setActive(i: number) { active.value = i; urlInput.value = panes.value[i].url; rememberShared() }
function toggleSplit() {
  if (split.value) { panes.value.splice(1); setActive(0); return }
  const other = devPorts.value.find(p => `http://localhost:${p.port}` !== panes.value[0].url)
  panes.value.push({ url: other ? `http://localhost:${other.port}` : '', key: 0, loading: false })
  setActive(1)
}

// ── start a project server (terminal) or serve a folder (built in) ──────────────
async function start(s: Suggestion) {
  showMenu.value = false
  if (s.command.startsWith('static:')) return serveFolder(s.command.slice(7))
  window.dispatchEvent(new CustomEvent('owee-run', { detail: s.command }))
  waiting.value = { label: s.label, port: s.port, before: new Set(listening.value.map(p => p.port)), until: Date.now() + 120_000 }
  notice.value = ''
}
async function serveFolder(sub = '') {
  showMenu.value = false
  if (!root.value) { notice.value = t('bOpenFirst'); return }
  try {
    const port = await invoke<number>('static_server_start', { folder: sub ? `${root.value}/${sub}` : root.value, preferred: null })
    await scan()
    navigate(`http://localhost:${port}`)
    notice.value = t('bServing', { what: sub || t('bFolder'), port })
  } catch (e) { notice.value = String(e) }
}
async function stopStatic(port: number) {
  await invoke('static_server_stop', { port })
  await scan()
  notice.value = t('bStopped', { port })
}
// opens the project as soon as its server answers (known port) or when any new web server appears
function checkWaiting() {
  const w = waiting.value; if (!w) return
  if (Date.now() > w.until) { waiting.value = null; notice.value = t('bNoStart'); return }
  const found = w.port ? (listeningSet.value.has(w.port) ? w.port : null) : (devPorts.value.find(p => !w.before.has(p.port))?.port ?? null)
  if (found) { waiting.value = null; navigate(`http://localhost:${found}`); notice.value = t('bReady', { port: found }) }
}

let timer: ReturnType<typeof setInterval> | null = null
let fpTimer: ReturnType<typeof setInterval> | null = null
let ro: ResizeObserver | null = null
onMounted(async () => {
  bookmarks.value = loadBookmarks(root.value)
  if (canvasEl.value) { ro = new ResizeObserver(e => { canvasSize.value = { width: e[0].contentRect.width, height: e[0].contentRect.height } }); ro.observe(canvasEl.value) }
  await Promise.all([scan(), loadSuggestions()])
  // a fresh tab with no address: open the project's own server if it is already running
  if (!panes.value[0].url) {
    const wanted = suggestions.value.map(s => s.port).filter((p): p is number => !!p).find(p => listeningSet.value.has(p))
    const any = devPorts.value.find(p => p.port !== 1420)
    const pick = wanted ?? any?.port
    if (pick) navigate(`http://localhost:${pick}`)
  }
  timer = setInterval(scan, 2500)
  fpTimer = setInterval(checkProjectChanges, 1200)
  void checkProjectChanges()
})
onBeforeUnmount(() => { ro?.disconnect(); if (timer) clearInterval(timer); if (fpTimer) clearInterval(fpTimer); if (reloadTimer) clearTimeout(reloadTimer) })
watch(root, () => { lastFp = ''; bookmarks.value = loadBookmarks(root.value); void loadSuggestions() })
watch(() => props.initialUrl, v => { if (v) navigate(v, 0) })

// the tab shows the host:port of the active pane
watch(() => cur.value.url, (u) => {
  try {
    const x = new URL(u); const tab = store.state.tabs.find(tb => tb.path === props.path)
    if (tab && x.host) tab.name = split.value ? `${panes.value.map(p => { try { return new URL(p.url).host } catch { return '…' } }).join(' | ')}` : x.host
  } catch { /* still typing */ }
}, { immediate: true })
</script>

<template>
  <div class="browser-view" @click="showMenu = false; showFavs = false">
    <div class="browser-toolbar" @click.stop>
      <button class="browser-btn" @click="reload" :title="t('reload')">
        <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor" :class="{ spinning: cur.loading }">
          <path d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2v1z"/>
          <path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466z"/>
        </svg>
      </button>
      <input v-model="urlInput" class="browser-url-input" spellcheck="false" placeholder="localhost:3000  ·  9000  ·  https://…" @keydown.enter="go" />
      <button class="browser-btn browser-go" @click="go" :title="t('browserGoTitle')">→</button>
      <div class="srv-wrap">
        <button class="browser-btn fav-btn" :class="{ on: faved }" :disabled="!cur?.url" @click.stop="toggleFav" :title="faved ? t('bFavDel') : t('bFavAdd')">{{ faved ? '★' : '☆' }}</button>
      </div>
      <div class="srv-wrap">
        <button class="browser-btn" :class="{ on: showFavs }" @click.stop="showFavs = !showFavs; showMenu = false" :title="t('bFavMenu')">⌄<b v-if="bookmarks.length" class="fav-n">{{ bookmarks.length }}</b></button>
        <div v-if="showFavs" class="srv-menu fav-menu" @click.stop>
          <div class="srv-h">{{ t('bFavs') }}</div>
          <div v-if="!bookmarks.length" class="srv-empty">{{ t('bFavNone') }}</div>
          <div v-for="b in bookmarks" :key="b.url" class="srv-row fav-row" @click="openFav(b.url)">
            <span class="fav-star">★</span><span class="srv-label">{{ b.title || labelFor(b.url) }}</span><small>{{ b.url.replace(/^https?:\/\//, '') }}</small>
            <button class="srv-mini" @click.stop="removeFav(b.url)" :title="t('bFavRemove')">✕</button>
          </div>
        </div>
      </div>
      <button class="browser-btn" @click="openExternal" :title="t('browserOpenExternalTitle')">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M8.636 3.5a.5.5 0 0 0-.5-.5H1.5A1.5 1.5 0 0 0 0 4.5v10A1.5 1.5 0 0 0 1.5 16h10a1.5 1.5 0 0 0 1.5-1.5V7.864a.5.5 0 0 0-1 0V14.5a.5.5 0 0 1-.5.5h-10a.5.5 0 0 1-.5-.5v-10a.5.5 0 0 1 .5-.5h6.636a.5.5 0 0 0 .5-.5z"/><path d="M16 .5a.5.5 0 0 0-.5-.5h-5a.5.5 0 0 0 0 1h3.793L6.146 9.146a.5.5 0 1 0 .708.708L15 1.707V5.5a.5.5 0 0 0 1 0v-5z"/></svg>
      </button>

      <div class="srv-wrap">
        <button class="srv-btn" :class="{ live: devPorts.length }" @click.stop="showMenu = !showMenu" :title="t('bServersTip')">
          <span class="dot" :class="{ on: devPorts.length }" /> {{ t('bServers') }} <b v-if="devPorts.length">{{ devPorts.length }}</b> ▾
        </button>
        <div v-if="showMenu" class="srv-menu" @click.stop>
          <div class="srv-h">{{ t('bRunningNow') }}</div>
          <div v-if="!devPorts.length" class="srv-empty">{{ t('bNoServer') }}</div>
          <button v-for="p in devPorts" :key="p.port" class="srv-row" @click="openPort(p.port)">
            <span class="dot on" /><b>:{{ p.port }}</b><span class="srv-proc">{{ processLabel(p) }}</span><small :title="p.cmdline">{{ p.cmdline.replace(/^.*node_modules\//, '').slice(0, 44) }}</small>
          </button>
          <div v-for="sp in staticPorts" :key="'s' + sp" class="srv-row static">
            <span class="dot on" /><b>:{{ sp }}</b><span class="srv-proc">{{ t('bOwnServer') }}</span>
            <button class="srv-mini" @click="stopStatic(sp)">{{ t('bStop') }}</button>
          </div>
          <template v-if="otherPorts.length">
            <button class="srv-h clickable" @click="showOthers = !showOthers">{{ showOthers ? '▾' : '▸' }} {{ t('bOtherPorts', { n: otherPorts.length }) }}</button>
            <button v-for="p in showOthers ? otherPorts : []" :key="p.port" class="srv-row dim" @click="openPort(p.port)"><span class="dot" /><b>:{{ p.port }}</b><span class="srv-proc">{{ processLabel(p) }}</span></button>
          </template>

          <div class="srv-h" style="margin-top:8px">{{ t('bForProject') }}</div>
          <div v-if="!suggestions.length" class="srv-empty">{{ root ? t('bNoRecipe') : t('bOpenProject') }}</div>
          <button v-for="s in suggestions" :key="s.command" class="srv-row start" @click="start(s)">
            <span class="play">▶</span><span class="srv-label">{{ rt(s.label) }}</span><small>{{ s.port ? ':' + s.port : '' }}</small>
            <em v-if="s.note">{{ rt(s.note) }}</em>
          </button>
          <button class="srv-row start" @click="serveFolder('')"><span class="play">▶</span><span class="srv-label">{{ t('bServeOther') }}</span></button>
        </div>
      </div>

      <select v-model="reloadMode" class="browser-device-select" :title="t('bReloadTip')">
        <option value="auto">{{ t('bReloadAuto') }}</option>
        <option value="always">{{ t('bReloadAlways') }}</option>
        <option value="off">{{ t('bReloadOff') }}</option>
      </select>
      <button class="browser-btn" :class="{ on: split }" @click="toggleSplit" :title="split ? t('bSplitOff') : t('bSplitOn')">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="4" width="18" height="16" rx="2"/><line x1="12" y1="4" x2="12" y2="20"/></svg>
      </button>
      <select v-model="deviceId" class="browser-device-select" :disabled="split" :title="split ? t('bSplitSize') : ''">
        <option v-for="d in DEVICES" :key="d.id" :value="d.id">{{ d.label }}</option>
      </select>
      <button v-if="!isResponsive" class="browser-btn" @click="landscape = !landscape" :title="t('browserRotateTitle')">
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><rect x="3" y="1" width="10" height="14" rx="1.5" transform="rotate(90 8 8)"/><path d="M6 8h4m0 0l-1.5-1.5M10 8l-1.5 1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>
      </button>
      <span v-if="!isResponsive" class="browser-size-label">{{ deviceW }}×{{ deviceH }} · {{ Math.round(scale * 100) }}%</span>
    </div>

    <!-- quick ports -->
    <div class="chips-row">
      <span class="chips-label">{{ t('bPorts') }}</span>
      <button v-for="p in chipPorts" :key="p" class="chip" :class="{ live: listeningSet.has(p), cur: parseLocal(cur.url)?.port === p }" @click="openPort(p)" :title="listeningSet.has(p) ? t('bPortLive') : t('bPortDead')">
        <span class="dot" :class="{ on: listeningSet.has(p) }" />:{{ p }}
      </button>
      <span v-if="waiting" class="waiting"><span class="mini-spin" /> {{ t('bStarting', { label: rt(waiting.label), what: waiting.port ? t('bWaitPort', { port: waiting.port }) : t('bWaitAny') }) }} <button class="srv-mini" @click="waiting = null">{{ t('bCancel') }}</button></span>
      <span v-else-if="notice" class="notice" @click="notice = ''">{{ notice }} ✕</span>
    </div>

    <div ref="canvasEl" class="browser-canvas" :class="{ split }">
      <div v-for="(p, i) in panes" :key="i" class="pane" :class="{ active: split && active === i }" @click="setActive(i)">
        <div v-if="split" class="pane-head" @click.stop="setActive(i)">
          <span class="dot" :class="{ on: !isDown(p) && !!p.url }" />{{ p.url || t('bNoAddress') }}
          <button class="pane-x" @click.stop="navigate(p.url, i)" :title="t('bReloadPane')">↻</button>
        </div>

        <!-- nothing there: help instead of a blank white page -->
        <div v-if="!p.url || isDown(p)" class="empty">
          <div class="empty-card">
            <div class="empty-title">{{ p.url ? t('bNothingAt', { url: p.url.replace(/^https?:\/\//, '') }) : t('bTitleEmpty') }}</div>
            <p class="empty-sub">{{ p.url ? t('bServerOff') : t('bPickServer') }}</p>
            <template v-if="devPorts.length">
              <div class="empty-h">{{ t('bRunning') }}</div>
              <div class="empty-chips"><button v-for="s in devPorts" :key="s.port" class="chip live" @click="navigate(`http://localhost:${s.port}`, i)"><span class="dot on" />:{{ s.port }} <small>{{ processLabel(s) }}</small></button></div>
            </template>
            <template v-if="suggestions.length">
              <div class="empty-h">{{ t('bStartProject') }}</div>
              <button v-for="s in suggestions.slice(0, 5)" :key="s.command" class="empty-start" @click="setActive(i); start(s)"><span class="play">▶</span> {{ rt(s.label) }} <small>{{ s.port ? t('bPortWord', { n: s.port }) : '' }}</small></button>
            </template>
            <button class="empty-start alt" @click="setActive(i); serveFolder('')">{{ t('bServeStatic') }}</button>
            <p class="empty-tip">{{ t('bTip', { ports: '80, 9000, 8080' }) }}</p>
          </div>
        </div>

        <template v-else>
          <div v-if="isResponsive" class="device-frame device-frame--responsive">
            <iframe :key="p.key" :src="p.url" class="browser-iframe" @load="p.loading = false" />
          </div>
          <div v-else class="device-frame" :style="{ width: deviceW + 'px', height: deviceH + 'px', transform: `scale(${scale})` }">
            <iframe :key="p.key" :src="p.url" class="browser-iframe" :style="{ width: deviceW + 'px', height: deviceH + 'px' }" @load="p.loading = false" />
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.browser-view { display:flex; flex-direction:column; height:100%; background:var(--bg-darkest); font-family:var(--font-ui); overflow:hidden; position:relative; }
.browser-toolbar { display:flex; align-items:center; gap:6px; padding:0 10px; height:38px; flex-shrink:0; background:var(--bg-dark); border-bottom:1px solid var(--border); }
.browser-btn { width:26px; height:26px; flex-shrink:0; background:none; border:1px solid var(--border); border-radius:5px; color:var(--fg-muted); cursor:pointer; display:flex; align-items:center; justify-content:center; }
.browser-btn:hover { background:var(--bg-hover); color:var(--fg); } .browser-btn.on { color:var(--accent); border-color:var(--accent); }
.browser-btn svg.spinning { animation:spin 0.8s linear infinite; } @keyframes spin { to { transform:rotate(360deg); } }
.browser-url-input { flex:1; min-width:120px; background:var(--bg-mid); border:1px solid var(--border); border-radius:5px; color:var(--fg); font-size:12px; font-family:var(--font-mono); padding:5px 9px; outline:none; }
.browser-url-input:focus { border-color:var(--accent); } .browser-go { color:var(--accent); font-weight:700; }
.browser-device-select { background:var(--bg-mid); border:1px solid var(--border); border-radius:5px; color:var(--fg); font-size:11.5px; padding:5px 7px; outline:none; flex-shrink:0; } .browser-device-select:disabled { opacity:.4; }
.browser-size-label { font-size:10.5px; color:var(--fg-muted); font-family:var(--font-mono); flex-shrink:0; white-space:nowrap; }

.dot { width:7px; height:7px; border-radius:50%; background:#6b6f78; display:inline-block; flex-shrink:0; } .dot.on { background:#3fb950; box-shadow:0 0 5px #3fb95099; }
.srv-wrap { position:relative; flex-shrink:0; }
.fav-btn.on { color:#e5a50a; } .fav-n { font-size:9px; margin-left:3px; color:var(--accent); }
.fav-menu { width:360px; right:auto; left:0; } .fav-row { cursor:pointer; } .fav-star { color:#e5a50a; }
.srv-btn { display:flex; align-items:center; gap:6px; height:26px; background:none; border:1px solid var(--border); color:var(--fg); font-size:11.5px; border-radius:5px; padding:0 9px; cursor:pointer; } .srv-btn.live { border-color:#3fb95066; } .srv-btn b { color:#3fb950; }
.srv-menu { position:absolute; top:30px; right:0; z-index:100; width:440px; max-height:70vh; overflow:auto; background:var(--bg-dark); border:1px solid var(--border); border-radius:8px; box-shadow:0 12px 36px rgba(0,0,0,.6); padding:6px; }
.srv-h { font-size:10.5px; text-transform:uppercase; letter-spacing:.06em; color:var(--fg-muted); padding:5px 8px 3px; background:none; border:none; text-align:left; width:100%; } .srv-h.clickable { cursor:pointer; text-transform:none; letter-spacing:0; }
.srv-empty { padding:4px 8px; font-size:11.5px; color:var(--fg-muted); }
.srv-row { display:flex; align-items:center; gap:8px; width:100%; background:none; border:none; color:var(--fg); font-size:12px; padding:5px 8px; border-radius:5px; cursor:pointer; text-align:left; flex-wrap:wrap; } .srv-row:hover { background:var(--bg-hover); } .srv-row.dim { opacity:.65; }
.srv-row b { font-family:var(--font-mono); } .srv-proc { color:var(--accent); font-size:11px; } .srv-row small { color:var(--fg-muted); font-size:10.5px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; max-width:200px; }
.srv-row.start .play, .empty-start .play { color:#3fb950; } .srv-label { flex:1; min-width:0; } .srv-row em { width:100%; font-size:10.5px; color:var(--fg-muted); font-style:normal; padding-left:20px; }
.srv-mini { background:none; border:1px solid var(--border); color:var(--fg); font-size:10.5px; border-radius:4px; padding:1px 7px; cursor:pointer; margin-left:auto; }

.chips-row { display:flex; align-items:center; gap:5px; padding:4px 10px; border-bottom:1px solid var(--border); background:var(--bg-dark); flex-shrink:0; flex-wrap:wrap; }
.chips-label { font-size:10.5px; color:var(--fg-muted); }
.chip { display:inline-flex; align-items:center; gap:5px; font-size:11px; font-family:var(--font-mono); background:none; border:1px solid var(--border); color:var(--fg-muted); border-radius:11px; padding:1px 9px; cursor:pointer; } .chip.live { color:var(--fg); border-color:#3fb95055; } .chip.cur { background:var(--bg-hover); color:var(--fg-bright); border-color:var(--accent); } .chip:hover { background:var(--bg-hover); } .chip small { font-family:var(--font-ui); color:var(--fg-muted); }
.waiting, .notice { margin-left:auto; font-size:11px; color:var(--accent); display:inline-flex; align-items:center; gap:6px; } .notice { cursor:pointer; color:var(--fg-muted); }
.mini-spin { width:9px; height:9px; border:2px solid rgba(208,208,216,.3); border-top-color:var(--accent); border-radius:50%; animation:spin .7s linear infinite; display:inline-block; }

.browser-canvas { flex:1; overflow:auto; display:flex; align-items:stretch; justify-content:center;
  background: linear-gradient(45deg, var(--bg-dark) 25%, transparent 25%), linear-gradient(-45deg, var(--bg-dark) 25%, transparent 25%), linear-gradient(45deg, transparent 75%, var(--bg-dark) 75%), linear-gradient(-45deg, transparent 75%, var(--bg-dark) 75%);
  background-size:20px 20px; background-position:0 0, 0 10px, 10px -10px, -10px 0; }
.pane { flex:1; min-width:0; display:flex; flex-direction:column; align-items:center; justify-content:center; position:relative; } .browser-canvas.split .pane { border-right:1px solid var(--border); } .pane.active { outline:1px solid var(--accent); outline-offset:-1px; }
.pane-head { align-self:stretch; display:flex; align-items:center; gap:6px; font-size:11px; font-family:var(--font-mono); padding:3px 8px; background:var(--bg-dark); border-bottom:1px solid var(--border); color:var(--fg-muted); } .pane-x { margin-left:auto; background:none; border:none; color:var(--fg-muted); cursor:pointer; }
.device-frame { flex-shrink:0; border-radius:14px; overflow:hidden; box-shadow:0 12px 40px rgba(0,0,0,0.5); border:6px solid #1a1b1e; background:#000; }
.device-frame--responsive { width:100%; height:100%; flex:1; border:none; border-radius:0; box-shadow:none; }
.pane .device-frame--responsive { align-self:stretch; }
.browser-iframe { display:block; width:100%; height:100%; border:none; background:#fff; }

.empty { flex:1; align-self:stretch; display:flex; align-items:center; justify-content:center; padding:20px; overflow:auto; background:var(--bg-darkest); }
.empty-card { width:520px; max-width:100%; } .empty-title { font-size:16px; font-weight:700; color:var(--fg-bright); margin-bottom:4px; } .empty-sub { font-size:12.5px; color:var(--fg-muted); margin:0 0 14px; }
.empty-h { font-size:10.5px; text-transform:uppercase; letter-spacing:.06em; color:var(--fg-muted); margin:12px 0 5px; } .empty-chips { display:flex; flex-wrap:wrap; gap:6px; }
.empty-start { display:flex; align-items:center; gap:8px; width:100%; text-align:left; background:var(--bg-dark); border:1px solid var(--border); color:var(--fg); font-size:12.5px; padding:8px 12px; border-radius:7px; cursor:pointer; margin-bottom:5px; } .empty-start:hover { background:var(--bg-hover); border-color:var(--accent); } .empty-start small { margin-left:auto; color:var(--fg-muted); } .empty-start.alt { margin-top:10px; }
.empty-tip { font-size:11px; color:var(--fg-muted); margin-top:16px; line-height:1.6; } .empty-tip code { background:var(--bg-dark); padding:0 4px; border-radius:3px; }
</style>
