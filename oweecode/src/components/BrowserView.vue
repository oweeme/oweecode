<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, watch } from 'vue'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useI18n } from '../composables/useI18n'
import { useEditorStore } from '../composables/useEditorStore'

const props = defineProps<{ initialUrl: string; path: string }>()
const { t } = useI18n()
const store = useEditorStore()

interface Device { id: string; label: string; width: number | null; height: number | null }

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

const url = ref(props.initialUrl)
const urlInput = ref(props.initialUrl)
const deviceId = ref('responsive')
const landscape = ref(false)
const iframeKey = ref(0)
const loading = ref(false)
const canvasEl = ref<HTMLElement | null>(null)
const canvasSize = ref({ width: 0, height: 0 })

const selectedDevice = computed(() => DEVICES.find(d => d.id === deviceId.value) ?? DEVICES[0])
const isResponsive = computed(() => selectedDevice.value.width === null)

const deviceW = computed(() => {
  if (isResponsive.value) return 0
  const d = selectedDevice.value
  return landscape.value ? d.height! : d.width!
})
const deviceH = computed(() => {
  if (isResponsive.value) return 0
  const d = selectedDevice.value
  return landscape.value ? d.width! : d.height!
})

// Auto-fit scale so the emulated device frame always fits the panel, same
// idea as Chrome DevTools' device toolbar — with generous padding so the
// frame's bezel/shadow doesn't get clipped.
const PADDING = 32
const scale = computed(() => {
  if (isResponsive.value || !canvasSize.value.width) return 1
  const availW = canvasSize.value.width - PADDING * 2
  const availH = canvasSize.value.height - PADDING * 2
  return Math.min(1, availW / deviceW.value, availH / deviceH.value)
})

let ro: ResizeObserver | null = null
onMounted(() => {
  if (canvasEl.value) {
    ro = new ResizeObserver(entries => {
      const { width, height } = entries[0].contentRect
      canvasSize.value = { width, height }
    })
    ro.observe(canvasEl.value)
  }
})
onBeforeUnmount(() => ro?.disconnect())

