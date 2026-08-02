<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount, computed, nextTick } from 'vue'

const _tablesCache: Record<string, string[]> = {}
import { invoke } from '@tauri-apps/api/core'
import { EditorView, keymap } from '@codemirror/view'
import { EditorState as CMState, Compartment } from '@codemirror/state'
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands'
import { syntaxHighlighting, defaultHighlightStyle } from '@codemirror/language'
import { autocompletion, completionKeymap } from '@codemirror/autocomplete'
import { sql, MySQL, PostgreSQL, SQLite } from '@codemirror/lang-sql'
import { oneDark } from '@codemirror/theme-one-dark'
import { useAppTheme } from '../composables/useAppTheme'
import { lightCmTheme } from '../composables/editorThemes'
import { useI18n } from '../composables/useI18n'
import { useEditorStore } from '../composables/useEditorStore'
import { useDatabaseStore } from '../composables/useDatabaseStore'

const { t } = useI18n()
const editorStore = useEditorStore()
const { tables: sidebarTables, activeConnId: sidebarActiveConnId } = useDatabaseStore()
const { theme: appTheme } = useAppTheme()
function cmThemeFor(t: string) {
  return t === 'white' ? lightCmTheme() : oneDark
}
const dbThemeCompartment = new Compartment()

interface Props { connId: string; tableName: string; driver: string }
interface QueryResult { columns: string[]; rows: Array<Array<string | null>> }
interface SavedQuery  { id: string; name: string; sql: string; ts: number; pinned?: boolean }
interface ColDef      { name: string; type: string; nullable: boolean; pk: boolean; ai: boolean; default_: string }
interface FkDef       { column: string; refTable: string; refColumn: string; constraintName: string }

const props = defineProps<Props>()

// ── views ──────────────────────────────────────────────────────────────────────
const activeView = ref<'data' | 'estructura' | 'query'>('data')

// ── data view ──────────────────────────────────────────────────────────────────
const tableData    = ref<QueryResult | null>(null)
const tableError   = ref('')
const loadingTable = ref(false)
const rowLimit     = ref(200)

// ── editing ────────────────────────────────────────────────────────────────────
const editingRow   = ref<number | null>(null)
const editValues   = ref<Record<string, string>>({})
const editError    = ref('')
const pkColumn     = ref<string | null>(null)

// ── insert modal ───────────────────────────────────────────────────────────────
const insertModal  = ref(false)
const insertValues = ref<Record<string, string>>({})
const insertError  = ref('')
const insertLoading = ref(false)
const insertHashAlgo = ref<Record<string, string>>({})
const HASH_ALGOS = ['none', 'md5', 'sha1', 'sha256', 'sha512', 'bcrypt', 'argon2'] as const
const HASH_ALGO_LABELS: Record<string, string> = {
  none: 'plainText', md5: 'MD5', sha1: 'SHA-1', sha256: 'SHA-256', sha512: 'SHA-512', bcrypt: 'Bcrypt', argon2: 'Argon2',
}

// ── structure view ─────────────────────────────────────────────────────────────
const structData    = ref<QueryResult | null>(null)
const structLoading = ref(false)
const structError   = ref('')

// ── create table modal ─────────────────────────────────────────────────────────
const createModal   = ref(false)
const newTableName  = ref('')
const newTableCols  = ref<ColDef[]>([
  { name: 'id', type: 'INT', nullable: false, pk: true, ai: true, default_: '' },
])
const createError   = ref('')
const createLoading = ref(false)
const COLUMN_TYPES  = ['INT','BIGINT','TINYINT','SMALLINT','FLOAT','DOUBLE','DECIMAL(10,2)','VARCHAR(255)','VARCHAR(100)','TEXT','LONGTEXT','DATE','DATETIME','TIMESTAMP','BOOLEAN','BLOB']
const newTableFks   = ref<FkDef[]>([])
const fkRefTableCols = ref<Record<string, string[]>>({})

// ── query view ─────────────────────────────────────────────────────────────────
const queryResult   = ref<QueryResult | null>(null)
const queryError    = ref('')
const loadingQuery  = ref(false)
const queryEditorEl = ref<HTMLElement | null>(null)
let queryView: EditorView | null = null
const allTables     = ref<string[]>([])
const editorHeight  = ref(140)
const isResizing    = ref(false)
let resizeStartY = 0
let resizeStartH = 0
const showHistory   = ref(false)
const savedQueries  = ref<SavedQuery[]>([])
const saveModal     = ref(false)
const saveName      = ref('')

// ── helpers ────────────────────────────────────────────────────────────────────
const storageKey = computed(() => `db_saved_queries_${props.connId}`)

function escapeVal(val: string | null | undefined): string {
  if (val == null || val === '' || val.toUpperCase() === 'NULL') return 'NULL'
  return `'${val.replace(/\\/g, '\\\\').replace(/'/g, "\\'")}'`
}

function quoteId(name: string): string {
  // Double up embedded quote chars so an identifier containing one can't break
  // out of the quoting and inject arbitrary SQL into a generated statement
  return props.driver === 'postgres' ? `"${name.replace(/"/g, '""')}"` : `\`${name.replace(/`/g, '``')}\``
}

function quoteTable(name: string): string { return quoteId(name) }

// ── rename / delete current table ────────────────────────────────────────────
const renamingCurrentTable = ref(false)
const renameCurrentValue   = ref('')
const tableActionError     = ref('')

function startRenameCurrentTable() {
  tableActionError.value = ''
  renameCurrentValue.value = props.tableName
  renamingCurrentTable.value = true
}

function cancelRenameCurrentTable() { renamingCurrentTable.value = false }

async function confirmRenameCurrentTable() {
  const newName = renameCurrentValue.value.trim()
  if (!newName || newName === props.tableName) { renamingCurrentTable.value = false; return }
  tableActionError.value = ''
  try {
    const sql = `ALTER TABLE ${quoteId(props.tableName)} RENAME TO ${quoteId(newName)}`
    await invoke('db_execute', { id: props.connId, sql })
    if (sidebarActiveConnId.value === props.connId) {
      sidebarTables.value = sidebarTables.value.map(t => (t === props.tableName ? newName : t))
    }
    editorStore.tabTableCreated(`db://${props.connId}/${props.tableName}`, newName)
    delete _tablesCache[props.connId]
    window.dispatchEvent(new CustomEvent('db-schema-changed', { detail: { connId: props.connId } }))
    renamingCurrentTable.value = false
  } catch (e: any) { tableActionError.value = String(e) }
}

async function deleteCurrentTable() {
  if (!confirm(`${t('confirmDeleteTable')} "${props.tableName}"?`)) return
  tableActionError.value = ''
  try {
    const sql = `DROP TABLE ${quoteId(props.tableName)}`
    await invoke('db_execute', { id: props.connId, sql })
    if (sidebarActiveConnId.value === props.connId) {
      sidebarTables.value = sidebarTables.value.filter(t => t !== props.tableName)
    }
    delete _tablesCache[props.connId]
    window.dispatchEvent(new CustomEvent('db-schema-changed', { detail: { connId: props.connId } }))
    editorStore.closeTabByPath(`db://${props.connId}/${props.tableName}`)
  } catch (e: any) { tableActionError.value = String(e) }
}

