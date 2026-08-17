<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ApiRequest, ApiKV, ApiAuth } from './ApiCollections.vue'
import { useI18n } from '../composables/useI18n'

const { t } = useI18n()
const props = defineProps<{ tabPath: string; initialJson: string }>()
const emit = defineEmits<{ (e: 'update-content', content: string): void; (e: 'save-request', req: ApiRequest): void }>()

const METHODS = ['GET','POST','PUT','PATCH','DELETE','HEAD','OPTIONS']
const AUTH_TYPES: { value: ApiAuth['type']; label: string }[] = [
  { value: 'no-auth',  label: 'No Auth' },
  { value: 'api-key',  label: 'API Key' },
  { value: 'bearer',   label: 'Bearer Token' },
  { value: 'jwt',      label: 'JWT Bearer' },
  { value: 'basic',    label: 'Basic Auth' },
  { value: 'digest',   label: 'Digest Auth' },
  { value: 'oauth1',   label: 'OAuth 1.0' },
  { value: 'oauth2',   label: 'OAuth 2.0' },
]

interface HttpResponse { status: number; status_text: string; headers: Record<string,string>; body: string; time_ms: number }

function defaultReq(): ApiRequest {
  return {
    id: crypto.randomUUID(),
    name: 'New Request',
    method: 'GET',
    url: '',
    headers: [{ key: 'Content-Type', value: 'application/json', enabled: true }],
    params: [],
    bodyType: 'none',
    body: '',
    auth: { type: 'no-auth' },
  }
}

function loadReq(): ApiRequest {
  try {
    const parsed = JSON.parse(props.initialJson)
    if (parsed.id) {
      if (!parsed.auth) parsed.auth = { type: 'no-auth' }
      return parsed
    }
  } catch { /**/ }
  return defaultReq()
}

const req = ref<ApiRequest>(loadReq())
const activeTab = ref<'params' | 'headers' | 'body' | 'auth'>('headers')
const loading = ref(false)
const response = ref<HttpResponse | null>(null)
const error = ref('')
const responseBodyTab = ref<'pretty' | 'raw' | 'preview'>('pretty')
const saveStatus = ref<'' | 'saved' | 'unsaved'>('')

watch(req, (v) => {
  emit('update-content', JSON.stringify(v))
  saveStatus.value = 'unsaved'
}, { deep: true })

function addRow(list: ApiKV[]) { list.push({ key: '', value: '', enabled: true }) }
function removeRow(list: ApiKV[], i: number) { if (list.length > 1) list.splice(i, 1) }

function buildFinalHeaders(): Record<string,string> {
  const hdrs: Record<string,string> = {}
  for (const h of req.value.headers) {
    if (h.enabled && h.key.trim()) hdrs[h.key.trim()] = h.value
  }
  const auth = req.value.auth
  if (auth.type === 'bearer' && auth.token) {
    hdrs['Authorization'] = `Bearer ${auth.token}`
  } else if (auth.type === 'jwt' && auth.token) {
    hdrs['Authorization'] = `Bearer ${auth.token}`
  } else if (auth.type === 'basic' && auth.username) {
    const encoded = btoa(`${auth.username}:${auth.password ?? ''}`)
    hdrs['Authorization'] = `Basic ${encoded}`
  } else if (auth.type === 'api-key' && auth.apiKeyName && auth.apiKeyValue && auth.apiKeyIn === 'header') {
    hdrs[auth.apiKeyName] = auth.apiKeyValue
  }
  return hdrs
}

function buildFinalUrl() {
  let url = req.value.url
  const enabledParams = req.value.params.filter(p => p.enabled && p.key.trim())
  const auth = req.value.auth
  if (auth.type === 'api-key' && auth.apiKeyName && auth.apiKeyValue && auth.apiKeyIn === 'query') {
    enabledParams.push({ key: auth.apiKeyName, value: auth.apiKeyValue, enabled: true })
  }
  if (!enabledParams.length) return url
  const qs = enabledParams.map(p => `${encodeURIComponent(p.key)}=${encodeURIComponent(p.value)}`).join('&')
  return url + (url.includes('?') ? '&' : '?') + qs
}

