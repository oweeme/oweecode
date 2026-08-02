<script setup lang="ts">
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog'
import { useDatabaseStore, type DbConnection as Connection } from '../composables/useDatabaseStore'
import { useEditorStore } from '../composables/useEditorStore'
import { useI18n } from '../composables/useI18n'

const { t } = useI18n()
const editorStore = useEditorStore()

const emit = defineEmits<{
  'open-table': [connId: string, tableName: string, connName: string, driver: string]
}>()

const { connections, activeConnId, tables, saveConnections } = useDatabaseStore()
const tableFilter = ref('')
const loading = ref(false)
const connError = ref('')
const showAddForm = ref(false)
const editingId = ref<string | null>(null)

const newConn = ref<Connection>({
  id: '', name: '', driver: 'mysql',
  host: 'localhost', port: 3306,
  user: 'root', password: '', database: '',
  filepath: '',
})
const createDbFirst = ref(false)
const creatingDb = ref(false)

function buildUrl(c: Connection): string {
  // mode=rwc (read-write-create) lets SQLite create the file if it doesn't exist yet,
  // so the same "connect" flow works for both an existing file and a brand-new database.
  if (c.driver === 'sqlite') return `sqlite://${c.filepath}?mode=rwc`
  const scheme = c.driver === 'postgres' ? 'postgres' : 'mysql'
  const pass = encodeURIComponent(c.password)
  return `${scheme}://${c.user}:${pass}@${c.host}:${c.port}/${c.database}`
}

async function connect(conn: Connection) {
  loading.value = true
  connError.value = ''
  tables.value = []
  try {
    await invoke('db_connect', { id: conn.id, driver: conn.driver, url: buildUrl(conn) })
    activeConnId.value = conn.id
    tables.value = await invoke<string[]>('db_list_tables', { id: conn.id })
  } catch (e: any) {
    connError.value = String(e)
    activeConnId.value = null
  } finally {
    loading.value = false
  }
}

async function disconnect() {
  if (!activeConnId.value) return
  await invoke('db_disconnect', { id: activeConnId.value }).catch(() => {})
  activeConnId.value = null
  tables.value = []
}

function openTable(table: string) {
  const conn = connections.value.find(c => c.id === activeConnId.value)
  if (!conn) return
  emit('open-table', conn.id, table, conn.name, conn.driver)
}

function removeConnection(id: string) {
  if (activeConnId.value === id) disconnect()
  connections.value = connections.value.filter(c => c.id !== id)
  saveConnections()
}

async function saveConnection() {
  if (editingId.value) {
    const id = editingId.value
    const idx = connections.value.findIndex(c => c.id === id)
    if (idx !== -1) {
      const wasActive = activeConnId.value === id
      const updated = { ...newConn.value, id }
      connections.value[idx] = updated
      saveConnections()
      showAddForm.value = false
      editingId.value = null
      resetForm()
      if (wasActive) {
        await disconnect()
        await connect(updated)
      }
    }
  } else {
    const c = { ...newConn.value, id: `db-${Date.now()}` }
    if (createDbFirst.value && (c.driver === 'mysql' || c.driver === 'postgres') && c.database) {
      creatingDb.value = true
      connError.value = ''
      try {
        await invoke('db_create_database', {
          driver: c.driver, host: c.host, port: c.port,
          user: c.user, password: c.password, dbName: c.database,
        })
      } catch (e: any) {
        connError.value = String(e)
        creatingDb.value = false
        return
      }
      creatingDb.value = false
    }
    connections.value.push(c)
    saveConnections()
    showAddForm.value = false
    resetForm()
    await connect(c)
  }
}

function editConnection(conn: Connection) {
  editingId.value = conn.id
  newConn.value = { ...conn }
  showAddForm.value = true
}

function cancelForm() {
  showAddForm.value = false
  editingId.value = null
  resetForm()
}

function resetForm() {
  newConn.value = { id: '', name: '', driver: 'mysql', host: 'localhost', port: 3306, user: 'root', password: '', database: '', filepath: '' }
  createDbFirst.value = false
}

async function newSqliteFile() {
  const selected = await saveDialog({
    filters: [{ name: 'SQLite Database', extensions: ['db', 'sqlite', 'sqlite3'] }],
    defaultPath: `${(newConn.value.name || 'database').replace(/[^a-z0-9_-]+/gi, '_')}.db`,
  })
  if (selected) newConn.value.filepath = selected
}