function formatTs(ts: number): string {
  const d = new Date(ts), pad = (n: number) => String(n).padStart(2, '0')
  return `${pad(d.getDate())}/${pad(d.getMonth()+1)} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

// ── load table data ────────────────────────────────────────────────────────────
async function loadTable() {
  if (!props.tableName) { tableData.value = null; return }
  loadingTable.value = true
  tableError.value = ''
  editingRow.value  = null
  editError.value   = ''
  try {
    const q = `SELECT * FROM ${quoteTable(props.tableName)} LIMIT ${rowLimit.value}`
    tableData.value = await invoke<QueryResult>('db_query', { id: props.connId, sql: q })
  } catch (e: any) { tableError.value = String(e) }
  finally { loadingTable.value = false }
}

// ── structure ──────────────────────────────────────────────────────────────────
async function loadStructure() {
  if (!props.tableName) { structData.value = null; return }
  structLoading.value = true
  structError.value = ''
  try {
    structData.value = await invoke<QueryResult>('db_describe_table', {
      id: props.connId, table: props.tableName, driver: props.driver
    })
    detectPk()
  } catch (e: any) { structError.value = String(e) }
  finally { structLoading.value = false }
}

function detectPk() {
  if (!structData.value) return
  const cols = structData.value.columns.map(c => c.toLowerCase())
  // MySQL DESCRIBE: Field, Type, Null, Key, Default, Extra
  const keyIdx   = cols.indexOf('key')
  const fieldIdx = cols.indexOf('field')
  if (keyIdx !== -1 && fieldIdx !== -1) {
    const pkRow = structData.value.rows.find(r => r[keyIdx] === 'PRI')
    pkColumn.value = pkRow ? (pkRow[fieldIdx] ?? null) : null
    return
  }
  // SQLite PRAGMA: cid, name, type, notnull, dflt_value, pk
  const pkColIdx  = cols.indexOf('pk')
  const nameIdx   = cols.indexOf('name')
  if (pkColIdx !== -1 && nameIdx !== -1) {
    const pkRow = structData.value.rows.find(r => r[pkColIdx] !== '0' && r[pkColIdx] !== null)
    pkColumn.value = pkRow ? (pkRow[nameIdx] ?? null) : null
    return
  }
  pkColumn.value = tableData.value?.columns[0] ?? null
}

// ── structure editing ──────────────────────────────────────────────────────────
interface StructEditState { field: string; type_: string; nullable: boolean; default_: string }
const structEditRow    = ref<number | null>(null)
const structEditVals   = ref<StructEditState>({ field: '', type_: '', nullable: true, default_: '' })
const structEditError  = ref('')
const addColModal      = ref(false)
const addColDef        = ref<ColDef>({ name: '', type: 'VARCHAR(255)', nullable: true, pk: false, ai: false, default_: '' })
const addColError      = ref('')
const addColLoading    = ref(false)

function getStructColIdx(colName: string): number {
  return (structData.value?.columns ?? []).map(c => c.toLowerCase()).indexOf(colName.toLowerCase())
}

function startStructEdit(ri: number) {
  if (!structData.value) return
  const row = structData.value.rows[ri]
  const fieldIdx   = getStructColIdx('field') !== -1 ? getStructColIdx('field') : getStructColIdx('name')
  const typeIdx    = getStructColIdx('type')
  const nullIdx    = getStructColIdx('null')
  const defaultIdx = getStructColIdx('default')
  structEditRow.value  = ri
  structEditError.value = ''
  structEditVals.value = {
    field:    row[fieldIdx]   ?? '',
    type_:    row[typeIdx]    ?? '',
    nullable: (row[nullIdx]   ?? 'YES') === 'YES',
    default_: row[defaultIdx] ?? '',
  }
}

function cancelStructEdit() { structEditRow.value = null; structEditError.value = '' }

async function saveStructEdit(ri: number) {
  if (!structData.value) return
  const fieldIdx = getStructColIdx('field') !== -1 ? getStructColIdx('field') : getStructColIdx('name')
  const oldName  = structData.value.rows[ri][fieldIdx] ?? ''
  const { field: newName, type_, nullable, default_ } = structEditVals.value
  if (!newName.trim()) { structEditError.value = 'El nombre no puede estar vacío'; return }

  let sql = ''
  if (props.driver === 'mysql') {
    const nullStr = nullable ? 'NULL' : 'NOT NULL'
    const defStr  = default_ ? ` DEFAULT ${escapeVal(default_)}` : ''
    sql = `ALTER TABLE ${quoteTable(props.tableName)} CHANGE COLUMN ${quoteId(oldName)} ${quoteId(newName)} ${type_} ${nullStr}${defStr}`
  } else if (props.driver === 'postgres') {
    // Rename if changed
    const cmds: string[] = []
    if (oldName !== newName)
      cmds.push(`ALTER TABLE ${quoteTable(props.tableName)} RENAME COLUMN ${quoteId(oldName)} TO ${quoteId(newName)}`)
    cmds.push(`ALTER TABLE ${quoteTable(props.tableName)} ALTER COLUMN ${quoteId(newName)} TYPE ${type_}`)
    cmds.push(`ALTER TABLE ${quoteTable(props.tableName)} ALTER COLUMN ${quoteId(newName)} ${nullable ? 'DROP NOT NULL' : 'SET NOT NULL'}`)
    sql = cmds.join('; ')
  } else {
    structEditError.value = 'SQLite no soporta modificar columnas (limitación del motor)'
    return
  }

  try {
    await invoke('db_execute', { id: props.connId, sql })
    // Update struct data locally
    if (fieldIdx !== -1) structData.value.rows[ri][fieldIdx] = newName
    const typeIdx = getStructColIdx('type')
    const nullIdx = getStructColIdx('null')
    if (typeIdx !== -1) structData.value.rows[ri][typeIdx] = type_
    if (nullIdx !== -1) structData.value.rows[ri][nullIdx] = nullable ? 'YES' : 'NO'
    cancelStructEdit()
    await loadStructure()
  } catch (e: any) { structEditError.value = String(e) }
}

async function dropColumn(ri: number) {
  if (!structData.value) return
  const fieldIdx = getStructColIdx('field') !== -1 ? getStructColIdx('field') : getStructColIdx('name')
  const colName  = structData.value.rows[ri][fieldIdx] ?? ''
  if (!confirm(`¿Eliminar columna "${colName}" de ${props.tableName}? Esta acción no se puede deshacer.`)) return
  if (props.driver === 'sqlite') { alert('SQLite no soporta DROP COLUMN en versiones antiguas'); return }
  const sql = `ALTER TABLE ${quoteTable(props.tableName)} DROP COLUMN ${quoteId(colName)}`
  try {
    await invoke('db_execute', { id: props.connId, sql })
    structData.value.rows.splice(ri, 1)
    if (structEditRow.value === ri) cancelStructEdit()
  } catch (e: any) { structEditError.value = String(e) }
}

function openAddCol() {
  addColDef.value = { name: '', type: 'VARCHAR(255)', nullable: true, pk: false, ai: false, default_: '' }
  addColError.value = ''
  addColModal.value = true
}

async function confirmAddCol() {
  const col = addColDef.value
  if (!col.name.trim()) { addColError.value = 'El nombre de columna es requerido'; return }
  addColLoading.value = true
  addColError.value   = ''
  const nullStr = col.nullable ? '' : ' NOT NULL'
  const aiStr   = col.ai ? ' AUTO_INCREMENT' : ''
  const defStr  = col.default_ ? ` DEFAULT ${escapeVal(col.default_)}` : ''
  const sql = `ALTER TABLE ${quoteTable(props.tableName)} ADD COLUMN ${quoteId(col.name)} ${col.type}${nullStr}${aiStr}${defStr}`
  try {
    await invoke('db_execute', { id: props.connId, sql })
    addColModal.value = false
    await loadStructure()
  } catch (e: any) { addColError.value = String(e) }
  finally { addColLoading.value = false }
}

// ── inline edit ────────────────────────────────────────────────────────────────
function startEdit(ri: number, row: Array<string | null>) {
  editingRow.value = ri
  editError.value  = ''
  editValues.value = {}
  tableData.value!.columns.forEach((col, ci) => {
    editValues.value[col] = row[ci] ?? ''
  })
}

function cancelEdit() { editingRow.value = null; editValues.value = {}; editError.value = '' }

async function saveEdit(ri: number) {
  if (!tableData.value) return
  const pk = pkColumn.value ?? tableData.value.columns[0]
  const pkIdx = tableData.value.columns.indexOf(pk)
  const pkVal = pkIdx !== -1 ? tableData.value.rows[ri][pkIdx] : null
  if (pkVal == null) { editError.value = 'No se encontró la clave primaria'; return }
  const sets = tableData.value.columns
    .filter(col => col !== pk)
    .map(col => `${quoteId(col)} = ${escapeVal(editValues.value[col])}`)
    .join(', ')
  if (!sets) { cancelEdit(); return }
  const sql = `UPDATE ${quoteTable(props.tableName)} SET ${sets} WHERE ${quoteId(pk)} = ${escapeVal(pkVal)}`
  try {
    await invoke('db_execute', { id: props.connId, sql })
    tableData.value.columns.forEach((col, ci) => {
      tableData.value!.rows[ri][ci] = editValues.value[col] ?? null
    })
    cancelEdit()
  } catch (e: any) { editError.value = String(e) }
}

// ── delete row ─────────────────────────────────────────────────────────────────
async function deleteRow(ri: number) {
  if (!tableData.value) return
  const pk = pkColumn.value ?? tableData.value.columns[0]
  const pkIdx = tableData.value.columns.indexOf(pk)
  if (pkIdx === -1) return
  const pkVal = tableData.value.rows[ri][pkIdx]
  if (!confirm(`¿Eliminar fila donde ${pk} = ${pkVal}?`)) return
  const sql = `DELETE FROM ${quoteTable(props.tableName)} WHERE ${quoteId(pk)} = ${escapeVal(pkVal)}`
  try {
    await invoke('db_execute', { id: props.connId, sql })
    tableData.value.rows.splice(ri, 1)
  } catch (e: any) { tableError.value = String(e) }
}

// ── insert row ─────────────────────────────────────────────────────────────────
function structColumnNames(): string[] {
  if (!structData.value) return []
  const fieldIdx = getStructColIdx('field') !== -1 ? getStructColIdx('field') : getStructColIdx('name')
  if (fieldIdx === -1) return []
  return structData.value.rows.map(r => r[fieldIdx] ?? '').filter(Boolean) as string[]
}

function colSqlType(col: string): string {
  if (!structData.value) return ''
  const fieldIdx = getStructColIdx('field') !== -1 ? getStructColIdx('field') : getStructColIdx('name')
  const typeIdx  = getStructColIdx('type')
  if (fieldIdx === -1 || typeIdx === -1) return ''
  const row = structData.value.rows.find(r => r[fieldIdx] === col)
  return (row?.[typeIdx] ?? '').toString()
}

type FieldKind = 'password' | 'date' | 'datetime' | 'time' | 'text'
function fieldKind(col: string): FieldKind {
  if (/pass|pwd|contrase/i.test(col)) return 'password'
  const ty = colSqlType(col).toLowerCase()
  if (ty.includes('datetime') || ty.includes('timestamp')) return 'datetime'
  if (ty.includes('date')) return 'date'
  if (ty.includes('time')) return 'time'
  return 'text'
}

async function openInsert() {
  insertValues.value = {}
  insertHashAlgo.value = {}
  insertError.value  = ''
  // tableData.columns comes back empty when the table has 0 rows, so prefer the
  // structure (DESCRIBE / PRAGMA table_info) which always lists every column
  if (!structData.value) await loadStructure()
  const cols = structColumnNames()
  const finalCols = cols.length ? cols : (tableData.value?.columns ?? [])
  finalCols.forEach(col => {
    insertValues.value[col] = ''
    if (fieldKind(col) === 'password') insertHashAlgo.value[col] = 'none'
  })
  insertModal.value = true
}

async function confirmInsert() {
  insertLoading.value = true
  insertError.value   = ''
  const finalValues: Record<string, string> = { ...insertValues.value }
  try {
    for (const col of Object.keys(finalValues)) {
      if (finalValues[col] === '') continue
      const kind = fieldKind(col)
      if (kind === 'password') {
        const algo = insertHashAlgo.value[col] ?? 'none'
        if (algo !== 'none') {
          finalValues[col] = await invoke<string>('hash_value', { algo, value: finalValues[col] })
        }
      } else if (kind === 'datetime') {
        finalValues[col] = finalValues[col].replace('T', ' ')
      }
    }
  } catch (e: any) { insertError.value = String(e); insertLoading.value = false; return }
  const cols = Object.keys(finalValues).filter(col => finalValues[col] !== '')
  if (cols.length === 0) { insertModal.value = false; insertLoading.value = false; return }
  const colList = cols.map(quoteId).join(', ')
  const valList = cols.map(c => escapeVal(finalValues[c])).join(', ')
  const sql = `INSERT INTO ${quoteTable(props.tableName)} (${colList}) VALUES (${valList})`
  try {
    await invoke('db_execute', { id: props.connId, sql })
    insertModal.value = false
    await loadTable()
  } catch (e: any) { insertError.value = String(e) }
  finally { insertLoading.value = false }
}

// ── create table ───────────────────────────────────────────────────────────────
function openCreateTable() {
  newTableName.value = ''
  createError.value  = ''
  newTableCols.value = [{ name: 'id', type: 'INT', nullable: false, pk: true, ai: true, default_: '' }]
  newTableFks.value  = []
  createModal.value = true
  loadAllTables()
}

function addNewTableCol() {
  newTableCols.value.push({ name: '', type: 'VARCHAR(255)', nullable: true, pk: false, ai: false, default_: '' })
}

function removeColDef(i: number) { newTableCols.value.splice(i, 1) }

function addNewTableFk() {
  newTableFks.value.push({ column: newTableCols.value[0]?.name ?? '', refTable: '', refColumn: '', constraintName: '' })
}

function removeNewTableFk(i: number) { newTableFks.value.splice(i, 1) }

async function loadFkRefTableCols(refTable: string) {
  if (!refTable || fkRefTableCols.value[refTable]) return
  try {
    const struct = await invoke<QueryResult>('db_describe_table', { id: props.connId, table: refTable, driver: props.driver })
    const cols = struct.columns.map(c => c.toLowerCase())
    const fieldIdx = cols.indexOf('field') !== -1 ? cols.indexOf('field') : cols.indexOf('name')
    fkRefTableCols.value[refTable] = fieldIdx !== -1
      ? (struct.rows.map(r => r[fieldIdx] ?? '').filter(Boolean) as string[])
      : []
  } catch { fkRefTableCols.value[refTable] = [] }
}

function buildCreateSql(): string {
  const pkCols = newTableCols.value.filter(c => c.pk)
  // SQLite has no AUTO_INCREMENT keyword: the only way to get an autoincrementing
  // key is a lone column declared inline as "INTEGER PRIMARY KEY AUTOINCREMENT"
  const sqliteInlinePk = props.driver === 'sqlite' && pkCols.length === 1 && pkCols[0].ai ? pkCols[0].name : null

  const defs = newTableCols.value.map(c => {
    if (sqliteInlinePk === c.name) {
      return `${quoteId(c.name)} INTEGER PRIMARY KEY AUTOINCREMENT`
    }
    let type = c.type
    let d: string
    if (c.ai && props.driver === 'postgres') {
      if (/^int(eger)?(\(|$)/i.test(type)) type = 'SERIAL'
      else if (/^bigint(\(|$)/i.test(type)) type = 'BIGSERIAL'
      d = `${quoteId(c.name)} ${type}`
    } else {
      d = `${quoteId(c.name)} ${type}`
      if (c.ai && props.driver === 'mysql') d += ' AUTO_INCREMENT'
    }
    if (!c.nullable) d += ' NOT NULL'
    if (c.default_)  d += ` DEFAULT ${escapeVal(c.default_)}`
    return d
  })
  const pks = sqliteInlinePk ? [] : pkCols.map(c => quoteId(c.name))
  if (pks.length) defs.push(`PRIMARY KEY (${pks.join(', ')})`)

  for (const fk of newTableFks.value) {
    if (!fk.column || !fk.refTable || !fk.refColumn) continue
    const prefix = fk.constraintName.trim() ? `CONSTRAINT ${quoteId(fk.constraintName.trim())} ` : ''
    defs.push(`${prefix}FOREIGN KEY (${quoteId(fk.column)}) REFERENCES ${quoteTable(fk.refTable)} (${quoteId(fk.refColumn)})`)
  }

  return `CREATE TABLE ${quoteTable(newTableName.value)} (\n  ${defs.join(',\n  ')}\n)`
}

async function confirmCreateTable() {
  if (!newTableName.value.trim() || newTableCols.value.length === 0) return
  createLoading.value = true
  createError.value   = ''
  const sql = buildCreateSql()
  try {
    await invoke('db_execute', { id: props.connId, sql })
    createModal.value = false
    const createdName = newTableName.value.trim()
    delete _tablesCache[props.connId]
    if (sidebarActiveConnId.value === props.connId && !sidebarTables.value.includes(createdName)) {
      sidebarTables.value = [...sidebarTables.value, createdName].sort((a, b) => a.localeCompare(b))
    }
    window.dispatchEvent(new CustomEvent('db-schema-changed', { detail: { connId: props.connId } }))
    if (!props.tableName) {
      // this tab was opened as a blank "new table" placeholder — promote it into a real table tab;
      // the props.tableName watcher below picks up the change and loads the new table's data
      editorStore.tabTableCreated(`db://${props.connId}/__new__`, createdName)
    }
  } catch (e: any) { createError.value = String(e) }
  finally { createLoading.value = false }
}

