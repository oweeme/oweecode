<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '../composables/useI18n'

const { t } = useI18n()
const props = defineProps<{ connId: string; connName: string }>()

interface RedisKeyInfo { key: string; type: string; ttl: number }
type RedisValue =
  | { kind: 'string'; value: string }
  | { kind: 'hash'; fields: [string, string][] }
  | { kind: 'list'; items: string[] }
  | { kind: 'set'; members: string[] }
  | { kind: 'zset'; members: [string, number][] }
  | { kind: 'none' }

const keys = ref<RedisKeyInfo[]>([])
const pattern = ref('*')
const loadingKeys = ref(false)
const dbSize = ref<number | null>(null)
const error = ref('')

const selectedKey = ref<string | null>(null)
const valueData = ref<RedisValue | null>(null)
const loadingValue = ref(false)

const showNewKeyForm = ref(false)
const newKeyName = ref('')
const newKeyType = ref<'string' | 'hash' | 'list' | 'set' | 'zset'>('string')

const stringDraft = ref('')
const newHashField = ref('')
const newHashValue = ref('')
const newListValue = ref('')
const newSetMember = ref('')
const newZMember = ref('')
const newZScore = ref(0)
const ttlDraft = ref<number | null>(null)

const typeColors: Record<string, string> = {
  string: '#79c0ff', hash: '#f7df1e', list: '#a6e3a1', set: '#d2a8ff', zset: '#f38ba8', none: '#5a5a6a',
}

async function loadKeys() {
  loadingKeys.value = true
  error.value = ''
  try {
    keys.value = await invoke<RedisKeyInfo[]>('redis_scan_keys', { id: props.connId, pattern: pattern.value, limit: 1000 })
    dbSize.value = await invoke<number>('redis_db_size', { id: props.connId })
  } catch (e: any) { error.value = String(e) }
  finally { loadingKeys.value = false }
}

async function selectKey(k: string) {
  selectedKey.value = k
  loadingValue.value = true
  error.value = ''
  try {
    valueData.value = await invoke<RedisValue>('redis_get_value', { id: props.connId, key: k })
    if (valueData.value.kind === 'string') stringDraft.value = valueData.value.value
    const info = keys.value.find(x => x.key === k)
    ttlDraft.value = info && info.ttl >= 0 ? info.ttl : null
  } catch (e: any) { error.value = String(e) }
  finally { loadingValue.value = false }
}

async function refreshSelected() {
  if (selectedKey.value) await selectKey(selectedKey.value)
  await loadKeys()
}

async function saveString() {
  if (!selectedKey.value) return
  try {
    await invoke('redis_set_string', { id: props.connId, key: selectedKey.value, value: stringDraft.value })
    await refreshSelected()
  } catch (e: any) { error.value = String(e) }
}

async function deleteKey(k: string) {
  try {
    await invoke('redis_delete_key', { id: props.connId, key: k })
    if (selectedKey.value === k) { selectedKey.value = null; valueData.value = null }
    await loadKeys()
  } catch (e: any) { error.value = String(e) }
}

async function applyTtl() {
  if (!selectedKey.value) return
  try {
    await invoke('redis_set_ttl', { id: props.connId, key: selectedKey.value, seconds: ttlDraft.value ?? -1 })
    await refreshSelected()
  } catch (e: any) { error.value = String(e) }
}

async function addHashField() {
  if (!selectedKey.value || !newHashField.value) return
  try {
    await invoke('redis_hash_set', { id: props.connId, key: selectedKey.value, field: newHashField.value, value: newHashValue.value })
    newHashField.value = ''; newHashValue.value = ''
    await refreshSelected()
  } catch (e: any) { error.value = String(e) }
}

async function editHashField(field: string, value: string) {
  if (!selectedKey.value) return
  try {
    await invoke('redis_hash_set', { id: props.connId, key: selectedKey.value, field, value })
    await refreshSelected()
  } catch (e: any) { error.value = String(e) }
}

async function delHashField(field: string) {
  if (!selectedKey.value) return
  try {
    await invoke('redis_hash_del', { id: props.connId, key: selectedKey.value, field })
    await refreshSelected()
  } catch (e: any) { error.value = String(e) }
}