async function send() {
  const url = buildFinalUrl()
  if (!url.trim()) return
  loading.value = true; error.value = ''; response.value = null
  const hdrs = buildFinalHeaders()
  let body: string | null = null
  if (req.value.bodyType !== 'none') body = req.value.body
  try {
    const res = await invoke<HttpResponse>('http_request', { method: req.value.method, url, headers: hdrs, body })
    response.value = res
    responseBodyTab.value = 'pretty'
  } catch (e: any) { error.value = String(e) }
  finally { loading.value = false }
}

function saveToCollection() {
  emit('save-request', req.value)
  saveStatus.value = 'saved'
  setTimeout(() => { if (saveStatus.value === 'saved') saveStatus.value = '' }, 2000)
}

const isJson = computed(() => {
  if (!response.value) return false
  const ct = Object.entries(response.value.headers).find(([k]) => k.toLowerCase() === 'content-type')?.[1] ?? ''
  return ct.includes('json')
})

const prettyBody = computed(() => {
  if (!response.value) return ''
  if (isJson.value) {
    try { return JSON.stringify(JSON.parse(response.value.body), null, 2) } catch { /**/ }
  }
  return response.value.body
})

function highlightJson(str: string): string {
  const esc = str.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;')
  return esc.replace(
    /("(\\u[a-fA-F0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE][+\-]?\d+)?)/g,
    (m) => {
      if (/^"/.test(m)) return /:$/.test(m) ? `<span class="jk">${m}</span>` : `<span class="js">${m}</span>`
      if (/true|false/.test(m)) return `<span class="jb">${m}</span>`
      if (/null/.test(m)) return `<span class="jnull">${m}</span>`
      return `<span class="jnum">${m}</span>`
    }
  )
}

const highlightedBody = computed(() => isJson.value ? highlightJson(prettyBody.value) : prettyBody.value)

const responseSize = computed(() => {
  if (!response.value) return ''
  const bytes = new TextEncoder().encode(response.value.body).length
  if (bytes < 1024) return `${bytes} B`
  return `${(bytes / 1024).toFixed(2)} KB`
})

const statusColor = computed(() => {
  const s = response.value?.status ?? 0
  if (s >= 200 && s < 300) return '#a6e3a1'
  if (s >= 300 && s < 400) return '#89b4fa'
  if (s >= 400 && s < 500) return '#f9e2af'
  return '#f38ba8'
})

function methodColor(m: string) {
  const c: Record<string,string> = { GET:'#a6e3a1', POST:'#89b4fa', PUT:'#f9e2af', PATCH:'#cba6f7', DELETE:'#f38ba8', HEAD:'#89dceb', OPTIONS:'#585b70' }
  return c[m] ?? '#9da5b4'
}

function copyBody() { navigator.clipboard.writeText(prettyBody.value) }

const authHasData = computed(() => {
  const a = req.value.auth
  if (a.type === 'no-auth') return false
  if ((a.type === 'bearer' || a.type === 'jwt') && a.token) return true
  if (a.type === 'basic' && a.username) return true
  if (a.type === 'api-key' && a.apiKeyName && a.apiKeyValue) return true
  return false
})
</script>

<template>
  <div class="api-tab">
    <!-- Top bar -->
    <div class="api-topbar">
      <input v-model="req.name" class="req-name-input" placeholder="Request name…" />
      <div class="topbar-right">
        <span v-if="saveStatus === 'saved'" class="save-ok">✓ {{ t('savedLabel') }}</span>
        <span v-else-if="saveStatus === 'unsaved'" class="save-dot">●</span>
        <button class="save-btn" @click="saveToCollection" :title="t('apiSave')">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor">
            <path d="M2 1a1 1 0 00-1 1v12a1 1 0 001 1h12a1 1 0 001-1V4.5a.5.5 0 00-.146-.354l-3-3A.5.5 0 0011.5 1H2zm0 1h9.293L14 4.707V14H2V2zm3 7a2 2 0 114 0 2 2 0 01-4 0z"/>
          </svg>
          {{ t('apiSave') }}
        </button>
      </div>
    </div>

    <!-- URL bar -->
    <div class="url-bar">
      <select v-model="req.method" class="method-select" :style="{ color: methodColor(req.method) }">
        <option v-for="m in METHODS" :key="m" :value="m" :style="{ color: methodColor(m) }">{{ m }}</option>
      </select>
      <input v-model="req.url" class="url-input" placeholder="https://api.example.com/endpoint" @keydown.enter="send" />
      <button class="send-btn" :disabled="loading" @click="send">
        <span v-if="loading" class="spinner" />
        <span v-else>{{ t('apiSend') }}</span>
      </button>
    </div>

    <!-- Split -->
    <div class="split">
      <!-- Left: request config -->
      <div class="req-panel">
        <div class="tab-row">
          <button v-for="t in ['params','headers','body','auth'] as const" :key="t"
            class="tab-btn" :class="{ active: activeTab === t }" @click="activeTab = t">
            {{ t.charAt(0).toUpperCase() + t.slice(1) }}
            <span v-if="t === 'params' && req.params.filter(p=>p.enabled&&p.key).length" class="tab-badge">{{ req.params.filter(p=>p.enabled&&p.key).length }}</span>
            <span v-if="t === 'headers' && req.headers.filter(h=>h.enabled&&h.key).length" class="tab-badge">{{ req.headers.filter(h=>h.enabled&&h.key).length }}</span>
            <span v-if="t === 'auth' && authHasData" class="tab-badge auth-badge">✓</span>
          </button>
        </div>

        <!-- Params -->
        <div v-if="activeTab === 'params'" class="kv-section">
          <div class="kv-head"><span/><span>{{ t('key') }}</span><span>{{ t('value') }}</span><span/></div>
          <div v-for="(p,i) in req.params" :key="i" class="kv-row">
            <input type="checkbox" v-model="p.enabled" class="kv-check" />
            <input v-model="p.key" class="kv-input" placeholder="key" />
            <input v-model="p.value" class="kv-input" placeholder="value" />
            <button class="kv-del" @click="removeRow(req.params, i)">×</button>
          </div>
          <button class="kv-add" @click="addRow(req.params)">{{ t('addParam') }}</button>
        </div>

        <!-- Headers -->
        <div v-if="activeTab === 'headers'" class="kv-section">
          <div class="kv-head"><span/><span>{{ t('key') }}</span><span>{{ t('value') }}</span><span/></div>
          <div v-for="(h,i) in req.headers" :key="i" class="kv-row">
            <input type="checkbox" v-model="h.enabled" class="kv-check" />
            <input v-model="h.key" class="kv-input" placeholder="Header-Name" />
            <input v-model="h.value" class="kv-input" placeholder="value" />
            <button class="kv-del" @click="removeRow(req.headers, i)">×</button>
          </div>
          <button class="kv-add" @click="addRow(req.headers)">{{ t('addHeader') }}</button>
        </div>

        <!-- Body -->
        <div v-if="activeTab === 'body'" class="body-section">
          <div class="body-type-row">
            <button v-for="bt in ['none','json','raw'] as const" :key="bt"
              class="body-type-btn" :class="{ active: req.bodyType === bt }" @click="req.bodyType = bt">
              {{ bt }}
            </button>
          </div>
          <textarea v-if="req.bodyType !== 'none'" v-model="req.body" class="body-textarea"
            :placeholder="req.bodyType === 'json' ? '{ &quot;key&quot;: &quot;value&quot; }' : 'Raw body…'"
            spellcheck="false" />
          <div v-else class="body-none">{{ t('noBody') }}</div>
        </div>

        <!-- Authorization -->
        <div v-if="activeTab === 'auth'" class="auth-section">
          <div class="auth-type-row">
            <label class="auth-type-label">{{ t('type') }}</label>
            <select v-model="req.auth.type" class="auth-type-select">
              <option v-for="at in AUTH_TYPES" :key="at.value" :value="at.value">{{ at.label }}</option>
            </select>
          </div>

          <div class="auth-note" v-if="req.auth.type === 'no-auth'">
            This request does not use any authorization.
          </div>

          <!-- API Key -->
          <template v-if="req.auth.type === 'api-key'">
            <div class="auth-field">
              <label>{{ t('key') }}</label>
              <input v-model="req.auth.apiKeyName" class="auth-input" placeholder="X-API-Key" />
            </div>
            <div class="auth-field">
              <label>{{ t('value') }}</label>
              <input v-model="req.auth.apiKeyValue" class="auth-input" placeholder="your-api-key" />
            </div>
            <div class="auth-field">
              <label>Add to</label>
              <div class="auth-radio-row">
                <label class="auth-radio"><input type="radio" v-model="req.auth.apiKeyIn" value="header" /> Header</label>
                <label class="auth-radio"><input type="radio" v-model="req.auth.apiKeyIn" value="query" /> Query Param</label>
              </div>
            </div>
          </template>

          <!-- Bearer Token -->
          <template v-if="req.auth.type === 'bearer' || req.auth.type === 'jwt'">
            <div class="auth-field">
              <label>Token</label>
              <textarea v-model="req.auth.token" class="auth-textarea" placeholder="Enter token…" spellcheck="false" />
            </div>
            <div class="auth-hint" v-if="req.auth.type === 'jwt'">
              Token will be sent as: <code>Authorization: Bearer &lt;token&gt;</code>
            </div>
          </template>

          <!-- Basic Auth -->
          <template v-if="req.auth.type === 'basic' || req.auth.type === 'digest'">
            <div class="auth-field">
              <label>{{ t('user') }}</label>
              <input v-model="req.auth.username" class="auth-input" placeholder="username" autocomplete="off" />
            </div>
            <div class="auth-field">
              <label>{{ t('password') }}</label>
              <input v-model="req.auth.password" class="auth-input" type="password" placeholder="password" autocomplete="off" />
            </div>
            <div class="auth-hint" v-if="req.auth.type === 'digest'">Digest Auth: credentials are hashed before sending.</div>
          </template>

          <!-- OAuth 1/2 info -->
          <template v-if="req.auth.type === 'oauth1' || req.auth.type === 'oauth2'">
            <div class="auth-note auth-note-warn">
              OAuth requires a token exchange flow. Obtain your access token externally and use it as a <strong>Bearer Token</strong>.
            </div>
          </template>
        </div>
      </div>

      <!-- Right: response -->
      <div class="resp-panel">
        <div class="resp-header">
          <span class="resp-label">{{ t('apiResponse').toUpperCase() }}</span>
          <template v-if="response">
            <span class="resp-status" :style="{ color: statusColor }">{{ response.status }} {{ response.status_text }}</span>
            <span class="resp-meta">{{ response.time_ms }} ms</span>
            <span class="resp-meta">{{ responseSize }}</span>
          </template>
          <div class="resp-actions" v-if="response">
            <button class="resp-copy" @click="copyBody" :title="t('copyBodyTitle')">
              <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M4 2a2 2 0 012-2h8a2 2 0 012 2v8a2 2 0 01-2 2H6a2 2 0 01-2-2V2zm2-1a1 1 0 00-1 1v8a1 1 0 001 1h8a1 1 0 001-1V2a1 1 0 00-1-1H6zM2 5a1 1 0 00-1 1v8a1 1 0 001 1h8a1 1 0 001-1v-1h-1v1H2V6h1V5H2z"/></svg>
            </button>
          </div>
        </div>

        <!-- Response body tabs -->
        <div class="resp-tab-row" v-if="response">
          <div class="resp-tabs-left">
            <button v-for="t in ['pretty','raw','preview'] as const" :key="t"
              class="resp-tab" :class="{ active: responseBodyTab === t }" @click="responseBodyTab = t">
              {{ t.charAt(0).toUpperCase() + t.slice(1) }}
            </button>
            <button class="resp-tab" :class="{ active: responseBodyTab === ('headers' as any) }" @click="(responseBodyTab as any) = 'headers'">{{ t('apiHeaders') }}</button>
          </div>
        </div>

        <div v-if="error" class="resp-error">{{ error }}</div>
        <div v-else-if="loading" class="resp-loading"><span class="spinner" /> {{ t('sendingLabel') }}</div>
        <div v-else-if="!response" class="resp-empty">{{ t('apiNoResponse') }}</div>

        <!-- Pretty -->
        <div v-else-if="responseBodyTab === 'pretty'" class="resp-body">
          <pre class="resp-pre" v-if="isJson" v-html="highlightedBody" />
          <pre class="resp-pre" v-else>{{ prettyBody }}</pre>
        </div>

        <!-- Raw -->
        <div v-else-if="responseBodyTab === 'raw'" class="resp-body">
          <pre class="resp-pre resp-pre-raw">{{ response.body }}</pre>
        </div>

        <!-- Preview (HTML) -->
        <div v-else-if="responseBodyTab === 'preview'" class="resp-preview">
          <iframe :srcdoc="response.body" sandbox="allow-same-origin" class="preview-frame" />
        </div>

        <!-- Headers -->
        <div v-else-if="(responseBodyTab as any) === 'headers'" class="resp-hdrs-list">
          <div v-for="(v,k) in response.headers" :key="k" class="resp-hdr-row">
            <span class="resp-hdr-key">{{ k }}</span>
            <span class="resp-hdr-val">{{ v }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.api-tab {
  display: flex; flex-direction: column; height: 100%; overflow: hidden;
  background: var(--bg); font-family: var(--font-ui); font-size: 12px; color: var(--fg);
}

.api-topbar {
  display: flex; align-items: center; gap: 10px; padding: 8px 14px;
  border-bottom: 1px solid var(--border); flex-shrink: 0;
}
.req-name-input {
  flex: 1; background: none; border: none; color: var(--fg-bright);
  font-size: 14px; font-weight: 600; outline: none; font-family: var(--font-ui);
}
.req-name-input::placeholder { color: var(--fg-muted); }
.topbar-right { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
.save-ok { font-size: 11px; color: #a6e3a1; }
.save-dot { font-size: 10px; color: #f9e2af; }
.save-btn {
  display: flex; align-items: center; gap: 6px;
  background: var(--bg-hover); border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg); font-size: 11.5px; font-weight: 600; padding: 5px 12px; cursor: pointer;
}
.save-btn:hover { border-color: var(--accent); color: var(--accent); }

.url-bar {
  display: flex; gap: 6px; padding: 8px 14px;
  border-bottom: 1px solid var(--border); flex-shrink: 0;
}
.method-select {
  background: var(--bg-mid); border: 1px solid var(--border); border-radius: 6px;
  font-size: 12px; font-weight: 700; padding: 0 10px; cursor: pointer;
  font-family: var(--font-mono); outline: none; height: 32px; min-width: 80px;
}
.url-input {
  flex: 1; background: var(--bg-mid); border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg); font-size: 12.5px; font-family: var(--font-mono); padding: 0 12px;
  outline: none; height: 32px;
}
.url-input:focus { border-color: var(--accent); }
.send-btn {
  background: var(--accent); border: none; border-radius: 6px; color: var(--accent-fg);
  font-size: 12.5px; font-weight: 700; padding: 0 22px; cursor: pointer; height: 32px;
  display: flex; align-items: center; gap: 6px;
}
.send-btn:disabled { opacity: 0.5; cursor: not-allowed; }
.send-btn:hover:not(:disabled) { opacity: 0.85; }

.split { flex: 1; display: flex; overflow: hidden; min-height: 0; }
.req-panel { width: 45%; display: flex; flex-direction: column; border-right: 1px solid var(--border); overflow: hidden; }
.resp-panel { flex: 1; display: flex; flex-direction: column; overflow: hidden; }

.tab-row {
  display: flex; border-bottom: 1px solid var(--border);
  padding: 0 10px; flex-shrink: 0;
}
.tab-btn {
  background: none; border: none; border-bottom: 2px solid transparent;
  color: var(--fg-muted); font-size: 11.5px; padding: 7px 12px; cursor: pointer;
  transition: all 0.12s; margin-bottom: -1px; display: flex; align-items: center; gap: 4px;
}
.tab-btn.active { color: var(--accent); border-bottom-color: var(--accent); }
.tab-btn:hover { color: var(--fg); }
.tab-badge {
  background: var(--accent); color: var(--accent-fg); border-radius: 10px;
  font-size: 9px; font-weight: 700; padding: 1px 5px; min-width: 14px; text-align: center;
}
.auth-badge { background: #a6e3a1; color: #1e1e2e; }

/* KV */
.kv-section { flex: 1; overflow-y: auto; padding: 6px 10px; display: flex; flex-direction: column; gap: 3px; }
.kv-head { display: grid; grid-template-columns: 18px 1fr 1fr 22px; gap: 4px; font-size: 10px; font-weight: 700; color: var(--fg-muted); letter-spacing: 0.8px; padding: 2px; }
.kv-row { display: grid; grid-template-columns: 18px 1fr 1fr 22px; gap: 4px; align-items: center; }
.kv-check { width: 14px; height: 14px; accent-color: var(--accent); cursor: pointer; }
.kv-input { background: var(--bg-mid); border: 1px solid var(--border); border-radius: 4px; color: var(--fg); font-size: 11px; font-family: var(--font-mono); padding: 3px 6px; outline: none; height: 22px; }
.kv-input:focus { border-color: var(--accent); }
.kv-del { background: none; border: none; color: var(--fg-muted); font-size: 14px; cursor: pointer; padding: 0; }
.kv-del:hover { color: var(--red); }
.kv-add { background: none; border: none; color: var(--accent); font-size: 11px; cursor: pointer; text-align: left; padding: 6px 2px; }

/* Body */
.body-section { display: flex; flex-direction: column; flex: 1; overflow: hidden; }
.body-type-row { display: flex; gap: 0; padding: 4px 10px; border-bottom: 1px solid var(--border); flex-shrink: 0; }
.body-type-btn { background: none; border: 1px solid transparent; border-radius: 4px; color: var(--fg-muted); font-size: 11px; padding: 3px 10px; cursor: pointer; }
.body-type-btn.active { background: var(--bg-hover); color: var(--fg); border-color: var(--border); }
.body-textarea { flex: 1; background: var(--bg-darkest); border: none; color: var(--fg); font-size: 12px; font-family: var(--font-mono); padding: 10px 12px; resize: none; outline: none; min-height: 0; }
.body-none { padding: 10px 12px; color: var(--fg-muted); font-size: 11px; font-style: italic; }

/* Auth */
.auth-section { flex: 1; overflow-y: auto; padding: 12px 14px; display: flex; flex-direction: column; gap: 12px; }
.auth-type-row { display: flex; align-items: center; gap: 10px; }
.auth-type-label { font-size: 11px; font-weight: 600; color: var(--fg-muted); width: 60px; flex-shrink: 0; }
.auth-type-select {
  flex: 1; background: var(--bg-mid); border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg); font-size: 12px; font-family: var(--font-ui); padding: 5px 10px; outline: none; cursor: pointer;
}
.auth-type-select:focus { border-color: var(--accent); }
.auth-note {
  color: var(--fg-muted); font-size: 11px; font-style: italic;
  background: var(--bg-mid); border-radius: 6px; padding: 10px 12px;
  border: 1px solid var(--border);
}
.auth-note-warn { border-color: #f9e2af44; color: #f9e2af; font-style: normal; }
.auth-field { display: flex; flex-direction: column; gap: 5px; }
.auth-field label { font-size: 11px; font-weight: 600; color: var(--fg-muted); }
.auth-input {
  background: var(--bg-mid); border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg); font-size: 12px; font-family: var(--font-mono); padding: 6px 10px; outline: none;
}
.auth-input:focus { border-color: var(--accent); }
.auth-textarea {
  background: var(--bg-mid); border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg); font-size: 12px; font-family: var(--font-mono); padding: 6px 10px; outline: none;
  resize: vertical; min-height: 80px;
}
.auth-textarea:focus { border-color: var(--accent); }
.auth-radio-row { display: flex; gap: 16px; }
.auth-radio { display: flex; align-items: center; gap: 5px; font-size: 12px; cursor: pointer; color: var(--fg); }
.auth-radio input { accent-color: var(--accent); }
.auth-hint { font-size: 10.5px; color: var(--fg-muted); background: var(--bg-darkest); border-radius: 4px; padding: 6px 8px; }
.auth-hint code { color: var(--accent); font-family: var(--font-mono); }

/* Response header */
.resp-header {
  display: flex; align-items: center; gap: 10px; padding: 7px 12px;
  border-bottom: 1px solid var(--border); flex-shrink: 0; flex-wrap: wrap;
}
.resp-label { font-size: 10px; font-weight: 700; letter-spacing: 1px; color: var(--fg-muted); text-transform: uppercase; }
.resp-status { font-weight: 700; font-size: 12.5px; }
.resp-meta { font-size: 11px; color: var(--fg-muted); }
.resp-meta + .resp-meta::before { content: '·'; margin-right: 6px; color: var(--border); }
.resp-actions { margin-left: auto; display: flex; gap: 4px; }
.resp-copy {
  background: none; border: 1px solid var(--border); border-radius: 4px;
  color: var(--fg-muted); cursor: pointer; padding: 3px 7px; display: flex; align-items: center;
}
.resp-copy:hover { color: var(--accent); border-color: var(--accent); }

/* Response tabs row */
.resp-tab-row {
  display: flex; align-items: center; border-bottom: 1px solid var(--border);
  padding: 0 10px; flex-shrink: 0;
}
.resp-tabs-left { display: flex; }
.resp-tab {
  background: none; border: none; border-bottom: 2px solid transparent;
  color: var(--fg-muted); font-size: 11px; padding: 5px 10px; cursor: pointer;
  margin-bottom: -1px; transition: all 0.12s;
}
.resp-tab.active { color: var(--accent); border-bottom-color: var(--accent); }
.resp-tab:hover { color: var(--fg); }

.resp-error { color: var(--red); padding: 10px 12px; }
.resp-loading { display: flex; align-items: center; gap: 8px; padding: 14px 12px; color: var(--fg-muted); }
.resp-empty { padding: 40px 12px; color: var(--fg-muted); text-align: center; font-style: italic; }
.resp-body { flex: 1; overflow: auto; }
.resp-pre {
  margin: 0; padding: 10px 12px; font-size: 11.5px; font-family: var(--font-mono);
  white-space: pre-wrap; word-break: break-word; color: var(--fg); line-height: 1.6;
}
.resp-pre-raw { color: var(--fg-dim); }

/* JSON syntax highlight */
:deep(.jk)    { color: #89dceb; }
:deep(.js)    { color: #a6e3a1; }
:deep(.jnum)  { color: #fab387; }
:deep(.jb)    { color: #89b4fa; }
:deep(.jnull) { color: #cba6f7; }

/* Preview */
.resp-preview { flex: 1; display: flex; flex-direction: column; overflow: hidden; }
.preview-frame { flex: 1; border: none; background: #fff; }

/* Headers list */
.resp-hdrs-list { flex: 1; overflow: auto; padding: 6px 10px; display: flex; flex-direction: column; gap: 3px; }
.resp-hdr-row { display: grid; grid-template-columns: 220px 1fr; gap: 8px; font-size: 11px; padding: 3px 0; border-bottom: 1px solid rgba(255,255,255,0.04); }
.resp-hdr-key { color: var(--accent); font-family: var(--font-mono); }
.resp-hdr-val { color: var(--fg); font-family: var(--font-mono); word-break: break-all; }

.spinner { display: inline-block; width: 10px; height: 10px; border: 2px solid rgba(255,255,255,.3); border-top-color: #fff; border-radius: 50%; animation: spin .7s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
</style>