// ── query view ─────────────────────────────────────────────────────────────────
function loadSavedQueries() {
  try { savedQueries.value = JSON.parse(localStorage.getItem(storageKey.value) ?? '[]') }
  catch { savedQueries.value = [] }
}

function persistQueries() { localStorage.setItem(storageKey.value, JSON.stringify(savedQueries.value)) }

function openSaveModal() {
  if (!queryView) return
  saveName.value = queryView.state.doc.toString().trim().slice(0, 60).replace(/\s+/g, ' ')
  saveModal.value = true
  nextTick(() => (document.querySelector('.save-name-input') as HTMLInputElement)?.select())
}

function confirmSave() {
  if (!queryView) return
  const sqlText = queryView.state.doc.toString().trim()
  if (!sqlText || !saveName.value.trim()) { saveModal.value = false; return }
  savedQueries.value.unshift({ id: crypto.randomUUID(), name: saveName.value.trim(), sql: sqlText, ts: Date.now(), pinned: true })
  persistQueries(); saveModal.value = false
}

function addToHistory(sqlText: string) {
  const existing = savedQueries.value.findIndex(q => !q.pinned && q.sql === sqlText)
  if (existing !== -1) { savedQueries.value[existing].ts = Date.now() }
  else {
    savedQueries.value.push({ id: crypto.randomUUID(), name: '', sql: sqlText, ts: Date.now(), pinned: false })
    const unpinned = savedQueries.value.filter(q => !q.pinned)
    if (unpinned.length > 30) {
      const oldest = unpinned.sort((a, b) => a.ts - b.ts)[0]
      savedQueries.value = savedQueries.value.filter(q => q.id !== oldest.id)
    }
  }
  persistQueries()
}

function loadQuery(q: SavedQuery) {
  if (!queryView) return
  queryView.dispatch({ changes: { from: 0, to: queryView.state.doc.length, insert: q.sql } })
  queryView.focus()
}

function deleteSavedQuery(id: string) {
  savedQueries.value = savedQueries.value.filter(q => q.id !== id)
  persistQueries()
}

function onResizeMouseDown(e: MouseEvent) {
  isResizing.value = true; resizeStartY = e.clientY; resizeStartH = editorHeight.value
  window.addEventListener('mousemove', onResizeMouseMove)
  window.addEventListener('mouseup', onResizeMouseUp)
}

function onResizeMouseMove(e: MouseEvent) {
  if (!isResizing.value) return
  editorHeight.value = Math.max(60, Math.min(500, resizeStartH + e.clientY - resizeStartY))
  queryView?.requestMeasure()
}

function onResizeMouseUp() {
  isResizing.value = false
  window.removeEventListener('mousemove', onResizeMouseMove)
  window.removeEventListener('mouseup', onResizeMouseUp)
  queryView?.requestMeasure()
}

function getSqlDialect() {
  if (props.driver === 'postgres') return PostgreSQL
  if (props.driver === 'sqlite')   return SQLite
  return MySQL
}

function buildSqlSchema(): Record<string, string[]> {
  const schema: Record<string, string[]> = {}
  for (const t of allTables.value) schema[t] = []
  if (tableData.value && props.tableName) schema[props.tableName] = tableData.value.columns
  return schema
}

function initQueryEditor() {
  if (!queryEditorEl.value) return
  const initSql = `SELECT * FROM ${quoteTable(props.tableName)}`
  if (queryView) { queryView.destroy(); queryView = null }
  queryView = new EditorView({
    state: CMState.create({
      doc: initSql,
      extensions: [
        history(),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        autocompletion(),
        dbThemeCompartment.of(cmThemeFor(appTheme.value)),
        EditorView.lineWrapping,
        sql({ dialect: getSqlDialect(), schema: buildSqlSchema(), upperCaseKeywords: false }),
        keymap.of([
          ...defaultKeymap, ...historyKeymap, ...completionKeymap,
          { key: 'Ctrl-Enter', run: () => { runQuery(); return true } },
          { key: 'Mod-Enter',  run: () => { runQuery(); return true } },
          { key: 'Ctrl-s',     run: () => { openSaveModal(); return true } },
        ]),
        EditorView.theme({
          '&': { height: '100%', fontSize: '13px', borderRadius: '6px' },
          '.cm-scroller': { fontFamily: 'var(--font-mono)', overflow: 'auto' },
          '.cm-content': { padding: '8px 10px' },
          '.cm-lineWrapping': { wordBreak: 'break-all' },
        }),
      ],
    }),
    parent: queryEditorEl.value,
  })
}

async function loadAllTables() {
  if (_tablesCache[props.connId]) { allTables.value = _tablesCache[props.connId]; return }
  try {
    const list = await invoke<string[]>('db_list_tables', { id: props.connId })
    _tablesCache[props.connId] = list; allTables.value = list
  } catch { /**/ }
}

async function runQuery() {
  if (!queryView) return
  const sqlText = queryView.state.doc.toString().trim()
  if (!sqlText) return
  loadingQuery.value = true; queryError.value = ''; queryResult.value = null
  try {
    queryResult.value = await invoke<QueryResult>('db_query', { id: props.connId, sql: sqlText })
    addToHistory(sqlText)
  } catch (e: any) { queryError.value = String(e) }
  finally { loadingQuery.value = false }
}

