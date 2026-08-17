<script setup lang="ts">
import { ref } from 'vue'
import { useEditorStore } from '../composables/useEditorStore'
import { useI18n } from '../composables/useI18n'

const { t } = useI18n()

export interface ApiKV { key: string; value: string; enabled: boolean }
export interface ApiAuth {
  type: 'no-auth' | 'api-key' | 'bearer' | 'jwt' | 'basic' | 'digest' | 'oauth1' | 'oauth2'
  apiKeyName?: string
  apiKeyValue?: string
  apiKeyIn?: 'header' | 'query'
  token?: string
  username?: string
  password?: string
}
export interface ApiRequest {
  id: string
  name: string
  method: string
  url: string
  headers: ApiKV[]
  params: ApiKV[]
  bodyType: 'none' | 'json' | 'raw'
  body: string
  auth: ApiAuth
}
export interface ApiCollection {
  id: string
  name: string
  color: string
  expanded: boolean
  requests: ApiRequest[]
}

const COLORS = ['#89b4fa','#cba6f7','#a6e3a1','#f9e2af','#f38ba8','#89dceb','#fab387']

const store = useEditorStore()

function loadCollections(): ApiCollection[] {
  try { return JSON.parse(localStorage.getItem('api_collections') ?? '[]') } catch { return [] }
}
function saveCollections() {
  localStorage.setItem('api_collections', JSON.stringify(collections.value))
}

const collections = ref<ApiCollection[]>(loadCollections())

// Add collection modal
const addCollModal = ref(false)
const newCollName = ref('')

function addCollection() {
  if (!newCollName.value.trim()) return
  collections.value.push({
    id: crypto.randomUUID(),
    name: newCollName.value.trim(),
    color: COLORS[collections.value.length % COLORS.length],
    expanded: true,
    requests: [],
  })
  saveCollections()
  newCollName.value = ''
  addCollModal.value = false
}

// Context menu
const ctx = ref<{ x: number; y: number; type: 'coll' | 'req'; collId: string; reqId?: string } | null>(null)
function openCtx(e: MouseEvent, type: 'coll' | 'req', collId: string, reqId?: string) {
  e.preventDefault(); e.stopPropagation()
  ctx.value = { x: e.clientX, y: e.clientY, type, collId, reqId }
}
function closeCtx() { ctx.value = null }

// Rename inline
const renaming = ref<{ collId: string; reqId?: string; name: string } | null>(null)
function startRename() {
  if (!ctx.value) return
  const { collId, reqId } = ctx.value
  closeCtx()
  if (reqId) {
    const coll = collections.value.find(c => c.id === collId)
    const req = coll?.requests.find(r => r.id === reqId)
    if (req) renaming.value = { collId, reqId, name: req.name }
  } else {
    const coll = collections.value.find(c => c.id === collId)
    if (coll) renaming.value = { collId, name: coll.name }
  }
}
function confirmRename() {
  if (!renaming.value) return
  const { collId, reqId, name } = renaming.value
  const coll = collections.value.find(c => c.id === collId)
  if (!coll) { renaming.value = null; return }
  if (reqId) {
    const req = coll.requests.find(r => r.id === reqId)
    if (req) req.name = name.trim() || req.name
  } else {
    coll.name = name.trim() || coll.name
  }
  saveCollections()
  renaming.value = null
}

function deleteItem() {
  if (!ctx.value) return
  const { collId, reqId } = ctx.value
  closeCtx()
  const coll = collections.value.find(c => c.id === collId)
  if (!coll) return
  if (reqId) {
    coll.requests = coll.requests.filter(r => r.id !== reqId)
  } else {
    collections.value = collections.value.filter(c => c.id !== collId)
  }
  saveCollections()
}