async function browseSqliteFile() {
  const selected = await openDialog({
    filters: [{ name: 'SQLite Database', extensions: ['db', 'sqlite', 'sqlite3'] }],
    multiple: false,
  })
  if (typeof selected === 'string') newConn.value.filepath = selected
}

function onDriverChange() {
  if (newConn.value.driver === 'postgres') newConn.value.port = 5432
  else if (newConn.value.driver === 'mysql') newConn.value.port = 3306
}

const filteredTables = computed(() =>
  tableFilter.value
    ? tables.value.filter(t => t.toLowerCase().includes(tableFilter.value.toLowerCase()))
    : tables.value
)

async function refreshTables() {
  if (!activeConnId.value) return
  loading.value = true
  try {
    tables.value = await invoke<string[]>('db_list_tables', { id: activeConnId.value })
  } catch (e: any) { connError.value = String(e) }
  finally { loading.value = false }
}

const exporting = ref(false)
const importing = ref(false)
const dbMsg = ref('')

function activeConn() {
  return connections.value.find(c => c.id === activeConnId.value) ?? null
}

async function exportSql() {
  const conn = activeConn()
  if (!conn) return
  const path = await saveDialog({
    defaultPath: `${conn.name.replace(/[^a-z0-9_-]+/gi, '_')}.sql`,
    filters: [{ name: 'SQL', extensions: ['sql'] }],
  })
  if (!path) return
  exporting.value = true
  connError.value = ''
  try {
    await invoke('db_export', { id: conn.id, driver: conn.driver, tables: null, path })
    dbMsg.value = `${t('exportedTo')} ${path}`
  } catch (e: any) { connError.value = String(e) }
  finally { exporting.value = false }
}

async function importSql() {
  const conn = activeConn()
  if (!conn) return
  const path = await openDialog({
    filters: [{ name: 'SQL', extensions: ['sql'] }],
    multiple: false,
  })
  if (typeof path !== 'string') return
  importing.value = true
  connError.value = ''
  try {
    const n = await invoke<number>('db_import', { id: conn.id, path })
    dbMsg.value = `${n} ${t('statementsExecuted')}`
    await refreshTables()
  } catch (e: any) { connError.value = String(e) }
  finally { importing.value = false }
}

function openErDiagram() {
  const conn = activeConn()
  if (!conn) return
  editorStore.openErDiagram(conn.id, conn.name, conn.driver)
}

function quoteId(driver: string, name: string): string {
  return driver === 'postgres' ? `"${name.replace(/"/g, '""')}"` : `\`${name.replace(/`/g, '``')}\``
}

const tableActionError = ref('')
const renamingTable = ref<string | null>(null)
const renameValue = ref('')

function startRenameTable(tbl: string, e: Event) {
  e.stopPropagation()
  tableActionError.value = ''
  renamingTable.value = tbl
  renameValue.value = tbl
}

function cancelRenameTable() {
  renamingTable.value = null
}

async function confirmRenameTable() {
  const conn = activeConn()
  const oldName = renamingTable.value
  const newName = renameValue.value.trim()
  if (!conn || !oldName) return
  if (!newName || newName === oldName) { renamingTable.value = null; return }
  tableActionError.value = ''
  try {
    const sql = `ALTER TABLE ${quoteId(conn.driver, oldName)} RENAME TO ${quoteId(conn.driver, newName)}`
    await invoke('db_execute', { id: conn.id, sql })
    tables.value = tables.value.map(t => (t === oldName ? newName : t))
    editorStore.tabTableCreated(`db://${conn.id}/${oldName}`, newName)
    window.dispatchEvent(new CustomEvent('db-schema-changed', { detail: { connId: conn.id } }))
    renamingTable.value = null
  } catch (e: any) { tableActionError.value = String(e) }
}