// ── computed ───────────────────────────────────────────────────────────────────
const pinnedQueries  = computed(() => savedQueries.value.filter(q => q.pinned).sort((a,b) => b.ts-a.ts))
const historyQueries = computed(() => savedQueries.value.filter(q => !q.pinned).sort((a,b) => b.ts-a.ts))

// ── watchers ───────────────────────────────────────────────────────────────────
watch(() => props.tableName, (name) => {
  if (!name) { activeView.value = 'estructura'; openCreateTable(); return }
  activeView.value = 'data'
  loadTable()
})
watch(() => props.connId, () => { delete _tablesCache[props.connId]; activeView.value = 'data'; loadTable() })
watch(appTheme, (t) => {
  queryView?.dispatch({ effects: dbThemeCompartment.reconfigure(cmThemeFor(t)) })
})

watch(activeView, async (v) => {
  if (v === 'estructura') {
    await loadStructure()
  } else if (v === 'query') {
    await nextTick()
    loadAllTables().then(() => {
      if (!queryView) initQueryEditor()
      else { queryView.destroy(); queryView = null; nextTick(initQueryEditor) }
    })
    if (!queryView) initQueryEditor()
  }
})

onMounted(async () => {
  loadSavedQueries()
  if (!props.tableName) {
    activeView.value = 'estructura'
    openCreateTable()
    return
  }
  await loadTable()
  // Also load structure in background to get PK info
  loadStructure()
})

onBeforeUnmount(() => {
  queryView?.destroy()
  window.removeEventListener('mousemove', onResizeMouseMove)
  window.removeEventListener('mouseup', onResizeMouseUp)
})
</script>

