<script setup lang="ts">
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '../composables/useI18n'

const { t } = useI18n()

interface Header { key: string; value: string; enabled: boolean }
interface HistoryItem { method: string; url: string; status: number; time: number; ts: number }
interface HttpResponse {
  status: number
  status_text: string
  headers: Record<string, string>
  body: string
  time_ms: number
}

const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS']

const method = ref('GET')
const url = ref('')
const activeTab = ref<'params' | 'headers' | 'body' | 'auth'>('headers')
const bodyTab = ref<'none' | 'json' | 'form' | 'raw'>('none')
const bodyJson = ref('')
const bodyRaw = ref('')

const headers = ref<Header[]>([
  { key: 'Content-Type', value: 'application/json', enabled: true },
  { key: '', value: '', enabled: true },
])
const params = ref<Header[]>([{ key: '', value: '', enabled: true }])

const loading = ref(false)
const response = ref<HttpResponse | null>(null)
const error = ref('')

const history = ref<HistoryItem[]>(JSON.parse(localStorage.getItem('api_history') ?? '[]'))

const responseTab = ref<'body' | 'headers'>('body')

const prettyBody = computed(() => {
  if (!response.value) return ''
  const ct = Object.entries(response.value.headers).find(([k]) => k.toLowerCase() === 'content-type')?.[1] ?? ''
  if (ct.includes('json')) {
    try { return JSON.stringify(JSON.parse(response.value.body), null, 2) } catch { /**/ }
  }
  return response.value.body
})

const statusColor = computed(() => {
  const s = response.value?.status ?? 0
  if (s >= 200 && s < 300) return '#a6e3a1'
  if (s >= 300 && s < 400) return '#89b4fa'
  if (s >= 400 && s < 500) return '#f9e2af'
  return '#f38ba8'
})

function addRow(list: typeof headers.value) {
  list.push({ key: '', value: '', enabled: true })
}

function removeRow(list: typeof headers.value, i: number) {
  if (list.length > 1) list.splice(i, 1)
}

function buildFinalUrl() {
  const enabledParams = params.value.filter(p => p.enabled && p.key.trim())
  if (!enabledParams.length) return url.value
  const qs = enabledParams.map(p => `${encodeURIComponent(p.key)}=${encodeURIComponent(p.value)}`).join('&')
  return url.value + (url.value.includes('?') ? '&' : '?') + qs
}