function defaultRequest(_collId: string): ApiRequest {
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

function newRequest(collId: string) {
  const coll = collections.value.find(c => c.id === collId)
  if (!coll) return
  const req = defaultRequest(collId)
  coll.requests.push(req)
  coll.expanded = true
  saveCollections()
  openRequest(req)
}

function newRequestFromCtx() {
  if (!ctx.value) return
  const collId = ctx.value.collId
  closeCtx()
  newRequest(collId)
}

function openRequest(req: ApiRequest) {
  store.openApiRequest(req.id, req.name, JSON.stringify(req))
}

function duplicateRequest() {
  if (!ctx.value) return
  const { collId, reqId } = ctx.value
  closeCtx()
  const coll = collections.value.find(c => c.id === collId)
  const req = coll?.requests.find(r => r.id === reqId)
  if (!req || !coll) return
  const copy = { ...JSON.parse(JSON.stringify(req)), id: crypto.randomUUID(), name: req.name + ' (copy)' }
  coll.requests.push(copy)
  saveCollections()
  openRequest(copy)
}

// Expose saveRequest for ApiRequestTab to call
function saveRequest(req: ApiRequest, _collId?: string) {
  for (const coll of collections.value) {
    const idx = coll.requests.findIndex(r => r.id === req.id)
    if (idx !== -1) {
      coll.requests[idx] = req
      saveCollections()
      return
    }
  }
  // Not found — save to first collection or create one
  if (collections.value.length === 0) {
    collections.value.push({ id: crypto.randomUUID(), name: 'My Requests', color: COLORS[0], expanded: true, requests: [req] })
  } else {
    collections.value[0].requests.push(req)
  }
  saveCollections()
}

defineExpose({ saveRequest, collections })

const methodColor: Record<string, string> = {
  GET:'#a6e3a1', POST:'#89b4fa', PUT:'#f9e2af', PATCH:'#cba6f7',
  DELETE:'#f38ba8', HEAD:'#89dceb', OPTIONS:'#585b70'
}
</script>

<template>
  <div class="api-coll" @click="closeCtx" @keydown.esc="closeCtx">
    <div class="coll-header">
      <span class="coll-title">{{ t('api') }}</span>
      <button class="hdr-btn" :title="t('newCollection')" @click.stop="addCollModal = true">
        <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor">
          <path d="M8 4a.5.5 0 01.5.5v3h3a.5.5 0 010 1h-3v3a.5.5 0 01-1 0v-3h-3a.5.5 0 010-1h3v-3A.5.5 0 018 4z"/>
        </svg>
      </button>
    </div>

    <div class="coll-list">
      <div v-if="collections.length === 0" class="coll-empty">
        <p>{{ t('noCollectionsYet') }}</p>
        <button class="coll-new-btn" @click="addCollModal = true">+ {{ t('newCollection') }}</button>
      </div>

      <div v-for="coll in collections" :key="coll.id" class="coll-group">
        <!-- Collection row -->
        <div
          class="coll-row"
          @click="coll.expanded = !coll.expanded"
          @contextmenu="openCtx($event, 'coll', coll.id)"
        >
          <svg class="coll-arrow" :class="{ expanded: coll.expanded }" width="9" height="9" viewBox="0 0 9 9" fill="currentColor">
            <path d="M2.5 0l4 4.5-4 4z"/>
          </svg>
          <span class="coll-dot" :style="{ background: coll.color }" />
          <template v-if="renaming?.collId === coll.id && !renaming.reqId">
            <input
              class="rename-input"
              v-model="renaming.name"
              @keydown.enter="confirmRename"
              @keydown.esc="renaming = null"
              @click.stop
              autofocus
            />
          </template>
          <span v-else class="coll-name">{{ coll.name }}</span>
          <button class="coll-add-req" :title="t('addRequestTitle')" @click.stop="newRequest(coll.id)">+</button>
        </div>

        <!-- Requests -->
        <div v-if="coll.expanded" class="req-list">
          <div
            v-for="req in coll.requests"
            :key="req.id"
            class="req-row"
            @click="openRequest(req)"
            @contextmenu="openCtx($event, 'req', coll.id, req.id)"
          >
            <span class="req-method" :style="{ color: methodColor[req.method] ?? '#9da5b4' }">{{ req.method }}</span>
            <template v-if="renaming?.collId === coll.id && renaming.reqId === req.id">
              <input
                class="rename-input"
                v-model="renaming.name"
                @keydown.enter="confirmRename"
                @keydown.esc="renaming = null"
                @click.stop
                autofocus
              />
            </template>
            <span v-else class="req-name">{{ req.name }}</span>
          </div>
          <div v-if="coll.requests.length === 0" class="req-empty">{{ t('noRequestsYet') }}</div>
        </div>
      </div>
    </div>

    <!-- Add collection modal -->
    <Teleport to="body">
      <div v-if="addCollModal" class="modal-overlay" @click.self="addCollModal = false">
        <div class="modal-box">
          <div class="modal-title">{{ t('newCollection') }}</div>
          <input
            v-model="newCollName"
            class="modal-input"
            placeholder="e.g. Empresa ABC, Backend API..."
            @keydown.enter="addCollection"
            @keydown.esc="addCollModal = false"
            autofocus
          />
          <div class="modal-actions">
            <button class="modal-cancel" @click="addCollModal = false">{{ t('cancel') }}</button>
            <button class="modal-confirm" @click="addCollection">{{ t('create') }}</button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Context menu -->
    <Teleport to="body">
      <div v-if="ctx" class="ctx-menu" :style="{ left: ctx.x + 'px', top: ctx.y + 'px' }" @click.stop>
        <template v-if="ctx.type === 'coll'">
          <button class="ctx-item" @click="newRequestFromCtx">+ {{ t('newRequest2') }}</button>
          <button class="ctx-item" @click="startRename">{{ t('rename') }}</button>
          <div class="ctx-sep" />
          <button class="ctx-item ctx-danger" @click="deleteItem">{{ t('deleteCollection') }}</button>
        </template>
        <template v-else>
          <button class="ctx-item" @click="duplicateRequest">{{ t('duplicate') }}</button>
          <button class="ctx-item" @click="startRename">{{ t('rename') }}</button>
          <div class="ctx-sep" />
          <button class="ctx-item ctx-danger" @click="deleteItem">{{ t('delete') }}</button>
        </template>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.api-coll {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
  font-family: var(--font-ui);
  color: var(--fg);
  font-size: 12px;
}

.coll-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 8px 4px 14px;
  flex-shrink: 0;
}
.coll-title { font-size: 10px; font-weight: 700; letter-spacing: 1.5px; color: var(--fg-muted); }
.hdr-btn {
  width: 24px; height: 24px; background: none; border: none;
  color: var(--fg-muted); border-radius: 4px; cursor: pointer;
  display: flex; align-items: center; justify-content: center;
}
.hdr-btn:hover { background: var(--bg-hover); color: var(--fg); }