<template>
  <div class="db-view">
    <!-- Top bar -->
    <div class="db-view-bar">
      <div class="dv-title">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="color:var(--accent)">
          <rect x="3" y="3" width="18" height="18" rx="1"/><path d="M3 9h18M3 15h18M9 3v18"/>
        </svg>
        <input
          v-if="renamingCurrentTable"
          v-model="renameCurrentValue"
          class="dv-rename-input"
          autofocus
          @keydown.enter="confirmRenameCurrentTable"
          @keydown.esc="cancelRenameCurrentTable"
          @blur="confirmRenameCurrentTable"
        />
        <span v-else>{{ tableName || t('newTableTab') }}</span>
        <span class="dv-driver">{{ driver }}</span>
        <template v-if="tableName && !renamingCurrentTable">
          <button class="dv-title-btn" @click="startRenameCurrentTable" :title="t('rename')">
            <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
              <path d="M12.854.146a.5.5 0 00-.707 0L10.5 1.793 14.207 5.5l1.647-1.646a.5.5 0 000-.708l-3-3zm.646 6.061L9.793 2.5 3.293 9H3.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.207l6.5-6.5zm-7.468 7.468A.5.5 0 015.5 14H5v-.5a.5.5 0 01-.5-.5H4v-.5a.5.5 0 01-.5-.5H3v-.5a.5.5 0 01-.5-.5H2.5v-.5a.5.5 0 01.646-.473l.853.214 1.832-1.832 3.709 3.709-1.832 1.832.214.853z"/>
            </svg>
          </button>
          <button class="dv-title-btn dv-title-del" @click="deleteCurrentTable" :title="t('confirmDeleteTable')">
            <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
              <path d="M5.5 5.5A.5.5 0 016 6v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm2.5 0a.5.5 0 01.5.5v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm3 .5a.5.5 0 00-1 0v6a.5.5 0 001 0V6z"/>
              <path fill-rule="evenodd" d="M14.5 3a1 1 0 01-1 1H13v9a2 2 0 01-2 2H5a2 2 0 01-2-2V4h-.5a1 1 0 010-2h4a1 1 0 011-1h2a1 1 0 011 1h4a1 1 0 011 1zM4.118 4L4 4.059V13a1 1 0 001 1h6a1 1 0 001-1V4.059L11.882 4H4.118zM2.5 3a.5.5 0 000 1h11a.5.5 0 000-1h-11z"/>
            </svg>
          </button>
        </template>
      </div>

      <div class="dv-tabs">
        <button class="dv-tab" :class="{ active: activeView === 'data' }" @click="activeView = 'data'">{{ t('dataTab') }}</button>
        <button class="dv-tab" :class="{ active: activeView === 'estructura' }" @click="activeView = 'estructura'">{{ t('structureTab') }}</button>
        <button class="dv-tab" :class="{ active: activeView === 'query' }" @click="activeView = 'query'">Query SQL</button>
      </div>

      <div class="dv-actions">
        <button v-if="activeView === 'data'" class="dv-action-btn dv-insert-btn" @click="openInsert" :title="t('newRowTitle')">
          <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M8 2a.5.5 0 01.5.5v5h5a.5.5 0 010 1h-5v5a.5.5 0 01-1 0v-5h-5a.5.5 0 010-1h5v-5A.5.5 0 018 2z"/></svg>
          Nueva fila
        </button>
        <button v-if="activeView === 'estructura'" class="dv-action-btn" @click="openCreateTable" :title="t('createTableTitle')">
          <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M8 2a.5.5 0 01.5.5v5h5a.5.5 0 010 1h-5v5a.5.5 0 01-1 0v-5h-5a.5.5 0 010-1h5v-5A.5.5 0 018 2z"/></svg>
          Crear tabla
        </button>
        <button class="dv-refresh" @click="activeView === 'estructura' ? loadStructure() : loadTable()" :title="t('reload')" :disabled="loadingTable || structLoading">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2v1z"/>
            <path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466z"/>
          </svg>
        </button>
        <select v-if="activeView === 'data'" v-model="rowLimit" class="dv-limit" @change="loadTable">
          <option :value="100">100 filas</option>
          <option :value="200">200 filas</option>
          <option :value="500">500 filas</option>
          <option :value="1000">1000 filas</option>
        </select>
      </div>
    </div>

    <div v-if="tableActionError" class="dv-table-action-error">
      {{ tableActionError }}
      <button @click="tableActionError = ''">×</button>
    </div>

    <!-- ── DATA VIEW ── -->
    <template v-if="activeView === 'data'">
      <div v-if="loadingTable" class="dv-loading"><span class="dv-spinner" /> Cargando…</div>
      <div v-else-if="tableError" class="dv-error">
        <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path d="M8 15A7 7 0 1 1 8 1a7 7 0 0 1 0 14zm0 1A8 8 0 1 0 8 0a8 8 0 0 0 0 16z"/><path d="M7.002 11a1 1 0 1 1 2 0 1 1 0 0 1-2 0zM7.1 4.995a.905.905 0 1 1 1.8 0l-.35 3.507a.552.552 0 0 1-1.1 0L7.1 4.995z"/></svg>
        <pre>{{ tableError }}</pre>
      </div>
      <template v-else-if="tableData">
        <div v-if="editError" class="dv-inline-error">{{ editError }}</div>
        <div v-if="tableData.rows.length === 0" class="dv-empty"><span>{{ t('emptyTableMsg') }}</span></div>
        <div v-else class="dv-grid-wrap">
          <table class="dv-grid">
            <thead><tr>
              <th class="dv-row-num">#</th>
              <th v-for="col in tableData.columns" :key="col">{{ col }}</th>
              <th class="dv-actions-th"></th>
            </tr></thead>
            <tbody>
              <tr v-for="(row, ri) in tableData.rows" :key="ri" :class="{ 'row-editing': editingRow === ri }">
                <td class="dv-row-num">{{ ri + 1 }}</td>
                <!-- Edit mode: inputs -->
                <template v-if="editingRow === ri">
                  <td v-for="col in tableData.columns" :key="col" class="dv-edit-cell">
                    <input class="cell-input" v-model="editValues[col]" :placeholder="col" />
                  </td>
                  <td class="dv-actions-td">
                    <button class="row-btn row-save" @click="saveEdit(ri)" :title="t('save')">✓</button>
                    <button class="row-btn row-cancel" @click="cancelEdit" :title="t('cancel')">✕</button>
                  </td>
                </template>
                <!-- Normal mode -->
                <template v-else>
                  <td v-for="(cell, ci) in row" :key="ci" :class="{ 'cell-null': cell === null }" :title="cell ?? 'NULL'">
                    {{ cell ?? 'NULL' }}
                  </td>
                  <td class="dv-actions-td">
                    <button class="row-btn row-edit" @click="startEdit(ri, row)" :title="t('edit')">
                      <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M12.854.146a.5.5 0 00-.707 0L10.5 1.793 14.207 5.5l1.647-1.646a.5.5 0 000-.708l-3-3zm.646 6.061L9.793 2.5 3.293 9H3.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.207l6.5-6.5zm-7.468 7.468A.5.5 0 015.5 14H5v-.5a.5.5 0 01-.5-.5H4v-.5a.5.5 0 01-.5-.5H3v-.5a.5.5 0 01-.5-.5H2v-.5a.5.5 0 01-.402-.492L1.01 10H1v.5a.5.5 0 00.134.336l.103.103.818-.818-.35.35a.5.5 0 000 .708l.5.5a.5.5 0 00.707 0l.35-.35-.818.818.103.103A.5.5 0 003.5 12H4v.5a.5.5 0 00.5.5H5v.5a.5.5 0 00.5.5H6v.5a.5.5 0 00.5.5H7v.5a.5.5 0 00.5.5h.207l-.325-.325z"/></svg>
                    </button>
                    <button class="row-btn row-del" @click="deleteRow(ri)" :title="t('delete')">
                      <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M5.5 5.5A.5.5 0 016 6v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm2.5 0a.5.5 0 01.5.5v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm3 .5a.5.5 0 00-1 0v6a.5.5 0 001 0V6z"/><path fill-rule="evenodd" d="M14.5 3a1 1 0 01-1 1H13v9a2 2 0 01-2 2H5a2 2 0 01-2-2V4h-.5a1 1 0 010-2h3a1 1 0 011-1h2a1 1 0 011 1h3a1 1 0 011 1zM4.118 4L4 4.059V13a1 1 0 001 1h6a1 1 0 001-1V4.059L11.882 4H4.118zM2.5 3h11V2h-11v1z"/></svg>
                    </button>
                  </td>
                </template>
              </tr>
            </tbody>
          </table>
        </div>
        <div class="dv-footer">{{ tableData.rows.length }} {{ t('rowsWord') }} · LIMIT {{ rowLimit }} <span v-if="pkColumn" class="dv-pk-hint">· PK: {{ pkColumn }}</span></div>
      </template>
    </template>

    <!-- ── ESTRUCTURA VIEW ── -->
    <template v-else-if="activeView === 'estructura'">
      <div v-if="structLoading" class="dv-loading"><span class="dv-spinner" /> Cargando estructura…</div>
      <div v-else-if="structError && !structData" class="dv-error">
        <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path d="M8 15A7 7 0 1 1 8 1a7 7 0 0 1 0 14zm0 1A8 8 0 1 0 8 0a8 8 0 0 0 0 16z"/><path d="M7.002 11a1 1 0 1 1 2 0 1 1 0 0 1-2 0zM7.1 4.995a.905.905 0 1 1 1.8 0l-.35 3.507a.552.552 0 0 1-1.1 0L7.1 4.995z"/></svg>
        <pre>{{ structError }}</pre>
      </div>
      <template v-else-if="structData">
        <div v-if="structEditError" class="dv-inline-error">{{ structEditError }}</div>
        <div class="dv-grid-wrap">
          <table class="dv-grid struct-grid">
            <thead><tr>
              <th v-for="col in structData.columns" :key="col">{{ col }}</th>
              <th class="dv-actions-th">{{ t('actions') }}</th>
            </tr></thead>
            <tbody>
              <tr v-for="(row, ri) in structData.rows" :key="ri" :class="{ 'row-editing': structEditRow === ri }">
                <!-- Edit mode -->
                <template v-if="structEditRow === ri">
                  <td class="dv-edit-cell">
                    <input class="cell-input" v-model="structEditVals.field" :placeholder="t('name')" />
                  </td>
                  <td class="dv-edit-cell">
                    <select class="cell-input" style="min-width:130px" v-model="structEditVals.type_">
                      <option v-for="t in COLUMN_TYPES" :key="t" :value="t">{{ t }}</option>
                      <option :value="structEditVals.type_">{{ structEditVals.type_ }} (actual)</option>
                    </select>
                  </td>
                  <td class="dv-edit-cell" style="text-align:center">
                    <input type="checkbox" v-model="structEditVals.nullable" style="accent-color:var(--accent)" />
                    <span style="font-size:10px;margin-left:3px;color:var(--fg-muted)">NULL</span>
                  </td>
                  <!-- Key and Extra: read-only in edit mode -->
                  <td v-for="ci in structData.columns.slice(3)" :key="ci" :class="{ 'cell-null': !row[structData.columns.indexOf(ci)] }">
                    {{ row[structData.columns.indexOf(ci)] ?? '' }}
                  </td>
                  <td class="dv-edit-cell">
                    <input class="cell-input" v-model="structEditVals.default_" placeholder="default" />
                  </td>
                  <td class="dv-actions-td">
                    <button class="row-btn row-save" @click="saveStructEdit(ri)" :title="t('save')">✓</button>
                    <button class="row-btn row-cancel" @click="cancelStructEdit" :title="t('cancel')">✕</button>
                  </td>
                </template>
                <!-- Normal mode -->
                <template v-else>
                  <td v-for="(cell, ci) in row" :key="ci"
                    :class="{
                      'cell-null':  cell === null || cell === '',
                      'cell-pk':    structData.columns[ci]?.toLowerCase() === 'key' && cell === 'PRI',
                      'cell-type':  structData.columns[ci]?.toLowerCase() === 'type',
                      'cell-extra': structData.columns[ci]?.toLowerCase() === 'extra' && !!cell,
                    }"
                  >{{ cell ?? '' }}</td>
                  <td class="dv-actions-td">
                    <button class="row-btn row-edit" @click="startStructEdit(ri)" :title="t('editColumnTitle')">
                      <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M12.854.146a.5.5 0 00-.707 0L10.5 1.793 14.207 5.5l1.647-1.646a.5.5 0 000-.708l-3-3zm.646 6.061L9.793 2.5 3.293 9H3.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.207l6.5-6.5zm-7.468 7.468A.5.5 0 015.5 14H5v-.5a.5.5 0 01-.5-.5H4v-.5a.5.5 0 01-.5-.5H3v-.5a.5.5 0 01-.5-.5H2v-.5a.5.5 0 01-.402-.492L1.01 10H1v.5a.5.5 0 00.134.336l.103.103.818-.818-.35.35a.5.5 0 000 .708l.5.5a.5.5 0 00.707 0l.35-.35-.818.818.103.103A.5.5 0 003.5 12H4v.5a.5.5 0 00.5.5H5v.5a.5.5 0 00.5.5H6v.5a.5.5 0 00.5.5H7v.5a.5.5 0 00.5.5h.207l-.325-.325z"/></svg>
                    </button>
                    <button class="row-btn row-del" @click="dropColumn(ri)" :title="t('deleteColumnTitle')">
                      <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M5.5 5.5A.5.5 0 016 6v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm2.5 0a.5.5 0 01.5.5v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm3 .5a.5.5 0 00-1 0v6a.5.5 0 001 0V6z"/><path fill-rule="evenodd" d="M14.5 3a1 1 0 01-1 1H13v9a2 2 0 01-2 2H5a2 2 0 01-2-2V4h-.5a1 1 0 010-2h3a1 1 0 011-1h2a1 1 0 011 1h3a1 1 0 011 1zM4.118 4L4 4.059V13a1 1 0 001 1h6a1 1 0 001-1V4.059L11.882 4H4.118zM2.5 3h11V2h-11v1z"/></svg>
                    </button>
                  </td>
                </template>
              </tr>
            </tbody>
          </table>
        </div>
        <div class="dv-footer struct-footer">
          {{ structData.rows.length }} columnas
          <span v-if="pkColumn" class="dv-pk-hint"> · PK: {{ pkColumn }}</span>
          <button class="struct-add-col-btn" @click="openAddCol">
            <svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor"><path d="M8 2a.5.5 0 01.5.5v5h5a.5.5 0 010 1h-5v5a.5.5 0 01-1 0v-5h-5a.5.5 0 010-1h5v-5A.5.5 0 018 2z"/></svg>
            Agregar columna
          </button>
        </div>
      </template>
    </template>

    <!-- ── QUERY VIEW ── -->
    <template v-else-if="activeView === 'query'">
      <div class="dv-query-toolbar">
        <span class="dv-query-hint"><kbd>Ctrl+Enter</kbd> {{ t('queryHint') }} · <kbd>Ctrl+S</kbd> {{ t('queryHintSave') }} · {{ t('queryHintAny') }}</span>
        <div class="dv-query-actions">
          <button class="dv-q-btn" @click="openSaveModal">
            <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M2 1a1 1 0 00-1 1v12a1 1 0 001 1h12a1 1 0 001-1V4.5a.5.5 0 00-.146-.354l-3-3A.5.5 0 0011.5 1H2zm0 1h9.293L14 4.707V14H2V2zm3 7a2 2 0 114 0 2 2 0 01-4 0z"/></svg>
            {{ t('save') }}
          </button>
          <button class="dv-q-btn" :class="{ active: showHistory }" @click="showHistory = !showHistory">
            {{ t('historyLabel') }} <span v-if="savedQueries.length" class="hist-count">{{ savedQueries.length }}</span>
          </button>
          <button class="dv-run-btn" @click="runQuery" :disabled="loadingQuery">
            <svg v-if="!loadingQuery" width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M11.596 8.697l-6.363 3.692c-.54.313-1.233-.066-1.233-.697V4.308c0-.63.692-1.01 1.233-.696l6.363 3.692a.802.802 0 010 1.393z"/></svg>
            <span v-else class="spin-sm" />
            {{ loadingQuery ? '' : t('run') }}
          </button>
        </div>
      </div>

      <div class="dv-editor-wrap" :style="{ height: editorHeight + 'px' }">
        <div ref="queryEditorEl" class="dv-cm-editor" />
      </div>
      <div class="dv-resize-handle" @mousedown.prevent="onResizeMouseDown" :class="{ dragging: isResizing }">
        <span class="dv-resize-dots" />
      </div>

      <div class="dv-query-body">
        <div v-if="showHistory" class="dv-history-panel">
          <div class="hist-section-title">{{ t('savedHeader') }} ({{ pinnedQueries.length }})</div>
          <div v-if="pinnedQueries.length === 0" class="hist-empty">{{ t('noSavedQueries') }}</div>
          <div v-for="q in pinnedQueries" :key="q.id" class="hist-item hist-item--pinned" @click="loadQuery(q)">
            <div class="hist-item-name">{{ q.name }}</div>
            <div class="hist-item-sql">{{ q.sql.slice(0, 80) }}</div>
            <div class="hist-item-foot">
              <span class="hist-ts">{{ formatTs(q.ts) }}</span>
              <button class="hist-del" @click.stop="deleteSavedQuery(q.id)">×</button>
            </div>
          </div>
          <div class="hist-section-title" style="margin-top:10px">{{ t('historyLabel') }} ({{ historyQueries.length }})</div>
          <div v-if="historyQueries.length === 0" class="hist-empty">{{ t('noHistoryYetQuery') }}</div>
          <div v-for="q in historyQueries" :key="q.id" class="hist-item" @click="loadQuery(q)">
            <div class="hist-item-sql hist-item-sql--mono">{{ q.sql.slice(0, 100) }}</div>
            <div class="hist-item-foot">
              <span class="hist-ts">{{ formatTs(q.ts) }}</span>
              <button class="hist-del" @click.stop="deleteSavedQuery(q.id)">×</button>
            </div>
          </div>
        </div>
        <div class="dv-results-area">
          <div v-if="loadingQuery" class="dv-loading"><span class="dv-spinner" /> {{ t('executingLabel') }}</div>
          <div v-else-if="queryError" class="dv-error">
            <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor"><path d="M8 15A7 7 0 1 1 8 1a7 7 0 0 1 0 14zm0 1A8 8 0 1 0 8 0a8 8 0 0 0 0 16z"/><path d="M7.002 11a1 1 0 1 1 2 0 1 1 0 0 1-2 0zM7.1 4.995a.905.905 0 1 1 1.8 0l-.35 3.507a.552.552 0 0 1-1.1 0L7.1 4.995z"/></svg>
            <pre>{{ queryError }}</pre>
          </div>
          <template v-else-if="queryResult">
            <div v-if="queryResult.rows.length === 0" class="dv-empty"><span>{{ t('noResults') }}</span></div>
            <div v-else class="dv-grid-wrap">
              <table class="dv-grid">
                <thead><tr>
                  <th class="dv-row-num">#</th>
                  <th v-for="col in queryResult.columns" :key="col">{{ col }}</th>
                </tr></thead>
                <tbody>
                  <tr v-for="(row, ri) in queryResult.rows" :key="ri">
                    <td class="dv-row-num">{{ ri + 1 }}</td>
                    <td v-for="(cell, ci) in row" :key="ci" :class="{ 'cell-null': cell === null }">{{ cell ?? 'NULL' }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
            <div class="dv-footer">{{ queryResult.rows.length }} filas</div>
          </template>
          <div v-else class="dv-empty">
            <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" opacity=".2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>
            <span>Ctrl+Enter para ejecutar</span>
          </div>
        </div>
      </div>
    </template>

    <!-- ── INSERT ROW MODAL ── -->
    <Teleport to="body">
      <div v-if="insertModal" class="modal-overlay" @click.self="insertModal = false">
        <div class="modal-box modal-wide">
          <div class="modal-head">
            <span>Nueva fila — {{ tableName }}</span>
            <button class="modal-close" @click="insertModal = false">×</button>
          </div>
          <div class="modal-body insert-grid">
            <template v-for="col in Object.keys(insertValues)" :key="col">
              <label class="insert-label">{{ col }}</label>
              <div v-if="fieldKind(col) === 'password'" class="insert-password-row">
                <input class="insert-input" v-model="insertValues[col]" :placeholder="`${col} ${t('emptyNullHint')}`" />
                <select class="insert-input insert-hash-select" v-model="insertHashAlgo[col]" :title="t('hashAlgoTitle')">
                  <option v-for="a in HASH_ALGOS" :key="a" :value="a">{{ a === 'none' ? t('plainText') : HASH_ALGO_LABELS[a] }}</option>
                </select>
              </div>
              <input v-else-if="fieldKind(col) === 'datetime'" type="datetime-local" step="1" class="insert-input" v-model="insertValues[col]" />
              <input v-else-if="fieldKind(col) === 'date'" type="date" class="insert-input" v-model="insertValues[col]" />
              <input v-else-if="fieldKind(col) === 'time'" type="time" step="1" class="insert-input" v-model="insertValues[col]" />
              <input v-else class="insert-input" v-model="insertValues[col]" :placeholder="`${col} ${t('emptyNullHint')}`" />
            </template>
          </div>
          <div v-if="insertError" class="modal-error">{{ insertError }}</div>
          <div class="modal-foot">
            <button class="btn-secondary" @click="insertModal = false">{{ t('cancel') }}</button>
            <button class="btn-primary" @click="confirmInsert" :disabled="insertLoading">
              <span v-if="insertLoading" class="spin-sm" /> {{ insertLoading ? '' : 'Insertar' }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- ── CREATE TABLE MODAL ── -->
    <Teleport to="body">
      <div v-if="createModal" class="modal-overlay" @click.self="createModal = false">
        <div class="modal-box modal-xl">
          <div class="modal-head">
            <span>{{ t('createNewTableTitle') }}</span>
            <button class="modal-close" @click="createModal = false">×</button>
          </div>
          <div class="modal-body">
            <div class="ct-row">
              <label class="ct-label">{{ t('tableNameLabel') }}</label>
              <input class="ct-name-input" v-model="newTableName" placeholder="table_name" />
            </div>
            <div class="ct-cols-header">
              <span class="ct-col-h" style="flex:2">{{ t('columnNameHeader') }}</span>
              <span class="ct-col-h" style="flex:2">{{ t('type') }}</span>
              <span class="ct-col-h ct-center">NOT NULL</span>
              <span class="ct-col-h ct-center">PK</span>
              <span class="ct-col-h ct-center">AUTO_INC</span>
              <span class="ct-col-h" style="flex:1">{{ t('defaultHeader') }}</span>
              <span style="width:24px"/>
            </div>
            <div v-for="(col, i) in newTableCols" :key="i" class="ct-col-row">
              <input class="ct-input" style="flex:2" v-model="col.name" :placeholder="t('columnNameHeader')" />
              <select class="ct-select" style="flex:2" v-model="col.type">
                <option v-for="t in COLUMN_TYPES" :key="t" :value="t">{{ t }}</option>
              </select>
              <input type="checkbox" v-model="col.nullable" class="ct-check" />
              <input type="checkbox" v-model="col.pk"       class="ct-check" />
              <input type="checkbox" v-model="col.ai"       class="ct-check" :disabled="!col.type.startsWith('INT') && !col.type.startsWith('BIG')" />
              <input class="ct-input" style="flex:1" v-model="col.default_" placeholder="default" />
              <button class="ct-del-col" @click="removeColDef(i)" :disabled="newTableCols.length <= 1">×</button>
            </div>
            <button class="ct-add-col" @click="addNewTableCol">+ Agregar columna</button>

            <div class="ct-fk-section">
              <div class="ct-preview-label" style="margin-top:10px">{{ t('foreignKeysLabel') }}</div>
              <div v-if="newTableFks.length" class="ct-cols-header">
                <span class="ct-col-h" style="flex:1.2">{{ t('columnNameHeader') }}</span>
                <span class="ct-col-h" style="flex:1.5">{{ t('erToTable') }}</span>
                <span class="ct-col-h" style="flex:1.2">{{ t('column') }}</span>
                <span class="ct-col-h" style="flex:1.2">{{ t('erConstraintName') }}</span>
                <span style="width:24px"/>
              </div>
              <div v-for="(fk, i) in newTableFks" :key="i" class="ct-col-row">
                <select class="ct-select" style="flex:1.2" v-model="fk.column">
                  <option v-for="c in newTableCols" :key="c.name" :value="c.name">{{ c.name }}</option>
                </select>
                <select class="ct-select" style="flex:1.5" v-model="fk.refTable" @change="loadFkRefTableCols(fk.refTable)">
                  <option value="" disabled>{{ t('erToTable') }}</option>
                  <option v-for="tb in allTables" :key="tb" :value="tb">{{ tb }}</option>
                </select>
                <select v-if="fkRefTableCols[fk.refTable]?.length" class="ct-select" style="flex:1.2" v-model="fk.refColumn">
                  <option v-for="c in fkRefTableCols[fk.refTable]" :key="c" :value="c">{{ c }}</option>
                </select>
                <input v-else class="ct-input" style="flex:1.2" v-model="fk.refColumn" placeholder="id" />
                <input class="ct-input" style="flex:1.2" v-model="fk.constraintName" placeholder="fk_..." />
                <button class="ct-del-col" @click="removeNewTableFk(i)">×</button>
              </div>
              <button class="ct-add-col" @click="addNewTableFk" :disabled="allTables.length === 0">+ {{ t('erNewRelationship') }}</button>
              <div v-if="allTables.length === 0" class="ct-fk-hint">{{ t('noOtherTablesHint') }}</div>
            </div>

            <div class="ct-preview">
              <div class="ct-preview-label">SQL generado:</div>
              <pre class="ct-preview-sql">{{ newTableName ? buildCreateSql() : '— ingresa el nombre de la tabla —' }}</pre>
            </div>
          </div>
          <div v-if="createError" class="modal-error">{{ createError }}</div>
          <div class="modal-foot">
            <button class="btn-secondary" @click="createModal = false">{{ t('cancel') }}</button>
            <button class="btn-primary" @click="confirmCreateTable" :disabled="createLoading || !newTableName.trim()">
              <span v-if="createLoading" class="spin-sm" /> {{ createLoading ? '' : 'Crear tabla' }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- ── ADD COLUMN MODAL ── -->
    <Teleport to="body">
      <div v-if="addColModal" class="modal-overlay" @click.self="addColModal = false">
        <div class="modal-box modal-wide">
          <div class="modal-head">
            <span>Agregar columna — {{ tableName }}</span>
            <button class="modal-close" @click="addColModal = false">×</button>
          </div>
          <div class="modal-body">
            <div class="insert-grid">
              <label class="insert-label">{{ t('name') }}</label>
              <input class="insert-input" v-model="addColDef.name" :placeholder="t('columnNameHeader')" autofocus />
              <label class="insert-label">{{ t('type') }}</label>
              <select class="insert-input" v-model="addColDef.type">
                <option v-for="t in COLUMN_TYPES" :key="t" :value="t">{{ t }}</option>
              </select>
              <label class="insert-label">DEFAULT</label>
              <input class="insert-input" v-model="addColDef.default_" :placeholder="t('emptyNoDefaultHint')" />
              <label class="insert-label">{{ t('optionsLabel') }}</label>
              <div style="display:flex;gap:14px;align-items:center">
                <label class="opt-label"><input type="checkbox" v-model="addColDef.nullable" style="accent-color:var(--accent)"> NULL permitido</label>
                <label class="opt-label"><input type="checkbox" v-model="addColDef.ai" :disabled="!addColDef.type.startsWith('INT') && !addColDef.type.startsWith('BIG')" style="accent-color:var(--accent)"> AUTO_INCREMENT</label>
              </div>
            </div>
            <div class="ct-preview" style="margin-top:10px">
              <div class="ct-preview-label">SQL generado:</div>
              <pre class="ct-preview-sql">ALTER TABLE {{ quoteTable(tableName) }} ADD COLUMN {{ addColDef.name ? quoteId(addColDef.name) : '`nombre`' }} {{ addColDef.type }}{{ addColDef.nullable ? '' : ' NOT NULL' }}{{ addColDef.ai ? ' AUTO_INCREMENT' : '' }}{{ addColDef.default_ ? ` DEFAULT '${addColDef.default_}'` : '' }}</pre>
            </div>
          </div>
          <div v-if="addColError" class="modal-error">{{ addColError }}</div>
          <div class="modal-foot">
            <button class="btn-secondary" @click="addColModal = false">{{ t('cancel') }}</button>
            <button class="btn-primary" @click="confirmAddCol" :disabled="addColLoading">
              <span v-if="addColLoading" class="spin-sm" /> {{ addColLoading ? '' : 'Agregar columna' }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- ── SAVE QUERY MODAL ── -->
    <Teleport to="body">
      <div v-if="saveModal" class="modal-overlay" @click.self="saveModal = false">
        <div class="modal-box">
          <div class="modal-head"><span>{{ t('saveQueryTitle') }}</span><button class="modal-close" @click="saveModal = false">×</button></div>
          <div class="modal-body">
            <input v-model="saveName" class="ct-name-input save-name-input" :placeholder="t('queryNamePlaceholder')"
              @keydown.enter="confirmSave" @keydown.esc="saveModal = false" autofocus />
          </div>
          <div class="modal-foot">
            <button class="btn-secondary" @click="saveModal = false">{{ t('cancel') }}</button>
            <button class="btn-primary" @click="confirmSave">{{ t('save') }}</button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.db-view { display: flex; flex-direction: column; height: 100%; background: var(--bg-darkest); font-family: var(--font-ui); overflow: hidden; }

/* Top bar */
.db-view-bar { display: flex; align-items: center; gap: 10px; padding: 0 12px; height: 40px; flex-shrink: 0; background: var(--bg-dark); border-bottom: 1px solid var(--border); }
.dv-title    { display: flex; align-items: center; gap: 7px; font-size: 13px; font-weight: 600; color: var(--fg); }
.dv-driver   { font-size: 10px; color: var(--accent); background: rgba(208,208,216,0.12); border: 1px solid rgba(208,208,216,0.25); border-radius: 10px; padding: 1px 7px; font-weight: 600; }
.dv-rename-input {
  background: var(--bg-mid); border: 1px solid var(--accent); border-radius: 4px;
  color: var(--fg); font-size: 13px; font-weight: 600; padding: 1px 6px; outline: none; font-family: var(--font-ui);
}
.dv-title-btn {
  width: 20px; height: 20px; background: none; border: none; border-radius: 4px;
  color: var(--fg-muted); cursor: pointer; display: flex; align-items: center; justify-content: center; flex-shrink: 0;
}
.dv-title-btn:hover { background: var(--bg-hover); color: var(--fg); }
.dv-title-del:hover { color: #f85149 !important; }
.dv-table-action-error {
  margin: 8px 12px 0; padding: 6px 10px; background: rgba(248,81,73,0.12);
  border: 1px solid rgba(248,81,73,0.3); border-radius: 6px;
  font-size: 11.5px; color: #f85149; display: flex; justify-content: space-between; gap: 8px; flex-shrink: 0;
}
.dv-table-action-error button { background: none; border: none; color: inherit; cursor: pointer; }
.dv-tabs     { display: flex; gap: 1px; background: var(--bg-mid); border-radius: 6px; padding: 2px; }
.dv-tab      { padding: 3px 11px; border: none; border-radius: 5px; background: none; color: var(--fg-muted); font-size: 11.5px; font-weight: 600; cursor: pointer; transition: all 0.12s; }
.dv-tab:hover  { color: var(--fg); }
.dv-tab.active { background: var(--bg-active); color: var(--fg); }
.dv-actions  { margin-left: auto; display: flex; align-items: center; gap: 6px; }
.dv-refresh  { width: 26px; height: 26px; background: none; border: 1px solid var(--border); border-radius: 5px; color: var(--fg-muted); cursor: pointer; display: flex; align-items: center; justify-content: center; transition: all 0.12s; }
.dv-refresh:hover { color: var(--accent); border-color: var(--accent); }
.dv-limit    { background: var(--bg-mid); border: 1px solid var(--border); border-radius: 5px; color: var(--fg-dim); font-size: 11.5px; padding: 3px 6px; outline: none; cursor: pointer; }
.dv-action-btn { display: flex; align-items: center; gap: 4px; background: none; border: 1px solid var(--border); border-radius: 5px; color: var(--fg-muted); font-size: 11px; font-weight: 600; padding: 3px 9px; cursor: pointer; transition: all 0.12s; }
.dv-action-btn:hover { color: var(--fg); border-color: var(--fg-muted); }
.dv-insert-btn:hover { color: var(--accent); border-color: var(--accent); }

/* States */
.dv-loading      { display: flex; align-items: center; gap: 8px; padding: 24px; font-size: 13px; color: var(--fg-muted); }
.dv-spinner      { width: 16px; height: 16px; border: 2px solid rgba(208,208,216,0.3); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.7s linear infinite; flex-shrink: 0; }
@keyframes spin  { to { transform: rotate(360deg); } }
.dv-error        { display: flex; align-items: flex-start; gap: 10px; padding: 16px; color: #f85149; flex-shrink: 0; }
.dv-error pre    { font-family: var(--font-mono); font-size: 12px; line-height: 1.5; white-space: pre-wrap; word-break: break-all; margin: 0; }
.dv-empty        { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; color: var(--fg-muted); font-size: 12.5px; text-align: center; padding: 20px; }
.dv-inline-error { padding: 6px 14px; background: rgba(248,81,73,0.1); color: #f85149; font-size: 11.5px; border-bottom: 1px solid rgba(248,81,73,0.2); }

/* Grid */
.dv-grid-wrap  { flex: 1; overflow: auto; min-height: 0; }
.dv-grid-wrap::-webkit-scrollbar { width: 8px; height: 8px; }
.dv-grid-wrap::-webkit-scrollbar-thumb { background: var(--bg-active); border-radius: 4px; }
.dv-grid-wrap::-webkit-scrollbar-corner { background: var(--bg-darkest); }
.dv-grid       { border-collapse: collapse; width: max-content; min-width: 100%; font-size: 12.5px; font-family: var(--font-mono); }
.dv-grid th    { padding: 6px 12px; text-align: left; background: var(--bg-panel); color: var(--fg-dim); border-bottom: 2px solid var(--border); border-right: 1px solid var(--border); white-space: nowrap; position: sticky; top: 0; z-index: 1; font-size: 11px; letter-spacing: 0.3px; font-family: var(--font-ui); }
.dv-grid td    { padding: 4px 12px; border-bottom: 1px solid rgba(255,255,255,0.03); border-right: 1px solid rgba(255,255,255,0.03); white-space: nowrap; max-width: 280px; overflow: hidden; text-overflow: ellipsis; color: var(--fg); }
.dv-grid tr:hover td { background: rgba(208,208,216,0.05); }
.row-editing td { background: rgba(208,208,216,0.06) !important; }
.dv-row-num    { color: var(--fg-muted) !important; font-size: 10.5px !important; user-select: none; min-width: 36px; text-align: right !important; padding-right: 8px !important; background: var(--bg-mid) !important; border-right: 1px solid var(--border) !important; }
.cell-null     { color: var(--fg-muted) !important; font-style: italic; }
.dv-footer     { padding: 5px 14px; font-size: 11px; color: var(--fg-muted); border-top: 1px solid var(--border); background: var(--bg-dark); flex-shrink: 0; }
.dv-pk-hint    { color: var(--accent); opacity: 0.7; }

/* Actions column */
.dv-actions-th { width: 60px; min-width: 60px; background: var(--bg-panel) !important; }
.dv-actions-td { padding: 2px 4px !important; white-space: nowrap; background: transparent; }
.row-btn       { width: 22px; height: 22px; border: none; border-radius: 4px; background: none; cursor: pointer; display: inline-flex; align-items: center; justify-content: center; opacity: 0; transition: all 0.1s; color: var(--fg-muted); font-size: 13px; }
.dv-grid tr:hover .row-btn { opacity: 1; }
.row-edit:hover  { background: rgba(208,208,216,0.15); color: var(--accent); }
.row-del:hover   { background: rgba(248,81,73,0.15);  color: #f85149; }
.row-save        { color: var(--accent); opacity: 1 !important; font-weight: 700; }
.row-save:hover  { background: rgba(208,208,216,0.2); }
.row-cancel      { color: #f85149; opacity: 1 !important; font-weight: 700; }
.row-cancel:hover { background: rgba(248,81,73,0.15); }

/* Edit cells */
.dv-edit-cell  { padding: 2px 4px !important; }
.cell-input    { width: 100%; min-width: 80px; background: var(--bg-mid); border: 1px solid var(--accent); border-radius: 3px; color: var(--fg); font-size: 12px; font-family: var(--font-mono); padding: 2px 6px; outline: none; }

/* Structure grid extras */
.struct-grid td { max-width: 300px; }
.cell-pk        { color: #f7df1e !important; font-weight: 700; }
.cell-type      { color: #79c0ff !important; }
.cell-extra     { color: var(--accent) !important; font-style: italic; }

/* Query toolbar */
.dv-query-toolbar  { display: flex; align-items: center; gap: 10px; padding: 6px 14px; background: var(--bg-dark); border-bottom: 1px solid var(--border); flex-shrink: 0; }
.dv-query-hint     { font-size: 10.5px; color: var(--fg-muted); flex: 1; }
.dv-query-hint kbd { background: var(--bg-mid); border: 1px solid var(--border); border-radius: 3px; padding: 1px 5px; font-size: 10px; font-family: var(--font-mono); color: var(--fg-dim); }
.dv-query-actions  { display: flex; align-items: center; gap: 5px; }
.dv-q-btn          { display: flex; align-items: center; gap: 4px; background: none; border: 1px solid var(--border); border-radius: 5px; color: var(--fg-muted); font-size: 11px; font-weight: 600; padding: 4px 9px; cursor: pointer; transition: all 0.12s; font-family: var(--font-ui); }
.dv-q-btn:hover    { color: var(--fg); border-color: var(--fg-muted); }
.dv-q-btn.active   { color: var(--accent); border-color: var(--accent); background: rgba(208,208,216,0.08); }
.hist-count        { background: var(--accent); color: var(--accent-fg); border-radius: 8px; font-size: 9px; padding: 0 4px; min-width: 14px; text-align: center; }
.dv-run-btn        { display: flex; align-items: center; gap: 5px; background: var(--accent); border: none; border-radius: 5px; color: var(--accent-fg); font-size: 12px; font-weight: 600; padding: 5px 14px; cursor: pointer; transition: opacity 0.12s; }
.dv-run-btn:hover  { opacity: 0.85; }
.dv-run-btn:disabled { opacity: 0.4; cursor: default; }

/* Editor */
.dv-editor-wrap    { flex-shrink: 0; padding: 8px 14px 0; background: var(--bg-dark); overflow: hidden; }
.dv-cm-editor      { height: 100%; border: 1px solid var(--border); border-radius: 6px; overflow: hidden; }
.dv-cm-editor:focus-within { border-color: var(--accent); }
.dv-resize-handle  { flex-shrink: 0; height: 10px; background: var(--bg-dark); border-bottom: 1px solid var(--border); display: flex; align-items: center; justify-content: center; cursor: ns-resize; user-select: none; }
.dv-resize-handle:hover .dv-resize-dots, .dv-resize-handle.dragging .dv-resize-dots { background: var(--accent); }
.dv-resize-dots    { width: 30px; height: 3px; border-radius: 2px; background: var(--border); transition: background 0.12s; }

/* Query body */
.dv-query-body    { flex: 1; display: flex; overflow: hidden; min-height: 0; }
.dv-results-area  { flex: 1; display: flex; flex-direction: column; overflow: hidden; }

/* History */
.dv-history-panel { width: 240px; flex-shrink: 0; overflow-y: auto; border-right: 1px solid var(--border); padding: 8px 6px; display: flex; flex-direction: column; gap: 3px; background: var(--bg-dark); }
.dv-history-panel::-webkit-scrollbar { width: 4px; }
.dv-history-panel::-webkit-scrollbar-thumb { background: var(--bg-active); border-radius: 2px; }
.hist-section-title { font-size: 9.5px; font-weight: 700; letter-spacing: 0.8px; color: var(--fg-muted); text-transform: uppercase; padding: 4px 4px 2px; }
.hist-empty      { font-size: 11px; color: var(--fg-muted); padding: 4px 6px; font-style: italic; }
.hist-item       { padding: 6px 8px; border-radius: 5px; cursor: pointer; border: 1px solid transparent; transition: all 0.1s; }
.hist-item:hover { background: var(--bg-hover); border-color: var(--border); }
.hist-item--pinned { border-color: rgba(208,208,216,0.2); background: rgba(208,208,216,0.04); }
.hist-item--pinned:hover { background: rgba(208,208,216,0.1); }
.hist-item-name  { font-size: 11.5px; font-weight: 600; color: var(--fg); margin-bottom: 2px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.hist-item-sql   { font-size: 10.5px; color: var(--fg-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.hist-item-sql--mono { font-family: var(--font-mono); }
.hist-item-foot  { display: flex; align-items: center; justify-content: space-between; margin-top: 3px; }
.hist-ts         { font-size: 10px; color: var(--fg-muted); opacity: 0.6; }
.hist-del        { background: none; border: none; color: var(--fg-muted); font-size: 14px; cursor: pointer; padding: 0 2px; opacity: 0; transition: opacity 0.1s; }
.hist-item:hover .hist-del { opacity: 1; }
.hist-del:hover  { color: var(--red); }

/* ── Modals ── */
.modal-overlay { position: fixed; inset: 0; z-index: 99999; background: rgba(0,0,0,0.6); display: flex; align-items: center; justify-content: center; backdrop-filter: blur(3px); }
.modal-box     { background: var(--bg-panel); border: 1px solid var(--border); border-radius: 10px; box-shadow: 0 20px 60px rgba(0,0,0,0.7); display: flex; flex-direction: column; width: 380px; max-height: 90vh; overflow: hidden; }
.modal-wide    { width: 540px; }
.modal-xl      { width: 760px; }
.modal-head    { display: flex; align-items: center; justify-content: space-between; padding: 14px 16px; border-bottom: 1px solid var(--border); font-size: 13px; font-weight: 700; color: var(--fg-bright); flex-shrink: 0; }
.modal-close   { background: none; border: none; color: var(--fg-muted); font-size: 18px; cursor: pointer; padding: 0 4px; transition: color 0.1s; }
.modal-close:hover { color: var(--fg); }
.modal-body    { padding: 14px 16px; overflow-y: auto; flex: 1; }
.modal-body::-webkit-scrollbar { width: 4px; }
.modal-body::-webkit-scrollbar-thumb { background: var(--bg-active); border-radius: 2px; }
.modal-foot    { display: flex; gap: 8px; padding: 12px 16px; border-top: 1px solid var(--border); flex-shrink: 0; justify-content: flex-end; }
.modal-error   { padding: 8px 16px; background: rgba(248,81,73,0.1); color: #f85149; font-size: 12px; border-top: 1px solid rgba(248,81,73,0.2); }
.btn-primary   { background: var(--accent); border: none; border-radius: 6px; color: var(--accent-fg); font-size: 12px; font-weight: 700; padding: 7px 18px; cursor: pointer; transition: opacity 0.12s; display: flex; align-items: center; gap: 6px; }
.btn-primary:hover { opacity: 0.85; }
.btn-primary:disabled { opacity: 0.4; cursor: default; }
.btn-secondary { background: var(--bg-hover); border: 1px solid var(--border); border-radius: 6px; color: var(--fg); font-size: 12px; font-weight: 600; padding: 7px 14px; cursor: pointer; }

/* Insert modal */
.insert-grid   { display: grid; grid-template-columns: auto 1fr; gap: 8px 12px; align-items: center; }
.insert-label  { font-size: 12px; color: var(--fg-dim); white-space: nowrap; text-align: right; font-family: var(--font-mono); }
.insert-input  { background: var(--bg-mid); border: 1px solid var(--border); border-radius: 5px; color: var(--fg); font-size: 12px; font-family: var(--font-mono); padding: 5px 8px; outline: none; }
.insert-input:focus { border-color: var(--accent); }
.insert-password-row { display: flex; gap: 6px; }
.insert-password-row .insert-input:first-child { flex: 1; min-width: 0; }
.insert-hash-select { flex-shrink: 0; width: 110px; }

/* Create table modal */
.ct-row        { display: flex; align-items: center; gap: 10px; margin-bottom: 14px; }
.ct-label      { font-size: 12px; color: var(--fg-dim); white-space: nowrap; }
.ct-name-input { flex: 1; background: var(--bg-mid); border: 1px solid var(--border); border-radius: 5px; color: var(--fg); font-size: 12px; font-family: var(--font-mono); padding: 6px 10px; outline: none; }
.ct-name-input:focus { border-color: var(--accent); }
.ct-cols-header { display: flex; align-items: center; gap: 6px; padding: 4px 0; margin-bottom: 4px; }
.ct-col-h      { font-size: 10px; font-weight: 700; color: var(--fg-muted); text-transform: uppercase; letter-spacing: 0.5px; }
.ct-center     { flex: 1; text-align: center; }
.ct-col-row    { display: flex; align-items: center; gap: 6px; margin-bottom: 5px; }
.ct-input      { background: var(--bg-mid); border: 1px solid var(--border); border-radius: 4px; color: var(--fg); font-size: 12px; font-family: var(--font-mono); padding: 4px 7px; outline: none; min-width: 0; }
.ct-input:focus { border-color: var(--accent); }
.ct-select     { background: var(--bg-mid); border: 1px solid var(--border); border-radius: 4px; color: var(--fg); font-size: 12px; padding: 4px 5px; outline: none; min-width: 0; }
.ct-check      { width: 15px; height: 15px; flex: 1; cursor: pointer; accent-color: var(--accent); }
.ct-del-col    { width: 22px; height: 22px; background: none; border: none; color: var(--fg-muted); font-size: 16px; cursor: pointer; border-radius: 4px; flex-shrink: 0; }
.ct-del-col:hover:not(:disabled) { background: rgba(248,81,73,0.15); color: #f85149; }
.ct-del-col:disabled { opacity: 0.2; cursor: default; }
.ct-add-col    { margin-top: 6px; background: none; border: 1px dashed var(--border); border-radius: 5px; color: var(--fg-muted); font-size: 11.5px; padding: 5px 12px; cursor: pointer; width: 100%; transition: all 0.12s; }
.ct-add-col:hover { color: var(--accent); border-color: var(--accent); }
.ct-preview    { margin-top: 12px; }
.ct-preview-label { font-size: 10.5px; color: var(--fg-muted); margin-bottom: 4px; }
.ct-fk-section { margin-top: 4px; }
.ct-fk-hint { font-size: 10.5px; color: var(--fg-muted); margin-top: 6px; }
.ct-preview-sql { background: var(--bg-dark); border: 1px solid var(--border); border-radius: 5px; padding: 8px 10px; font-size: 11.5px; font-family: var(--font-mono); color: var(--accent); white-space: pre-wrap; word-break: break-all; margin: 0; max-height: 120px; overflow-y: auto; }

.spin-sm { width: 12px; height: 12px; border: 2px solid rgba(255,255,255,0.3); border-top-color: #fff; border-radius: 50%; animation: spin 0.7s linear infinite; flex-shrink: 0; }

/* Structure footer + add col btn */
.struct-footer { display: flex; align-items: center; gap: 12px; }
.struct-add-col-btn { margin-left: auto; display: flex; align-items: center; gap: 4px; background: none; border: 1px dashed var(--accent); border-radius: 5px; color: var(--accent); font-size: 11px; font-weight: 600; padding: 3px 9px; cursor: pointer; transition: all 0.12s; opacity: 0.7; }
.struct-add-col-btn:hover { opacity: 1; background: rgba(208,208,216,0.08); }

/* Insert opt label */
.opt-label { font-size: 11.5px; color: var(--fg-dim); display: flex; align-items: center; gap: 5px; cursor: pointer; }
</style>