async function send() {
  const finalUrl = buildFinalUrl()
  if (!finalUrl.trim()) return
  loading.value = true
  error.value = ''
  response.value = null

  const hdrs: Record<string, string> = {}
  for (const h of headers.value) {
    if (h.enabled && h.key.trim()) hdrs[h.key.trim()] = h.value
  }

  let body: string | null = null
  if (bodyTab.value === 'json') body = bodyJson.value
  else if (bodyTab.value === 'raw') body = bodyRaw.value

  try {
    const res = await invoke<HttpResponse>('http_request', {
      method: method.value,
      url: finalUrl,
      headers: hdrs,
      body,
    })
    response.value = res
    const item: HistoryItem = { method: method.value, url: finalUrl, status: res.status, time: res.time_ms, ts: Date.now() }
    history.value.unshift(item)
    if (history.value.length > 50) history.value = history.value.slice(0, 50)
    localStorage.setItem('api_history', JSON.stringify(history.value))
  } catch (e: any) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

function loadHistory(item: HistoryItem) {
  method.value = item.method
  url.value = item.url
}

function clearHistory() {
  history.value = []
  localStorage.removeItem('api_history')
}

function copyBody() {
  navigator.clipboard.writeText(prettyBody.value)
}

function methodColor(m: string) {
  const c: Record<string, string> = { GET: '#a6e3a1', POST: '#89b4fa', PUT: '#f9e2af', PATCH: '#cba6f7', DELETE: '#f38ba8', HEAD: '#89dceb', OPTIONS: '#585b70' }
  return c[m] ?? '#9da5b4'
}
</script>

<template>
  <div class="api-client">
    <!-- URL Bar -->
    <div class="url-bar">
      <select v-model="method" class="method-select" :style="{ color: methodColor(method) }">
        <option v-for="m in METHODS" :key="m" :value="m" :style="{ color: methodColor(m) }">{{ m }}</option>
      </select>
      <input
        v-model="url"
        class="url-input"
        placeholder="https://api.example.com/endpoint"
        @keydown.enter="send"
      />
      <button class="send-btn" :disabled="loading" @click="send">
        <span v-if="loading" class="spinner" />
        <span v-else>{{ t('apiSend') }}</span>
      </button>
    </div>

    <!-- Request Tabs -->
    <div class="req-tabs">
      <button
        v-for="tab in ['params','headers','body'] as const"
        :key="tab"
        class="req-tab"
        :class="{ active: activeTab === tab }"
        @click="activeTab = tab"
      >{{ tab.charAt(0).toUpperCase() + tab.slice(1) }}</button>
    </div>

    <!-- Params Tab -->
    <div v-if="activeTab === 'params'" class="kv-table">
      <div class="kv-row kv-head">
        <span></span><span>{{ t('key') }}</span><span>{{ t('value') }}</span><span></span>
      </div>
      <div v-for="(p, i) in params" :key="i" class="kv-row">
        <input type="checkbox" v-model="p.enabled" class="kv-check" />
        <input v-model="p.key" class="kv-input" placeholder="key" />
        <input v-model="p.value" class="kv-input" placeholder="value" />
        <button class="kv-del" @click="removeRow(params, i)">×</button>
      </div>
      <button class="kv-add" @click="addRow(params)">{{ t('addParam') }}</button>
    </div>

    <!-- Headers Tab -->
    <div v-if="activeTab === 'headers'" class="kv-table">
      <div class="kv-row kv-head">
        <span></span><span>{{ t('key') }}</span><span>{{ t('value') }}</span><span></span>
      </div>
      <div v-for="(h, i) in headers" :key="i" class="kv-row">
        <input type="checkbox" v-model="h.enabled" class="kv-check" />
        <input v-model="h.key" class="kv-input" placeholder="Header-Name" />
        <input v-model="h.value" class="kv-input" placeholder="value" />
        <button class="kv-del" @click="removeRow(headers, i)">×</button>
      </div>
      <button class="kv-add" @click="addRow(headers)">{{ t('addHeader') }}</button>
    </div>

    <!-- Body Tab -->
    <div v-if="activeTab === 'body'" class="body-section">
      <div class="body-type-row">
        <button
          v-for="bt in ['none','json','raw'] as const"
          :key="bt"
          class="body-type-btn"
          :class="{ active: bodyTab === bt }"
          @click="bodyTab = bt"
        >{{ bt }}</button>
      </div>
      <textarea
        v-if="bodyTab === 'json'"
        v-model="bodyJson"
        class="body-textarea"
        placeholder='{"key": "value"}'
        spellcheck="false"
      />
      <textarea
        v-if="bodyTab === 'raw'"
        v-model="bodyRaw"
        class="body-textarea"
        placeholder="Raw body content..."
        spellcheck="false"
      />
      <div v-if="bodyTab === 'none'" class="body-none">{{ t('noBody') }}</div>
    </div>

    <!-- Response -->
    <div class="response-section">
      <div class="response-header">
        <span class="resp-label">{{ t('apiResponse') }}</span>
        <template v-if="response">
          <span class="resp-status" :style="{ color: statusColor }">{{ response.status }} {{ response.status_text }}</span>
          <span class="resp-time">{{ response.time_ms }}ms</span>
          <button class="resp-copy" @click="copyBody" :title="t('copyBodyTitle')">⎘</button>
        </template>
        <div class="resp-tabs" v-if="response">
          <button class="resp-tab" :class="{ active: responseTab === 'body' }" @click="responseTab = 'body'">{{ t('apiBody') }}</button>
          <button class="resp-tab" :class="{ active: responseTab === 'headers' }" @click="responseTab = 'headers'">{{ t('apiHeaders') }}</button>
        </div>
      </div>

      <div v-if="error" class="resp-error">{{ error }}</div>
      <div v-else-if="loading" class="resp-loading"><span class="spinner" /> {{ t('sendingLabel') }}</div>
      <div v-else-if="!response" class="resp-empty">{{ t('apiNoResponse') }}</div>
      <div v-else-if="responseTab === 'body'" class="resp-body">
        <pre class="resp-pre">{{ prettyBody }}</pre>
      </div>
      <div v-else-if="responseTab === 'headers'" class="resp-headers-list">
        <div v-for="(v, k) in response.headers" :key="k" class="resp-hdr-row">
          <span class="resp-hdr-key">{{ k }}</span>
          <span class="resp-hdr-val">{{ v }}</span>
        </div>
      </div>
    </div>

    <!-- History (collapsible) -->
    <details class="history-section">
      <summary class="history-title">
        {{ t('historyLabel') }}
        <button v-if="history.length" class="hist-clear" @click.prevent="clearHistory">{{ t('clearBtn') }}</button>
      </summary>
      <div v-if="!history.length" class="hist-empty">{{ t('noHistoryYet') }}</div>
      <div
        v-for="(item, i) in history"
        :key="i"
        class="hist-row"
        @click="loadHistory(item)"
      >
        <span class="hist-method" :style="{ color: methodColor(item.method) }">{{ item.method }}</span>
        <span class="hist-url">{{ item.url }}</span>
        <span class="hist-status" :style="{ color: item.status >= 400 ? '#f38ba8' : '#a6e3a1' }">{{ item.status }}</span>
      </div>
    </details>
  </div>
</template>

<style scoped>
.api-client {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
  font-family: var(--font-ui);
  font-size: 12px;
  color: var(--fg);
  background: var(--bg);
}

.url-bar {
  display: flex;
  gap: 6px;
  padding: 8px 10px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.method-select {
  background: var(--bg-mid);
  border: 1px solid var(--border);
  border-radius: 6px;
  font-size: 11.5px;
  font-weight: 700;
  padding: 0 8px;
  cursor: pointer;
  font-family: var(--font-mono);
  outline: none;
  height: 30px;
}

.url-input {
  flex: 1;
  background: var(--bg-mid);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--fg);
  font-size: 12px;
  font-family: var(--font-mono);
  padding: 0 10px;
  outline: none;
  height: 30px;
}
.url-input:focus { border-color: var(--accent); }

.send-btn {
  background: var(--accent);
  border: none;
  border-radius: 6px;
  color: #fff;
  font-size: 12px;
  font-weight: 700;
  padding: 0 18px;
  cursor: pointer;
  height: 30px;
  display: flex;
  align-items: center;
  gap: 6px;
  transition: opacity 0.12s;
}
.send-btn:disabled { opacity: 0.5; cursor: not-allowed; }
.send-btn:hover:not(:disabled) { opacity: 0.85; }

.req-tabs {
  display: flex;
  gap: 0;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
  padding: 0 10px;
}
.req-tab {
  background: none;
  border: none;
  border-bottom: 2px solid transparent;
  color: var(--fg-muted);
  font-size: 11.5px;
  padding: 6px 12px;
  cursor: pointer;
  transition: all 0.12s;
  margin-bottom: -1px;
}
.req-tab.active { color: var(--accent); border-bottom-color: var(--accent); }
.req-tab:hover { color: var(--fg); }

/* KV table */
.kv-table {
  padding: 6px 10px;
  display: flex;
  flex-direction: column;
  gap: 3px;
  flex-shrink: 0;
  max-height: 160px;
  overflow-y: auto;
}
.kv-head {
  display: grid;
  grid-template-columns: 18px 1fr 1fr 22px;
  gap: 4px;
  font-size: 10px;
  font-weight: 700;
  color: var(--fg-muted);
  letter-spacing: 0.8px;
  padding: 0 2px;
}
.kv-row {
  display: grid;
  grid-template-columns: 18px 1fr 1fr 22px;
  gap: 4px;
  align-items: center;
}
.kv-check { width: 14px; height: 14px; accent-color: var(--accent); cursor: pointer; }
.kv-input {
  background: var(--bg-mid);
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--fg);
  font-size: 11px;
  font-family: var(--font-mono);
  padding: 3px 6px;
  outline: none;
  height: 22px;
}
.kv-input:focus { border-color: var(--accent); }
.kv-del {
  background: none;
  border: none;
  color: var(--fg-muted);
  font-size: 14px;
  cursor: pointer;
  padding: 0;
  line-height: 1;
}
.kv-del:hover { color: var(--red); }
.kv-add {
  background: none;
  border: none;
  color: var(--accent);
  font-size: 11px;
  cursor: pointer;
  text-align: left;
  padding: 4px 2px;
}
.kv-add:hover { text-decoration: underline; }