async function deleteTable(tbl: string, e: Event) {
  e.stopPropagation()
  const conn = activeConn()
  if (!conn) return
  if (!confirm(`${t('confirmDeleteTable')} "${tbl}"?`)) return
  tableActionError.value = ''
  try {
    const sql = `DROP TABLE ${quoteId(conn.driver, tbl)}`
    await invoke('db_execute', { id: conn.id, sql })
    tables.value = tables.value.filter(t => t !== tbl)
    editorStore.closeTabByPath(`db://${conn.id}/${tbl}`)
    window.dispatchEvent(new CustomEvent('db-schema-changed', { detail: { connId: conn.id } }))
  } catch (e: any) { tableActionError.value = String(e) }
}

function newTable() {
  const conn = activeConn()
  if (!conn) return
  editorStore.openNewTable(conn.id, conn.driver)
}
</script>

<template>
  <div class="db-panel">
    <!-- Header -->
    <div class="db-header">
      <div class="db-title">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <ellipse cx="12" cy="5" rx="9" ry="3"/>
          <path d="M3 5v4c0 1.66 4.03 3 9 3s9-1.34 9-3V5"/>
          <path d="M3 9v4c0 1.66 4.03 3 9 3s9-1.34 9-3V9"/>
          <path d="M3 13v4c0 1.66 4.03 3 9 3s9-1.34 9-3v-4"/>
        </svg>
        {{ t('database') }}
      </div>
      <div class="db-header-btns">
        <button v-if="activeConnId" class="db-icon-btn" :disabled="exporting" @click="exportSql" :title="t('dbDumpTitle')">
          <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
            <path d="M.5 9.9a.5.5 0 0 1 .5.5v2.5a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-2.5a.5.5 0 0 1 1 0v2.5a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2v-2.5a.5.5 0 0 1 .5-.5z"/>
            <path d="M7.646 10.854a.5.5 0 0 0 .708 0l3-3a.5.5 0 0 0-.708-.708L8.5 9.293V1.5a.5.5 0 0 0-1 0v7.793L5.354 7.146a.5.5 0 1 0-.708.708l3 3z"/>
          </svg>
        </button>
        <button v-if="activeConnId" class="db-icon-btn" :disabled="importing" @click="importSql" :title="t('importSqlTitle')">
          <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
            <path d="M.5 9.9a.5.5 0 0 1 .5.5v2.5a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-2.5a.5.5 0 0 1 1 0v2.5a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2v-2.5a.5.5 0 0 1 .5-.5z"/>
            <path d="M7.646 1.146a.5.5 0 0 1 .708 0l3 3a.5.5 0 0 1-.708.708L8.5 2.707V10.5a.5.5 0 0 1-1 0V2.707L5.354 4.854a.5.5 0 1 1-.708-.708l3-3z"/>
          </svg>
        </button>
        <button v-if="activeConnId" class="db-icon-btn" @click="refreshTables" :title="t('refreshBtn')">
          <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2v1z"/>
            <path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466z"/>
          </svg>
        </button>
        <button v-if="activeConnId" class="db-icon-btn" @click="openErDiagram" :title="t('erDiagramTitle')">
          <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
            <rect x="2" y="3" width="8" height="6" rx="1"/><rect x="14" y="3" width="8" height="6" rx="1"/>
            <rect x="8" y="15" width="8" height="6" rx="1"/>
            <path d="M6 9v3a2 2 0 002 2h1M18 9v3a2 2 0 01-2 2h-1"/>
          </svg>
        </button>
        <button class="db-icon-btn" @click="showAddForm ? cancelForm() : (showAddForm = true)" :title="t('newConnection')">
          {{ showAddForm ? '✕' : '+' }}
        </button>
      </div>
    </div>

    <div v-if="dbMsg || exporting || importing" class="db-msg-bar">
      <span>{{ exporting ? t('exporting') : importing ? t('importing') : dbMsg }}</span>
      <button v-if="!exporting && !importing" @click="dbMsg = ''">×</button>
    </div>

    <!-- Add/edit connection form -->
    <div v-if="showAddForm" class="db-form">
      <div class="form-title">{{ editingId ? t('editConnection') : t('newConnection') }}</div>
      <div class="form-field">
        <label>{{ t('connectionName') }}</label>
        <input v-model="newConn.name" placeholder="My database" class="form-input" />
      </div>
      <div class="form-field">
        <label>{{ t('engine') }}</label>
        <div class="driver-tabs">
          <button v-for="d in ['mysql', 'postgres', 'sqlite']" :key="d"
            class="driver-tab" :class="{ active: newConn.driver === d }"
            @click="newConn.driver = d as any; onDriverChange()">{{ d }}</button>
        </div>
      </div>
      <template v-if="newConn.driver !== 'sqlite'">
        <div class="form-row">
          <div class="form-field" style="flex:1">
            <label>{{ t('host') }}</label>
            <input v-model="newConn.host" class="form-input" placeholder="localhost" />
          </div>
          <div class="form-field" style="width:68px">
            <label>{{ t('port') }}</label>
            <input v-model.number="newConn.port" type="number" class="form-input" />
          </div>
        </div>
        <div class="form-field">
          <label>{{ t('user') }}</label>
          <input v-model="newConn.user" class="form-input" />
        </div>
        <div class="form-field">
          <label>{{ t('password') }}</label>
          <input v-model="newConn.password" type="password" class="form-input" />
        </div>
        <div class="form-field">
          <label>{{ t('fieldDatabase') }}</label>
          <input v-model="newConn.database" class="form-input" />
        </div>
        <label v-if="!editingId" class="form-checkbox-row">
          <input type="checkbox" v-model="createDbFirst" />
          {{ t('createDbIfNotExists') }}
        </label>
      </template>
      <template v-else>
        <div class="form-field">
          <label>{{ t('sqliteFile') }}</label>
          <div class="form-file-row">
            <input v-model="newConn.filepath" class="form-input" placeholder="/path/file.db" style="flex:1" />
            <button class="form-browse-btn" @click="browseSqliteFile" :title="t('openFolder')">…</button>
            <button class="form-browse-btn" @click="newSqliteFile" :title="t('createDbIfNotExists')">+</button>
          </div>
        </div>
      </template>
      <div v-if="creatingDb" class="db-msg-bar" style="margin: 0 0 8px">
        <span>{{ t('creatingDatabase') }}</span>
      </div>
      <div class="form-actions">
        <button class="form-cancel" @click="cancelForm" :disabled="creatingDb">{{ t('cancel') }}</button>
        <button class="form-connect" @click="saveConnection" :disabled="!newConn.name || creatingDb">{{ editingId ? t('save') : t('connect') }}</button>
      </div>
    </div>

    <!-- Error bar -->
    <div v-if="connError" class="db-error-bar">
      <span>{{ connError }}</span>
      <button @click="connError = ''">×</button>
    </div>

    <!-- Connection list -->
    <div class="db-conn-list">
      <div v-if="connections.length === 0 && !showAddForm" class="db-empty">
        <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2" opacity=".25">
          <ellipse cx="12" cy="5" rx="9" ry="3"/>
          <path d="M3 5v4c0 1.66 4.03 3 9 3s9-1.34 9-3V5"/>
          <path d="M3 9v4c0 1.66 4.03 3 9 3s9-1.34 9-3V9"/>
          <path d="M3 13v4c0 1.66 4.03 3 9 3s9-1.34 9-3v-4"/>
        </svg>
        <span>{{ t('clickToConnect') }}</span>
      </div>

      <div v-for="conn in connections" :key="conn.id"
        class="db-conn-item" :class="{ active: conn.id === activeConnId }">
        <span class="conn-dot" :class="conn.id === activeConnId ? 'dot-on' : 'dot-off'" />
        <div class="conn-info" @click="connect(conn)" style="cursor:pointer">
          <span class="conn-name">
            {{ conn.name }}
            <span v-if="conn.id === activeConnId" class="conn-live">● {{ t('connected') }}</span>
          </span>
          <span class="conn-meta">{{ conn.driver }} · {{ conn.driver === 'sqlite' ? conn.filepath.split('/').pop() : conn.database }}</span>
        </div>
        <button class="conn-btn" :title="conn.id === activeConnId ? t('disconnect') : t('connect')"
          @click="conn.id === activeConnId ? disconnect() : connect(conn)">
          {{ conn.id === activeConnId ? '⏏' : '▶' }}
        </button>
        <button class="conn-btn" @click="editConnection(conn)" :title="t('editConnection')">✎</button>
        <button class="conn-btn conn-del" @click="removeConnection(conn.id)" :title="t('delete')">×</button>
      </div>
    </div>

    <!-- Loading -->
    <div v-if="loading" class="db-loading"><span class="db-spinner" /> {{ t('connecting') }}</div>

    <!-- Table tree (when connected) -->
    <template v-if="activeConnId && !loading">
      <div class="section-label section-label-row">
        <span>{{ t('tablesLabel') }} ({{ filteredTables.length }})</span>
        <button class="db-icon-btn section-add-btn" @click="newTable" :title="t('createTableTitle')">+</button>
      </div>
      <div class="table-filter-wrap">
        <svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor" style="color:var(--fg-muted);flex-shrink:0">
          <path d="M11.742 10.344a6.5 6.5 0 10-1.397 1.398h-.001c.03.04.062.078.098.115l3.85 3.85a1 1 0 001.415-1.414l-3.85-3.85a1.007 1.007 0 00-.115-.099zm-5.242 1.656a5.5 5.5 0 110-11 5.5 5.5 0 010 11z"/>
        </svg>
        <input v-model="tableFilter" class="table-filter" :placeholder="t('filter')" />
      </div>
      <div v-if="tableActionError" class="table-action-error">
        {{ tableActionError }}
        <button @click="tableActionError = ''">×</button>
      </div>

      <div class="table-list">
        <div v-for="tbl in filteredTables" :key="tbl" class="table-item" @click="renamingTable === tbl ? null : openTable(tbl)">
          <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" style="flex-shrink:0;color:var(--fg-muted)">
            <rect x="3" y="3" width="18" height="18" rx="1"/>
            <path d="M3 9h18M3 15h18M9 3v18"/>
          </svg>
          <input
            v-if="renamingTable === tbl"
            v-model="renameValue"
            class="table-rename-input"
            autofocus
            @click.stop
            @keydown.enter="confirmRenameTable"
            @keydown.esc="cancelRenameTable"
            @blur="confirmRenameTable"
          />
          <span v-else class="table-item-name">{{ tbl }}</span>
          <div v-if="renamingTable !== tbl" class="table-item-actions">
            <button class="table-item-btn" @click="startRenameTable(tbl, $event)" :title="t('rename')">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
                <path d="M12.854.146a.5.5 0 00-.707 0L10.5 1.793 14.207 5.5l1.647-1.646a.5.5 0 000-.708l-3-3zm.646 6.061L9.793 2.5 3.293 9H3.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.207l6.5-6.5zm-7.468 7.468A.5.5 0 015.5 14H5v-.5a.5.5 0 01-.5-.5H4v-.5a.5.5 0 01-.5-.5H3v-.5a.5.5 0 01-.5-.5H2.5v-.5a.5.5 0 01.646-.473l.853.214 1.832-1.832 3.709 3.709-1.832 1.832.214.853z"/>
              </svg>
            </button>
            <button class="table-item-btn table-item-del" @click="deleteTable(tbl, $event)" :title="t('delete')">
              <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
                <path d="M5.5 5.5A.5.5 0 016 6v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm2.5 0a.5.5 0 01.5.5v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm3 .5a.5.5 0 00-1 0v6a.5.5 0 001 0V6z"/>
                <path fill-rule="evenodd" d="M14.5 3a1 1 0 01-1 1H13v9a2 2 0 01-2 2H5a2 2 0 01-2-2V4h-.5a1 1 0 010-2h4a1 1 0 011-1h2a1 1 0 011 1h4a1 1 0 011 1zM4.118 4L4 4.059V13a1 1 0 001 1h6a1 1 0 001-1V4.059L11.882 4H4.118zM2.5 3a.5.5 0 000 1h11a.5.5 0 000-1h-11z"/>
              </svg>
            </button>
          </div>
        </div>
        <div v-if="filteredTables.length === 0" class="table-none">{{ t('noResults') }}</div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.db-panel {
  display: flex; flex-direction: column;
  height: 100%; background: var(--bg-dark);
  font-family: var(--font-ui); overflow: hidden;
}
.db-header {
  display: flex; align-items: center; justify-content: space-between;
  padding: 8px 12px; border-bottom: 1px solid var(--border);
  background: var(--bg-darkest); flex-shrink: 0;
}
.db-title {
  display: flex; align-items: center; gap: 6px;
  font-size: 10.5px; font-weight: 700; letter-spacing: 0.8px; color: var(--fg-muted);
}
.db-header-btns { display: flex; gap: 4px; }
.db-icon-btn {
  width: 22px; height: 22px; border-radius: 5px;
  background: none; border: 1px solid var(--border);
  color: var(--fg-muted); font-size: 14px; cursor: pointer;
  display: flex; align-items: center; justify-content: center; transition: all 0.12s;
}
.db-icon-btn:hover { background: var(--bg-hover); color: var(--fg); border-color: var(--accent); }
.db-icon-btn:disabled { opacity: 0.4; cursor: default; }
.db-icon-btn:disabled:hover { background: none; border-color: var(--border); }