function normalizeUrl(raw: string): string {
  const trimmed = raw.trim()
  if (!trimmed) return trimmed
  if (/^https?:\/\//i.test(trimmed)) return trimmed
  if (/^localhost(:\d+)?/i.test(trimmed) || /^\d{1,3}(\.\d{1,3}){3}(:\d+)?/.test(trimmed)) {
    return `http://${trimmed}`
  }
  return `https://${trimmed}`
}

function go() {
  const next = normalizeUrl(urlInput.value)
  urlInput.value = next
  loading.value = true
  if (next === url.value) { iframeKey.value++ } // same URL → force a real reload
  url.value = next
}

function reload() {
  loading.value = true
  iframeKey.value++
}

function onIframeLoad() { loading.value = false }

async function openExternal() {
  try { await openUrl(url.value) } catch { /* ignore */ }
}

watch(() => props.initialUrl, (v) => { url.value = v; urlInput.value = v })

// Can't read the loaded page's real <title> — it's cross-origin content
// inside an iframe, off-limits to our JS by design (same reason a website
// can't read another site's iframe). The hostname is the closest honest
// substitute, and unlike the page title it's always available since it
// comes straight from the address bar, not the page itself.
watch(url, (u) => {
  try {
    const host = new URL(u).host
    if (!host) return
    const tab = store.state.tabs.find(t => t.path === props.path)
    if (tab) tab.name = host
  } catch { /* incomplete/invalid URL mid-typing — leave the tab name as-is */ }
}, { immediate: true })
</script>

<template>
  <div class="browser-view">
    <div class="browser-toolbar">
      <button class="browser-btn" @click="reload" :title="t('reload')">
        <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor" :class="{ spinning: loading }">
          <path d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2v1z"/>
          <path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466z"/>
        </svg>
      </button>
      <input
        v-model="urlInput" class="browser-url-input" spellcheck="false"
        placeholder="localhost:3000" @keydown.enter="go"
      />
      <button class="browser-btn browser-go" @click="go" :title="t('browserGoTitle')">→</button>
      <button class="browser-btn" @click="openExternal" :title="t('browserOpenExternalTitle')">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M8.636 3.5a.5.5 0 0 0-.5-.5H1.5A1.5 1.5 0 0 0 0 4.5v10A1.5 1.5 0 0 0 1.5 16h10a1.5 1.5 0 0 0 1.5-1.5V7.864a.5.5 0 0 0-1 0V14.5a.5.5 0 0 1-.5.5h-10a.5.5 0 0 1-.5-.5v-10a.5.5 0 0 1 .5-.5h6.636a.5.5 0 0 0 .5-.5z"/><path d="M16 .5a.5.5 0 0 0-.5-.5h-5a.5.5 0 0 0 0 1h3.793L6.146 9.146a.5.5 0 1 0 .708.708L15 1.707V5.5a.5.5 0 0 0 1 0v-5z"/></svg>
      </button>
      <div class="browser-spacer" />

      <select v-model="deviceId" class="browser-device-select">
        <option v-for="d in DEVICES" :key="d.id" :value="d.id">{{ d.label }}</option>
      </select>
      <button v-if="!isResponsive" class="browser-btn" @click="landscape = !landscape" :title="t('browserRotateTitle')">
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><rect x="3" y="1" width="10" height="14" rx="1.5" transform="rotate(90 8 8)"/><path d="M6 8h4m0 0l-1.5-1.5M10 8l-1.5 1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>
      </button>
      <span v-if="!isResponsive" class="browser-size-label">{{ deviceW }}×{{ deviceH }} · {{ Math.round(scale * 100) }}%</span>
    </div>

    <div ref="canvasEl" class="browser-canvas">
      <div v-if="isResponsive" class="device-frame device-frame--responsive">
        <iframe :key="iframeKey" :src="url" class="browser-iframe" @load="onIframeLoad" />
      </div>
      <div
        v-else class="device-frame"
        :style="{ width: deviceW + 'px', height: deviceH + 'px', transform: `scale(${scale})` }"
      >
        <iframe :key="iframeKey" :src="url" class="browser-iframe" :style="{ width: deviceW + 'px', height: deviceH + 'px' }" @load="onIframeLoad" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.browser-view { display:flex; flex-direction:column; height:100%; background:var(--bg-darkest); font-family:var(--font-ui); overflow:hidden; }

.browser-toolbar {
  display:flex; align-items:center; gap:6px; padding:0 10px; height:38px; flex-shrink:0;
  background:var(--bg-dark); border-bottom:1px solid var(--border);
}
.browser-btn {
  width:26px; height:26px; flex-shrink:0; background:none; border:1px solid var(--border);
  border-radius:5px; color:var(--fg-muted); cursor:pointer; display:flex; align-items:center; justify-content:center;
}
.browser-btn:hover { background:var(--bg-hover); color:var(--fg); }
.browser-btn svg.spinning { animation:spin 0.8s linear infinite; }
@keyframes spin { to { transform:rotate(360deg); } }

.browser-url-input {
  flex:1; min-width:120px; background:var(--bg-mid); border:1px solid var(--border); border-radius:5px;
  color:var(--fg); font-size:12px; font-family:var(--font-mono); padding:5px 9px; outline:none;
}
.browser-url-input:focus { border-color:var(--accent); }
.browser-go { color:var(--accent); font-weight:700; }

.browser-spacer { flex:0 0 8px; }

.browser-device-select {
  background:var(--bg-mid); border:1px solid var(--border); border-radius:5px;
  color:var(--fg); font-size:11.5px; padding:5px 7px; outline:none; font-family:var(--font-ui); flex-shrink:0;
}
.browser-size-label { font-size:10.5px; color:var(--fg-muted); font-family:var(--font-mono); flex-shrink:0; white-space:nowrap; }

.browser-canvas {
  flex:1; overflow:auto; display:flex; align-items:center; justify-content:center;
  background:
    linear-gradient(45deg, var(--bg-dark) 25%, transparent 25%),
    linear-gradient(-45deg, var(--bg-dark) 25%, transparent 25%),
    linear-gradient(45deg, transparent 75%, var(--bg-dark) 75%),
    linear-gradient(-45deg, transparent 75%, var(--bg-dark) 75%);
  background-size:20px 20px; background-position:0 0, 0 10px, 10px -10px, -10px 0;
}

.device-frame {
  flex-shrink:0; border-radius:14px; overflow:hidden; box-shadow:0 12px 40px rgba(0,0,0,0.5);
  border:6px solid #1a1b1e; background:#000;
}
.device-frame--responsive { width:100%; height:100%; border:none; border-radius:0; box-shadow:none; }

.browser-iframe { display:block; width:100%; height:100%; border:none; background:#fff; }
</style>