/* Body */
.body-section { display: flex; flex-direction: column; flex-shrink: 0; }
.body-type-row {
  display: flex;
  gap: 0;
  padding: 4px 10px;
  border-bottom: 1px solid var(--border);
}
.body-type-btn {
  background: none;
  border: 1px solid transparent;
  border-radius: 4px;
  color: var(--fg-muted);
  font-size: 11px;
  padding: 3px 10px;
  cursor: pointer;
}
.body-type-btn.active { background: var(--bg-hover); color: var(--fg); border-color: var(--border); }
.body-textarea {
  background: var(--bg-darkest);
  border: none;
  border-bottom: 1px solid var(--border);
  color: var(--fg);
  font-size: 12px;
  font-family: var(--font-mono);
  padding: 8px 10px;
  resize: none;
  height: 100px;
  outline: none;
}
.body-none { padding: 10px 12px; color: var(--fg-muted); font-size: 11px; font-style: italic; }

/* Response */
.response-section {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border-top: 1px solid var(--border);
}
.response-header {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 10px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
  flex-wrap: wrap;
}
.resp-label { font-size: 10px; font-weight: 700; letter-spacing: 1px; color: var(--fg-muted); text-transform: uppercase; }
.resp-status { font-weight: 700; font-size: 12px; }
.resp-time { font-size: 11px; color: var(--fg-muted); }
.resp-copy { background: none; border: 1px solid var(--border); border-radius: 4px; color: var(--fg-muted); cursor: pointer; padding: 1px 6px; font-size: 13px; }
.resp-copy:hover { color: var(--accent); border-color: var(--accent); }
.resp-tabs { display: flex; gap: 0; margin-left: auto; }
.resp-tab { background: none; border: none; border-bottom: 2px solid transparent; color: var(--fg-muted); font-size: 11px; padding: 4px 10px; cursor: pointer; }
.resp-tab.active { color: var(--accent); border-bottom-color: var(--accent); }