async function pushListItem(front: boolean) {
  if (!selectedKey.value || !newListValue.value) return
  try {
    await invoke('redis_list_push', { id: props.connId, key: selectedKey.value, value: newListValue.value, front })
    newListValue.value = ''
    await refreshSelected()
  } catch (e: any) { error.value = String(e) }
}

async function editListItem(index: number, value: string) {
  if (!selectedKey.value) return
  try {
    await invoke('redis_list_set', { id: props.connId, key: selectedKey.value, index, value })
    await refreshSelected()
  } catch (e: any) { error.value = String(e) }
}

async function removeListItem(index: number) {
  if (!selectedKey.value) return
  try {
    await invoke('redis_list_remove_index', { id: props.connId, key: selectedKey.value, index })
    await refreshSelected()
  } catch (e: any) { error.value = String(e) }
}

async function addSetMember() {
  if (!selectedKey.value || !newSetMember.value) return
  try {
    await invoke('redis_set_add', { id: props.connId, key: selectedKey.value, member: newSetMember.value })
    newSetMember.value = ''
    await refreshSelected()
  } catch (e: any) { error.value = String(e) }
}

async function removeSetMember(member: string) {
  if (!selectedKey.value) return
  try {
    await invoke('redis_set_remove', { id: props.connId, key: selectedKey.value, member })
    await refreshSelected()
  } catch (e: any) { error.value = String(e) }
}

async function addZMember() {
  if (!selectedKey.value || !newZMember.value) return
  try {
    await invoke('redis_zset_add', { id: props.connId, key: selectedKey.value, member: newZMember.value, score: newZScore.value })
    newZMember.value = ''; newZScore.value = 0
    await refreshSelected()
  } catch (e: any) { error.value = String(e) }
}

async function removeZMember(member: string) {
  if (!selectedKey.value) return
  try {
    await invoke('redis_zset_remove', { id: props.connId, key: selectedKey.value, member })
    await refreshSelected()
  } catch (e: any) { error.value = String(e) }
}

async function createKey() {
  if (!newKeyName.value) return
  try {
    await invoke('redis_create_key', { id: props.connId, key: newKeyName.value, kind: newKeyType.value })
    showNewKeyForm.value = false
    const created = newKeyName.value
    newKeyName.value = ''
    await loadKeys()
    await selectKey(created)
  } catch (e: any) { error.value = String(e) }
}

function fmtTtl(ttl: number): string {
  if (ttl < 0) return '∞'
  if (ttl < 60) return `${ttl}s`
  if (ttl < 3600) return `${Math.round(ttl / 60)}m`
  return `${Math.round(ttl / 3600)}h`
}

const filteredCount = computed(() => keys.value.length)

onMounted(loadKeys)
</script>