.db-form {
  background: var(--bg-mid); border-bottom: 1px solid var(--border);
  padding: 12px; flex-shrink: 0;
}
.form-title { font-size: 11px; font-weight: 700; color: var(--fg); margin-bottom: 8px; }
.form-field { margin-bottom: 7px; }
.form-field label { display: block; font-size: 10px; color: var(--fg-muted); margin-bottom: 3px; }
.form-checkbox-row {
  display: flex; align-items: center; gap: 6px;
  font-size: 11px; color: var(--fg-dim); margin-bottom: 8px; cursor: pointer;
}
.form-checkbox-row input { accent-color: var(--accent); cursor: pointer; }
.form-input {
  width: 100%; background: var(--bg-darkest); border: 1px solid var(--border);
  border-radius: 4px; color: var(--fg); font-size: 12px; padding: 4px 7px;
  outline: none; font-family: var(--font-mono);
}
.form-input:focus { border-color: var(--accent); }
.form-row { display: flex; gap: 6px; }
.form-file-row { display: flex; gap: 5px; }
.form-browse-btn {
  background: var(--bg-hover); border: 1px solid var(--border);
  border-radius: 4px; color: var(--fg-dim); padding: 0 8px;
  cursor: pointer; font-size: 13px; flex-shrink: 0;
}
.driver-tabs { display: flex; gap: 4px; }
.driver-tab {
  flex: 1; padding: 4px 0; border: 1px solid var(--border); border-radius: 4px;
  background: none; color: var(--fg-dim); font-size: 11px; cursor: pointer; text-align: center;
}
.driver-tab.active { background: rgba(208,208,216,0.15); border-color: var(--accent); color: var(--accent); font-weight: 600; }
.form-actions { display: flex; gap: 6px; margin-top: 8px; }
.form-cancel {
  flex: 1; background: none; border: 1px solid var(--border);
  color: var(--fg-muted); border-radius: 5px; font-size: 11.5px; padding: 5px; cursor: pointer;
}
.form-connect {
  flex: 2; background: var(--accent); border: none; border-radius: 5px;
  color: var(--accent-fg); font-size: 11.5px; font-weight: 600; padding: 5px; cursor: pointer;
}
.form-connect:disabled { opacity: 0.4; cursor: default; }