.resp-error { color: var(--red); padding: 10px 12px; font-size: 12px; }
.resp-loading { display: flex; align-items: center; gap: 8px; padding: 14px 12px; color: var(--fg-muted); }
.resp-empty { padding: 20px 12px; color: var(--fg-muted); font-size: 12px; text-align: center; font-style: italic; }
.resp-body { flex: 1; overflow: auto; }
.resp-pre {
  margin: 0;
  padding: 10px 12px;
  font-size: 11.5px;
  font-family: var(--font-mono);
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--fg);
}
.resp-headers-list { flex: 1; overflow: auto; padding: 6px 10px; display: flex; flex-direction: column; gap: 3px; }
.resp-hdr-row { display: grid; grid-template-columns: 200px 1fr; gap: 8px; font-size: 11.5px; padding: 3px 0; border-bottom: 1px solid rgba(255,255,255,0.04); }
.resp-hdr-key { color: var(--accent); font-family: var(--font-mono); }
.resp-hdr-val { color: var(--fg); font-family: var(--font-mono); word-break: break-all; }

/* History */
.history-section { border-top: 1px solid var(--border); flex-shrink: 0; max-height: 140px; overflow: hidden; display: flex; flex-direction: column; }
.history-title {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  cursor: pointer;
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 1px;
  color: var(--fg-muted);
  text-transform: uppercase;
  user-select: none;
}
.hist-clear { margin-left: auto; background: none; border: none; color: var(--fg-muted); font-size: 11px; cursor: pointer; padding: 0; }
.hist-clear:hover { color: var(--red); }
.hist-empty { padding: 8px 12px; color: var(--fg-muted); font-size: 11px; font-style: italic; }
.hist-row { display: flex; align-items: center; gap: 8px; padding: 4px 10px; cursor: pointer; transition: background 0.1s; }
.hist-row:hover { background: var(--bg-hover); }
.hist-method { font-size: 10.5px; font-weight: 700; font-family: var(--font-mono); width: 52px; flex-shrink: 0; }
.hist-url { flex: 1; font-size: 11px; font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--fg-dim); }
.hist-status { font-size: 11px; font-weight: 600; }

.spinner {
  display: inline-block;
  width: 10px; height: 10px;
  border: 2px solid rgba(255,255,255,0.3);
  border-top-color: #fff;
  border-radius: 50%;
  animation: spin 0.7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }
</style>