<template>
  <div class="redis-view">
    <!-- Key browser -->
    <div class="redis-keys">
      <div class="redis-toolbar">
        <input v-model="pattern" class="redis-search" :placeholder="t('searchPlaceholder')" @keyup.enter="loadKeys" />
        <button class="redis-icon-btn" @click="loadKeys" :title="t('search')">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
            <path d="M11.742 10.344a6.5 6.5 0 10-1.397 1.398h-.001c.03.04.062.078.098.115l3.85 3.85a1 1 0 001.415-1.414l-3.85-3.85a1.007 1.007 0 00-.115-.099zm-5.242 1.656a5.5 5.5 0 110-11 5.5 5.5 0 010 11z"/>
          </svg>
        </button>
        <button class="redis-icon-btn" @click="showNewKeyForm = !showNewKeyForm" :title="t('newKey')">
          {{ showNewKeyForm ? '✕' : '+' }}
        </button>
      </div>

      <div v-if="showNewKeyForm" class="redis-new-key">
        <input v-model="newKeyName" class="redis-input" :placeholder="t('keyNamePlaceholder')" />
        <select v-model="newKeyType" class="redis-input">
          <option value="string">string</option>
          <option value="hash">hash</option>
          <option value="list">list</option>
          <option value="set">set</option>
          <option value="zset">zset</option>
        </select>
        <button class="redis-btn-primary" @click="createKey" :disabled="!newKeyName">{{ t('create') }}</button>
      </div>

      <div class="redis-meta">
        {{ filteredCount }} {{ t('keysLabel') }}{{ dbSize !== null ? ` · ${dbSize} ${t('inTotal')}` : '' }}
        <span v-if="loadingKeys" class="redis-spinner" />
      </div>

      <div class="redis-key-list">
        <div v-if="keys.length === 0 && !loadingKeys" class="redis-empty">{{ t('noKeys') }}</div>
        <div
          v-for="k in keys" :key="k.key"
          class="redis-key-row" :class="{ active: k.key === selectedKey }"
          @click="selectKey(k.key)"
        >
          <span class="redis-type-dot" :style="{ background: typeColors[k.type] ?? typeColors.none }" :title="k.type" />
          <span class="redis-key-name">{{ k.key }}</span>
          <span class="redis-key-ttl">{{ fmtTtl(k.ttl) }}</span>
          <button class="redis-key-del" @click.stop="deleteKey(k.key)" :title="t('delete')">×</button>
        </div>
      </div>
    </div>

    <!-- Value editor -->
    <div class="redis-detail">
      <div v-if="error" class="redis-error">{{ error }} <button @click="error = ''">×</button></div>

      <div v-if="!selectedKey" class="redis-placeholder">{{ t('selectKeyHint') }}</div>

      <template v-else-if="valueData">
        <div class="redis-detail-header">
          <div class="redis-detail-title">
            <span class="redis-type-badge" :style="{ color: typeColors[valueData.kind] ?? typeColors.none }">{{ valueData.kind }}</span>
            <span class="redis-detail-key">{{ selectedKey }}</span>
          </div>
          <div class="redis-ttl-control">
            <label>TTL (s)</label>
            <input v-model.number="ttlDraft" type="number" class="redis-input" style="width:90px" :placeholder="t('noExpiry')" />
            <button class="redis-btn-small" @click="applyTtl">{{ t('apply') }}</button>
          </div>
        </div>

        <div v-if="loadingValue" class="redis-loading">{{ t('loading') }}</div>

        <!-- String -->
        <div v-else-if="valueData.kind === 'string'" class="redis-string-editor">
          <textarea v-model="stringDraft" class="redis-textarea" spellcheck="false" />
          <button class="redis-btn-primary" @click="saveString">{{ t('save') }}</button>
        </div>

        <!-- Hash -->
        <div v-else-if="valueData.kind === 'hash'" class="redis-collection">
          <div class="redis-row redis-row-new">
            <input v-model="newHashField" class="redis-input" :placeholder="t('field')" />
            <input v-model="newHashValue" class="redis-input" :placeholder="t('value')" />
            <button class="redis-btn-small" @click="addHashField">{{ t('add') }}</button>
          </div>
          <div v-for="[f, v] in valueData.fields" :key="f" class="redis-row">
            <span class="redis-row-key">{{ f }}</span>
            <input class="redis-input" :value="v" @change="editHashField(f, ($event.target as HTMLInputElement).value)" />
            <button class="redis-row-del" @click="delHashField(f)">×</button>
          </div>
          <div v-if="valueData.fields.length === 0" class="redis-empty">Hash: {{ t('empty') }}</div>
        </div>

        <!-- List -->
        <div v-else-if="valueData.kind === 'list'" class="redis-collection">
          <div class="redis-row redis-row-new">
            <input v-model="newListValue" class="redis-input" :placeholder="t('newValue')" @keyup.enter="pushListItem(false)" />
            <button class="redis-btn-small" @click="pushListItem(true)">⇤ {{ t('start') }}</button>
            <button class="redis-btn-small" @click="pushListItem(false)">{{ t('end') }} ⇥</button>
          </div>
          <div v-for="(item, i) in valueData.items" :key="i" class="redis-row">
            <span class="redis-row-key">{{ i }}</span>
            <input class="redis-input" :value="item" @change="editListItem(i, ($event.target as HTMLInputElement).value)" />
            <button class="redis-row-del" @click="removeListItem(i)">×</button>
          </div>
          <div v-if="valueData.items.length === 0" class="redis-empty">List: {{ t('empty') }}</div>
        </div>

        <!-- Set -->
        <div v-else-if="valueData.kind === 'set'" class="redis-collection">
          <div class="redis-row redis-row-new">
            <input v-model="newSetMember" class="redis-input" :placeholder="t('newMember')" @keyup.enter="addSetMember" />
            <button class="redis-btn-small" @click="addSetMember">{{ t('add') }}</button>
          </div>
          <div v-for="m in valueData.members" :key="m" class="redis-row">
            <span class="redis-row-key" style="flex:1">{{ m }}</span>
            <button class="redis-row-del" @click="removeSetMember(m)">×</button>
          </div>
          <div v-if="valueData.members.length === 0" class="redis-empty">Set: {{ t('empty') }}</div>
        </div>

        <!-- ZSet -->
        <div v-else-if="valueData.kind === 'zset'" class="redis-collection">
          <div class="redis-row redis-row-new">
            <input v-model="newZMember" class="redis-input" :placeholder="t('member')" @keyup.enter="addZMember" />
            <input v-model.number="newZScore" type="number" class="redis-input" style="width:80px" placeholder="score" />
            <button class="redis-btn-small" @click="addZMember">{{ t('add') }}</button>
          </div>
          <div v-for="[m, s] in valueData.members" :key="m" class="redis-row">
            <span class="redis-row-key" style="flex:1">{{ m }}</span>
            <span class="redis-zscore">{{ s }}</span>
            <button class="redis-row-del" @click="removeZMember(m)">×</button>
          </div>
          <div v-if="valueData.members.length === 0" class="redis-empty">ZSet: {{ t('empty') }}</div>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.redis-view {
  display: flex; height: 100%; overflow: hidden;
  font-family: var(--font-ui); background: var(--bg-darkest);
}