.coll-list { flex: 1; overflow-y: auto; padding: 4px 0; }
.coll-empty { padding: 20px 14px; text-align: center; color: var(--fg-muted); }
.coll-empty p { margin: 0 0 10px; font-size: 12px; }
.coll-new-btn {
  background: var(--accent); border: none; border-radius: 6px;
  color: var(--accent-fg); font-size: 12px; font-weight: 600; padding: 6px 14px; cursor: pointer;
}

.coll-group { margin-bottom: 2px; }

.coll-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 8px 5px 10px;
  cursor: pointer;
  border-radius: 4px;
  margin: 1px 4px;
  user-select: none;
}
.coll-row:hover { background: var(--bg-hover); }
.coll-arrow { color: var(--fg-muted); flex-shrink: 0; transition: transform 0.15s; }
.coll-arrow.expanded { transform: rotate(90deg); }
.coll-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
.coll-name { flex: 1; font-size: 12px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.coll-add-req {
  background: none; border: none; color: var(--fg-muted); font-size: 16px;
  cursor: pointer; padding: 0 2px; line-height: 1; opacity: 0;
  transition: opacity 0.1s;
}
.coll-row:hover .coll-add-req { opacity: 1; }
.coll-add-req:hover { color: var(--accent); }

.req-list { padding-left: 24px; }
.req-empty { padding: 4px 10px; font-size: 11px; color: var(--fg-muted); font-style: italic; }

.req-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 8px;
  border-radius: 4px;
  margin: 1px 4px 1px 0;
  cursor: pointer;
  user-select: none;
}
.req-row:hover { background: var(--bg-hover); }
.req-method { font-size: 10px; font-weight: 700; font-family: var(--font-mono); width: 48px; flex-shrink: 0; }
.req-name { flex: 1; font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--fg); }

.rename-input {
  flex: 1; background: var(--bg-darkest); border: 1px solid var(--accent);
  border-radius: 3px; color: var(--fg); font-size: 12px; font-family: var(--font-ui);
  padding: 1px 6px; outline: none; height: 20px;
}

/* Modal */
.modal-overlay {
  position: fixed; inset: 0; z-index: 99999;
  background: rgba(0,0,0,0.6); display: flex;
  align-items: center; justify-content: center;
  backdrop-filter: blur(2px);
}
.modal-box {
  background: var(--bg-panel); border: 1px solid var(--border);
  border-radius: 10px; padding: 20px; width: 300px;
  box-shadow: 0 16px 48px rgba(0,0,0,0.6);
}
.modal-title { font-size: 13px; font-weight: 700; color: var(--fg-bright); margin-bottom: 12px; }
.modal-input {
  width: 100%; box-sizing: border-box;
  background: var(--bg-mid); border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg); font-size: 12px; padding: 7px 10px; outline: none;
  font-family: var(--font-ui); margin-bottom: 14px;
}
.modal-input:focus { border-color: var(--accent); }
.modal-actions { display: flex; gap: 8px; }
.modal-cancel {
  flex: 1; background: var(--bg-hover); border: 1px solid var(--border);
  border-radius: 6px; color: var(--fg); font-size: 12px; font-weight: 600;
  padding: 7px; cursor: pointer;
}
.modal-confirm {
  flex: 1; background: var(--accent); border: none; border-radius: 6px;
  color: var(--accent-fg); font-size: 12px; font-weight: 700; padding: 7px; cursor: pointer;
}
</style>

<style>
/* context menu global */
.ctx-menu { position:fixed; background:var(--bg-panel); border:1px solid var(--border); border-radius:7px; padding:4px; z-index:9999; min-width:150px; box-shadow:0 8px 24px rgba(0,0,0,.5); }
.ctx-item { display:flex; align-items:center; gap:8px; width:100%; background:none; border:none; color:var(--fg); font-size:12px; font-family:var(--font-ui); padding:6px 10px; border-radius:5px; cursor:pointer; text-align:left; transition:background .1s; }
.ctx-item:hover { background:var(--bg-hover); }
.ctx-danger { color:var(--red); }
.ctx-danger:hover { background:rgba(243,139,168,.1); }
.ctx-sep { height:1px; background:var(--border); margin:4px 0; }
</style>