.db-error-bar {
  display: flex; align-items: flex-start; justify-content: space-between; gap: 6px;
  background: rgba(248,81,73,0.1); border-bottom: 1px solid rgba(248,81,73,0.3);
  padding: 7px 10px; font-size: 10.5px; color: #f85149; flex-shrink: 0; line-height: 1.4;
}
.db-error-bar button {
  background: none; border: none; color: #f85149; cursor: pointer; font-size: 14px; flex-shrink: 0;
}

.db-msg-bar {
  display: flex; align-items: center; justify-content: space-between; gap: 6px;
  background: rgba(208,208,216,0.08); border-bottom: 1px solid var(--border);
  padding: 6px 10px; font-size: 10.5px; color: var(--fg-dim); flex-shrink: 0;
}
.db-msg-bar button {
  background: none; border: none; color: var(--fg-dim); cursor: pointer; font-size: 13px; flex-shrink: 0;
}

.db-conn-list { flex-shrink: 0; }
.db-empty {
  display: flex; flex-direction: column; align-items: center;
  gap: 6px; padding: 24px 0; color: var(--fg-muted); font-size: 11px;
}
.db-conn-item {
  display: flex; align-items: center; gap: 6px; border-left: 3px solid transparent;
  padding: 6px 10px 6px 7px; border-bottom: 1px solid rgba(255,255,255,0.04); transition: background 0.15s, border-color 0.15s;
}
.db-conn-item:hover { background: var(--bg-hover); }
.db-conn-item.active { background: rgba(166,227,161,0.08); border-left-color: var(--green); }
.db-conn-item.active:hover { background: rgba(166,227,161,0.13); }
.db-conn-item.active .conn-name { color: var(--fg-bright); }
.conn-dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; }
.dot-on  { background: var(--green); box-shadow: 0 0 5px rgba(166,227,161,0.7); }
.dot-off { background: var(--fg-muted); opacity: 0.4; }
.conn-info { flex: 1; min-width: 0; }
.conn-name { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--fg); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.conn-live { font-size: 9px; font-weight: 700; color: var(--green); letter-spacing: 0.3px; flex-shrink: 0; }
.conn-meta { display: block; font-size: 10px; color: var(--fg-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.conn-btn {
  background: none; border: none; color: var(--fg-muted); font-size: 12px;
  cursor: pointer; padding: 2px 5px; border-radius: 3px; transition: all 0.1s; flex-shrink: 0;
}
.conn-btn:hover { color: var(--accent); background: var(--bg-hover); }
.conn-del:hover { color: var(--red); }

.db-loading {
  display: flex; align-items: center; gap: 7px;
  padding: 10px 14px; font-size: 11.5px; color: var(--fg-muted); flex-shrink: 0;
}
.db-spinner {
  width: 12px; height: 12px; border: 2px solid rgba(208,208,216,0.3);
  border-top-color: var(--accent); border-radius: 50%; animation: spin 0.7s linear infinite; display: inline-block;
}
@keyframes spin { to { transform: rotate(360deg); } }

.section-label {
  padding: 8px 12px 4px; font-size: 9.5px; font-weight: 700;
  letter-spacing: 0.8px; color: var(--fg-muted); flex-shrink: 0;
}
.section-label-row {
  display: flex; align-items: center; justify-content: space-between; padding-right: 8px;
}
.section-add-btn {
  width: 18px; height: 18px; font-size: 12px; flex-shrink: 0;
}
.table-filter-wrap {
  display: flex; align-items: center; gap: 7px;
  padding: 4px 10px 6px; flex-shrink: 0;
}
.table-filter {
  flex: 1; background: none; border: none;
  color: var(--fg); font-size: 12px; outline: none; font-family: var(--font-ui);
}
.table-filter::placeholder { color: var(--fg-muted); }
.table-list { flex: 1; overflow-y: auto; }
.table-list::-webkit-scrollbar { width: 3px; }
.table-list::-webkit-scrollbar-thumb { background: var(--bg-active); border-radius: 2px; }
.table-item {
  display: flex; align-items: center; gap: 7px;
  padding: 5px 8px 5px 14px; font-size: 12px; color: var(--fg-dim);
  cursor: pointer; transition: background 0.1s;
}
.table-item:hover { background: var(--bg-hover); color: var(--fg); }
.table-item-name { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.table-rename-input {
  flex: 1; min-width: 0; background: var(--bg-mid); border: 1px solid var(--accent);
  border-radius: 4px; color: var(--fg); font-size: 12px; padding: 2px 5px; outline: none; font-family: var(--font-ui);
}
.table-item-actions { display: flex; gap: 2px; flex-shrink: 0; opacity: 0; transition: opacity 0.1s; }
.table-item:hover .table-item-actions { opacity: 1; }
.table-item-btn {
  width: 20px; height: 20px; background: none; border: none; border-radius: 4px;
  color: var(--fg-muted); cursor: pointer; display: flex; align-items: center; justify-content: center;
}
.table-item-btn:hover { background: var(--bg-active); color: var(--fg); }
.table-item-del:hover { color: #f85149 !important; }
.table-none { padding: 8px 14px; font-size: 11px; color: var(--fg-muted); }
.table-action-error {
  margin: 6px 10px; padding: 6px 10px; background: rgba(248,81,73,0.12);
  border: 1px solid rgba(248,81,73,0.3); border-radius: 6px;
  font-size: 11px; color: #f85149; display: flex; justify-content: space-between; gap: 8px;
}
.table-action-error button { background: none; border: none; color: inherit; cursor: pointer; }
</style>