/* Key browser */
.redis-keys {
  width: 280px; flex-shrink: 0; display: flex; flex-direction: column;
  border-right: 1px solid var(--border); background: var(--bg-dark); overflow: hidden;
}
.redis-toolbar { display: flex; gap: 4px; padding: 8px; flex-shrink: 0; }
.redis-search {
  flex: 1; background: var(--bg-darkest); border: 1px solid var(--border);
  border-radius: 4px; color: var(--fg); font-size: 12px; padding: 4px 7px;
  outline: none; font-family: var(--font-mono);
}
.redis-search:focus { border-color: var(--accent); }
.redis-icon-btn {
  width: 24px; height: 24px; border-radius: 5px; flex-shrink: 0;
  background: none; border: 1px solid var(--border); color: var(--fg-muted);
  font-size: 14px; cursor: pointer; display: flex; align-items: center; justify-content: center;
}
.redis-icon-btn:hover { background: var(--bg-hover); color: var(--fg); border-color: var(--accent); }

.redis-new-key { display: flex; flex-direction: column; gap: 5px; padding: 0 8px 8px; }

.redis-meta {
  padding: 2px 10px 6px; font-size: 10px; color: var(--fg-muted);
  display: flex; align-items: center; gap: 6px; flex-shrink: 0;
}
.redis-spinner {
  width: 10px; height: 10px; border: 2px solid rgba(208,208,216,0.3);
  border-top-color: var(--accent); border-radius: 50%; animation: rspin 0.7s linear infinite; display: inline-block;
}
@keyframes rspin { to { transform: rotate(360deg); } }

.redis-key-list { flex: 1; overflow-y: auto; }
.redis-key-list::-webkit-scrollbar { width: 3px; }
.redis-key-list::-webkit-scrollbar-thumb { background: var(--bg-active); border-radius: 2px; }
.redis-empty { padding: 16px 12px; font-size: 11px; color: var(--fg-muted); text-align: center; }

.redis-key-row {
  display: flex; align-items: center; gap: 6px;
  padding: 5px 10px; cursor: pointer; font-size: 12px; color: var(--fg-dim);
  border-bottom: 1px solid rgba(255,255,255,0.03);
}
.redis-key-row:hover { background: var(--bg-hover); }
.redis-key-row.active { background: rgba(208,208,216,0.1); color: var(--fg); }
.redis-type-dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; }
.redis-key-name {
  flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  font-family: var(--font-mono); font-size: 11.5px;
}
.redis-key-ttl { font-size: 9.5px; color: var(--fg-muted); flex-shrink: 0; }
.redis-key-del {
  background: none; border: none; color: var(--fg-muted); cursor: pointer;
  font-size: 13px; flex-shrink: 0; opacity: 0; transition: opacity 0.1s;
}
.redis-key-row:hover .redis-key-del { opacity: 1; }
.redis-key-del:hover { color: var(--red); }

/* Detail */
.redis-detail { flex: 1; overflow-y: auto; padding: 14px 18px; min-width: 0; }
.redis-placeholder { color: var(--fg-muted); font-size: 12.5px; padding: 30px 0; text-align: center; }
.redis-loading { color: var(--fg-muted); font-size: 12px; padding: 10px 0; }

.redis-error {
  display: flex; align-items: center; justify-content: space-between; gap: 8px;
  background: rgba(248,81,73,0.1); border: 1px solid rgba(248,81,73,0.3);
  border-radius: 6px; padding: 7px 10px; font-size: 11.5px; color: #f85149; margin-bottom: 10px;
}
.redis-error button { background: none; border: none; color: #f85149; cursor: pointer; font-size: 14px; }

.redis-detail-header {
  display: flex; align-items: center; justify-content: space-between;
  gap: 12px; margin-bottom: 12px; flex-wrap: wrap;
}
.redis-detail-title { display: flex; align-items: center; gap: 8px; min-width: 0; }
.redis-type-badge {
  font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.5px;
  border: 1px solid currentColor; border-radius: 4px; padding: 2px 6px; flex-shrink: 0;
}
.redis-detail-key {
  font-family: var(--font-mono); font-size: 13.5px; color: var(--fg-bright);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.redis-ttl-control { display: flex; align-items: center; gap: 6px; flex-shrink: 0; }
.redis-ttl-control label { font-size: 10.5px; color: var(--fg-muted); }

.redis-input {
  background: var(--bg-mid); border: 1px solid var(--border); border-radius: 4px;
  color: var(--fg); font-size: 12px; padding: 4px 7px; outline: none; font-family: var(--font-mono);
}
.redis-input:focus { border-color: var(--accent); }

.redis-btn-primary {
  background: var(--accent); border: none; border-radius: 5px; color: var(--accent-fg);
  font-size: 12px; font-weight: 600; padding: 5px 12px; cursor: pointer;
}
.redis-btn-primary:disabled { opacity: 0.4; cursor: default; }
.redis-btn-small {
  background: var(--bg-hover); border: 1px solid var(--border); border-radius: 4px;
  color: var(--fg-dim); font-size: 11px; padding: 4px 9px; cursor: pointer; white-space: nowrap;
}
.redis-btn-small:hover { border-color: var(--accent); color: var(--fg); }

.redis-string-editor { display: flex; flex-direction: column; gap: 8px; }
.redis-textarea {
  min-height: 240px; background: var(--bg-mid); border: 1px solid var(--border);
  border-radius: 6px; color: var(--fg); font-family: var(--font-mono); font-size: 12.5px;
  padding: 10px; outline: none; resize: vertical;
}
.redis-textarea:focus { border-color: var(--accent); }

.redis-collection { display: flex; flex-direction: column; gap: 4px; }
.redis-row { display: flex; align-items: center; gap: 6px; }
.redis-row .redis-input { flex: 1; }
.redis-row-new { padding-bottom: 6px; margin-bottom: 4px; border-bottom: 1px solid var(--border); }
.redis-row-key {
  font-family: var(--font-mono); font-size: 11.5px; color: var(--fg-muted);
  width: 90px; flex-shrink: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.redis-zscore { font-family: var(--font-mono); font-size: 11px; color: var(--fg-muted); flex-shrink: 0; }
.redis-row-del {
  background: none; border: none; color: var(--fg-muted); cursor: pointer;
  font-size: 15px; flex-shrink: 0; padding: 0 4px;
}
.redis-row-del:hover { color: var(--red); }
</style>
