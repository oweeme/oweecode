<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, reactive } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '../composables/useI18n'

const props = defineProps<{ connId: string; connName: string; driver: string }>()
const { t } = useI18n()

interface ErColumn { name: string; data_type: string; pk: boolean; nullable: boolean }
interface ErTable { name: string; columns: ErColumn[] }
interface ErRelationship { name: string; from_table: string; from_column: string; to_table: string; to_column: string }
interface ErSchema { tables: ErTable[]; relationships: ErRelationship[] }

const schema = ref<ErSchema | null>(null)
const loading = ref(false)
const error = ref('')
const msg = ref('')

const CARD_WIDTH = 220
const HEADER_H = 30
const ROW_H = 22

type Pos = { x: number; y: number }
const positions = reactive<Record<string, Pos>>({})
const zOrder = ref<string[]>([])

const posKey = computed(() => `er_positions_${props.connId}`)

function loadPositions(): Record<string, Pos> {
  try { return JSON.parse(localStorage.getItem(posKey.value) ?? '{}') } catch { return {} }
}
function savePositions() {
  localStorage.setItem(posKey.value, JSON.stringify(positions))
}

function layoutGrid(tables: ErTable[], force = false) {
  const saved = force ? {} : loadPositions()
  const perRow = 4
  tables.forEach((tbl, i) => {
    if (!force && saved[tbl.name]) {
      positions[tbl.name] = saved[tbl.name]
    } else if (force || !positions[tbl.name]) {
      positions[tbl.name] = { x: 40 + (i % perRow) * (CARD_WIDTH + 60), y: 40 + Math.floor(i / perRow) * 260 }
    }
  })
  zOrder.value = tables.map(t => t.name)
}

async function loadSchema() {
  loading.value = true
  error.value = ''
  try {
    schema.value = await invoke<ErSchema>('db_get_er_schema', { id: props.connId })
    layoutGrid(schema.value.tables)
  } catch (e: any) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

function bringToFront(name: string) {
  zOrder.value = [...zOrder.value.filter(n => n !== name), name]
}

// ── Pan / zoom ──────────────────────────────────────────────────────────────
const panX = ref(40)
const panY = ref(20)
const scale = ref(1)
const canvasEl = ref<HTMLElement | null>(null)

function onWheel(e: WheelEvent) {
  e.preventDefault()
  const delta = e.deltaY > 0 ? -0.08 : 0.08
  scale.value = Math.max(0.15, Math.min(2, scale.value + delta))
}

function zoomStep(dir: 1 | -1) {
  scale.value = Math.max(0.15, Math.min(2, scale.value + dir * 0.1))
}

function onKeydown(e: KeyboardEvent) {
  if (!(e.ctrlKey || e.metaKey)) return
  // This listener is a window-wide fallback (the diagram has no natural
  // single focusable element to attach to), so without this check it fired
  // Ctrl+=/-/0 even while focus was in an unrelated panel (terminal, a
  // sidebar input) just because an ER-diagram tab happened to be active.
  if (!(e.target as HTMLElement | null)?.closest('.er-view')) return
  if (e.key === '+' || e.key === '=') { e.preventDefault(); zoomStep(1) }
  else if (e.key === '-' || e.key === '_') { e.preventDefault(); zoomStep(-1) }
  else if (e.key === '0') { e.preventDefault(); resetView() }
}

let panning = false
let panStart = { x: 0, y: 0, panX: 0, panY: 0 }
function onCanvasMousedown(e: MouseEvent) {
  // Grab focus on any click inside the diagram (including on a card), not
  // just the raw background — needed so the Ctrl+=/-/0 zoom shortcuts below
  // know this diagram is what the user means, instead of firing globally.
  canvasEl.value?.focus()
  if (e.target !== canvasEl.value) return
  panning = true
  panStart = { x: e.clientX, y: e.clientY, panX: panX.value, panY: panY.value }
  selectedRel.value = null
  window.addEventListener('mousemove', onCanvasMousemove)
  window.addEventListener('mouseup', onCanvasMouseup)
}
function onCanvasMousemove(e: MouseEvent) {
  if (!panning) return
  panX.value = panStart.panX + (e.clientX - panStart.x)
  panY.value = panStart.panY + (e.clientY - panStart.y)
}
function onCanvasMouseup() {
  panning = false
  window.removeEventListener('mousemove', onCanvasMousemove)
  window.removeEventListener('mouseup', onCanvasMouseup)
}

function resetView() { panX.value = 40; panY.value = 20; scale.value = 1 }
function reflow() {
  for (const k of Object.keys(positions)) delete positions[k]
  if (schema.value) layoutGrid(schema.value.tables, true)
  savePositions()
  resetView()
}

function zoomToFit() {
  const names = Object.keys(positions)
  if (!names.length || !canvasEl.value) return
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity
  for (const name of names) {
    const p = positions[name]
    const tbl = schema.value?.tables.find(t => t.name === name)
    const h = HEADER_H + (tbl?.columns.length ?? 0) * ROW_H
    minX = Math.min(minX, p.x); minY = Math.min(minY, p.y)
    maxX = Math.max(maxX, p.x + CARD_WIDTH); maxY = Math.max(maxY, p.y + h)
  }
  const pad = 60
  const contentW = maxX - minX + pad * 2
  const contentH = maxY - minY + pad * 2
  const rect = canvasEl.value.getBoundingClientRect()
  const nextScale = Math.max(0.2, Math.min(1.5, Math.min(rect.width / contentW, rect.height / contentH)))
  scale.value = nextScale
  panX.value = (rect.width - (maxX - minX) * nextScale) / 2 - minX * nextScale
  panY.value = (rect.height - (maxY - minY) * nextScale) / 2 - minY * nextScale
}

// ── Card dragging ─────────────────────────────────────────────────────────────
let dragging: string | null = null
let dragStart = { x: 0, y: 0, cardX: 0, cardY: 0 }
function onCardMousedown(name: string, e: MouseEvent) {
  e.stopPropagation()
  bringToFront(name)
  dragging = name
  dragStart = { x: e.clientX, y: e.clientY, cardX: positions[name].x, cardY: positions[name].y }
  window.addEventListener('mousemove', onCardMousemove)
  window.addEventListener('mouseup', onCardMouseup)
}
function onCardMousemove(e: MouseEvent) {
  if (!dragging) return
  const dx = (e.clientX - dragStart.x) / scale.value
  const dy = (e.clientY - dragStart.y) / scale.value
  positions[dragging] = { x: dragStart.cardX + dx, y: dragStart.cardY + dy }
}
function onCardMouseup() {
  dragging = null
  savePositions()
  window.removeEventListener('mousemove', onCardMousemove)
  window.removeEventListener('mouseup', onCardMouseup)
}

// ── Relationship geometry ──────────────────────────────────────────────────────
function columnRowIndex(tableName: string, colName: string): number {
  const tbl = schema.value?.tables.find(t => t.name === tableName)
  if (!tbl) return 0
  const idx = tbl.columns.findIndex(c => c.name === colName)
  return idx < 0 ? 0 : idx
}

function rowY(tableName: string, colName: string): number {
  const p = positions[tableName]
  if (!p) return 0
  return p.y + HEADER_H + columnRowIndex(tableName, colName) * ROW_H + ROW_H / 2
}

interface LinePath { rel: ErRelationship; d: string; midX: number; midY: number }

const linePaths = computed<LinePath[]>(() => {
  if (!schema.value) return []
  const out: LinePath[] = []
  for (const rel of schema.value.relationships) {
    const fromPos = positions[rel.from_table]
    const toPos = positions[rel.to_table]
    if (!fromPos || !toPos) continue
    const fromCenterX = fromPos.x + CARD_WIDTH / 2
    const toCenterX = toPos.x + CARD_WIDTH / 2
    const fromRight = fromCenterX < toCenterX
    const x1 = fromRight ? fromPos.x + CARD_WIDTH : fromPos.x
    const y1 = rowY(rel.from_table, rel.from_column)
    const x2 = fromRight ? toPos.x : toPos.x + CARD_WIDTH
    const y2 = rowY(rel.to_table, rel.to_column) || (toPos.y + HEADER_H / 2)
    const dx = Math.max(40, Math.abs(x2 - x1) / 2)
    const c1x = fromRight ? x1 + dx : x1 - dx
    const c2x = fromRight ? x2 - dx : x2 + dx
    out.push({
      rel,
      d: `M ${x1} ${y1} C ${c1x} ${y1}, ${c2x} ${y2}, ${x2} ${y2}`,
      midX: (x1 + x2) / 2, midY: (y1 + y2) / 2,
    })
  }
  return out
})

const selectedRel = ref<string | null>(null)

async function deleteRelationship(rel: ErRelationship) {
  if (props.driver === 'sqlite') {
    error.value = t('sqliteNoAlterFk')
    return
  }
  if (!confirm(`${t('confirmDeleteRel')} "${rel.name}"?`)) return
  try {
    await invoke('db_drop_relationship', { id: props.connId, driver: props.driver, table: rel.from_table, constraintName: rel.name })
    msg.value = t('relationshipDeleted')
    selectedRel.value = null
    await loadSchema()
  } catch (e: any) { error.value = String(e) }
}

// ── Add relationship form ──────────────────────────────────────────────────────
const showAddForm = ref(false)
const newRel = reactive({ fromTable: '', fromColumn: '', toTable: '', toColumn: '', name: '' })

function openAddForm(fromTable?: string, fromColumn?: string) {
  if (props.driver === 'sqlite') { error.value = t('sqliteNoAlterFk'); return }
  const t0 = fromTable ?? schema.value?.tables[0]?.name ?? ''
  const c0 = fromColumn ?? schema.value?.tables.find(t => t.name === t0)?.columns[0]?.name ?? ''
  const otherTable = schema.value?.tables.find(t => t.name !== t0)?.name ?? t0
  newRel.fromTable = t0
  newRel.fromColumn = c0
  newRel.toTable = otherTable
  newRel.toColumn = schema.value?.tables.find(t => t.name === otherTable)?.columns.find(c => c.pk)?.name
    ?? schema.value?.tables.find(t => t.name === otherTable)?.columns[0]?.name ?? ''
  newRel.name = `fk_${t0}_${c0}`
  showAddForm.value = true
}

const columnsForTable = (tableName: string) => schema.value?.tables.find(t => t.name === tableName)?.columns ?? []

async function confirmAddRelationship() {
  if (!newRel.fromTable || !newRel.fromColumn || !newRel.toTable || !newRel.toColumn || !newRel.name) return
  try {
    await invoke('db_add_relationship', {
      id: props.connId, driver: props.driver,
      table: newRel.fromTable, column: newRel.fromColumn,
      refTable: newRel.toTable, refColumn: newRel.toColumn,
      constraintName: newRel.name,
    })
    msg.value = t('relationshipCreated')
    showAddForm.value = false
    await loadSchema()
  } catch (e: any) { error.value = String(e) }
}

function onSchemaChanged(e: Event) {
  const detail = (e as CustomEvent).detail
  if (detail?.connId === props.connId) loadSchema()
}

onMounted(() => {
  loadSchema()
  window.addEventListener('db-schema-changed', onSchemaChanged)
  window.addEventListener('keydown', onKeydown)
})
onBeforeUnmount(() => {
  window.removeEventListener('db-schema-changed', onSchemaChanged)
  window.removeEventListener('keydown', onKeydown)
})
</script>

<template>
  <div class="er-view">
    <div class="er-toolbar">
      <span class="er-title">{{ connName }} — {{ t('erDiagramTitle') }}</span>
      <div class="er-toolbar-actions">
        <button class="er-btn" @click="loadSchema" :title="t('reload')">⟳</button>
        <button class="er-btn" @click="reflow" :title="t('erAutoArrange')">▦</button>
        <button class="er-btn" @click="zoomToFit" :title="t('erZoomToFit')">⛶</button>
        <button class="er-btn" @click="resetView" :title="t('erResetView')">⊙</button>
        <button class="er-btn er-btn--primary" @click="openAddForm()" :disabled="!schema || schema.tables.length < 1">
          + {{ t('erNewRelationship') }}
        </button>
      </div>
    </div>

    <div v-if="error" class="er-msg er-msg--err">{{ error }} <button @click="error = ''">×</button></div>
    <div v-if="msg" class="er-msg er-msg--ok">{{ msg }} <button @click="msg = ''">×</button></div>

    <div v-if="loading" class="er-loading">{{ t('loading') }}</div>

    <div
      v-else
      ref="canvasEl"
      class="er-canvas"
      tabindex="0"
      @wheel="onWheel"
      @mousedown="onCanvasMousedown"
    >
      <div class="er-canvas-inner" :style="{ transform: `translate(${panX}px, ${panY}px) scale(${scale})` }">
        <svg class="er-svg" :width="4000" :height="3000">
          <path
            v-for="lp in linePaths" :key="lp.rel.name"
            :d="lp.d"
            class="er-edge"
            :class="{ selected: selectedRel === lp.rel.name }"
            @click.stop="selectedRel = selectedRel === lp.rel.name ? null : lp.rel.name"
          />
          <circle v-for="lp in linePaths" :key="`${lp.rel.name}-dot`" :cx="lp.midX" :cy="lp.midY" r="3" class="er-edge-dot" />
        </svg>

        <div
          v-for="tbl in (schema?.tables ?? [])" :key="tbl.name"
          class="er-card"
          :style="{
            left: (positions[tbl.name]?.x ?? 0) + 'px',
            top: (positions[tbl.name]?.y ?? 0) + 'px',
            width: CARD_WIDTH + 'px',
            zIndex: zOrder.indexOf(tbl.name) + 1,
          }"
        >
          <div class="er-card-header" @mousedown="onCardMousedown(tbl.name, $event)">
            <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" style="flex-shrink:0">
              <rect x="3" y="3" width="18" height="18" rx="1"/><path d="M3 9h18M3 15h18M9 3v18"/>
            </svg>
            <span class="er-card-title">{{ tbl.name }}</span>
          </div>
          <div class="er-card-rows">
            <div v-for="col in tbl.columns" :key="col.name" class="er-row">
              <span class="er-row-icon">
                <span v-if="col.pk" class="er-pk-dot" :title="t('primaryKey')">🔑</span>
              </span>
              <span class="er-row-name" :class="{ pk: col.pk }">{{ col.name }}</span>
              <span class="er-row-type">{{ col.data_type }}</span>
              <button class="er-row-link" :title="t('erNewRelationship')" @click.stop="openAddForm(tbl.name, col.name)">+</button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Selected relationship popover -->
    <div v-if="selectedRel" class="er-rel-popover">
      <span>{{ selectedRel }}</span>
      <button class="er-rel-del" @click="deleteRelationship(linePaths.find(l => l.rel.name === selectedRel)!.rel)">{{ t('delete') }}</button>
      <button class="er-rel-close" @click="selectedRel = null">×</button>
    </div>

    <!-- Add relationship modal -->
    <div v-if="showAddForm" class="er-modal-overlay" @click.self="showAddForm = false">
      <div class="er-modal">
        <div class="er-modal-title">{{ t('erNewRelationship') }}</div>

        <div class="er-modal-row">
          <div class="er-modal-field">
            <label>{{ t('erFromTable') }}</label>
            <select v-model="newRel.fromTable" @change="newRel.fromColumn = columnsForTable(newRel.fromTable)[0]?.name ?? ''">
              <option v-for="tb in (schema?.tables ?? [])" :key="tb.name" :value="tb.name">{{ tb.name }}</option>
            </select>
          </div>
          <div class="er-modal-field">
            <label>{{ t('column') }}</label>
            <select v-model="newRel.fromColumn">
              <option v-for="c in columnsForTable(newRel.fromTable)" :key="c.name" :value="c.name">{{ c.name }}</option>
            </select>
          </div>
        </div>

        <div class="er-modal-arrow">→ {{ t('erReferences') }}</div>

        <div class="er-modal-row">
          <div class="er-modal-field">
            <label>{{ t('erToTable') }}</label>
            <select v-model="newRel.toTable" @change="newRel.toColumn = columnsForTable(newRel.toTable)[0]?.name ?? ''">
              <option v-for="tb in (schema?.tables ?? [])" :key="tb.name" :value="tb.name">{{ tb.name }}</option>
            </select>
          </div>
          <div class="er-modal-field">
            <label>{{ t('column') }}</label>
            <select v-model="newRel.toColumn">
              <option v-for="c in columnsForTable(newRel.toTable)" :key="c.name" :value="c.name">{{ c.name }}</option>
            </select>
          </div>
        </div>

        <div class="er-modal-field">
          <label>{{ t('erConstraintName') }}</label>
          <input v-model="newRel.name" class="er-modal-input" />
        </div>

        <div class="er-modal-actions">
          <button class="er-modal-cancel" @click="showAddForm = false">{{ t('cancel') }}</button>
          <button class="er-modal-confirm" @click="confirmAddRelationship">{{ t('create') }}</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.er-view { display: flex; flex-direction: column; height: 100%; background: var(--bg-darkest); font-family: var(--font-ui); overflow: hidden; position: relative; }

.er-toolbar {
  display: flex; align-items: center; justify-content: space-between;
  padding: 8px 12px; background: var(--bg-dark); border-bottom: 1px solid var(--border); flex-shrink: 0;
}
.er-title { font-size: 12.5px; font-weight: 600; color: var(--fg); }
.er-toolbar-actions { display: flex; gap: 6px; }
.er-btn {
  width: 26px; height: 26px; background: var(--bg-mid); border: 1px solid var(--border);
  border-radius: 5px; color: var(--fg-dim); cursor: pointer; font-size: 13px;
  display: flex; align-items: center; justify-content: center;
}
.er-btn:hover { background: var(--bg-hover); color: var(--fg); }
.er-btn--primary {
  width: auto; padding: 0 10px; background: var(--accent); color: var(--accent-fg);
  font-size: 11.5px; font-weight: 600; border: none;
}
.er-btn--primary:disabled { opacity: 0.4; cursor: default; }

.er-msg { margin: 8px 12px 0; padding: 6px 10px; border-radius: 6px; font-size: 11.5px; display: flex; justify-content: space-between; gap: 8px; flex-shrink: 0; }
.er-msg--err { background: rgba(248,81,73,0.1); border: 1px solid rgba(248,81,73,0.3); color: #f85149; }
.er-msg--ok { background: rgba(166,227,161,0.1); border: 1px solid rgba(166,227,161,0.3); color: var(--green); }
.er-msg button { background: none; border: none; color: inherit; cursor: pointer; }

.er-loading { padding: 20px; color: var(--fg-muted); font-size: 12.5px; }

.er-canvas {
  flex: 1; position: relative; overflow: hidden; cursor: grab;
  background-image: radial-gradient(var(--border) 1px, transparent 1px);
  background-size: 22px 22px;
  outline: none;
}
.er-canvas:focus { box-shadow: inset 0 0 0 1px rgba(208,208,216,0.35); }
.er-canvas:active { cursor: grabbing; }
.er-canvas-inner { position: absolute; top: 0; left: 0; transform-origin: 0 0; }

.er-svg { position: absolute; top: 0; left: 0; overflow: visible; pointer-events: none; }
.er-edge { fill: none; stroke: var(--fg-muted); stroke-width: 1.6; pointer-events: stroke; cursor: pointer; }
.er-edge:hover, .er-edge.selected { stroke: var(--accent); stroke-width: 2.2; }
.er-edge-dot { fill: var(--bg-darkest); stroke: var(--fg-muted); stroke-width: 1; }

.er-card {
  position: absolute; background: var(--bg-panel); border: 1px solid var(--border);
  border-radius: 8px; box-shadow: 0 6px 20px rgba(0,0,0,0.35); overflow: hidden;
}
.er-card-header {
  display: flex; align-items: center; gap: 6px; padding: 0 10px; height: 30px;
  background: var(--bg-mid); border-bottom: 1px solid var(--border); cursor: grab; color: var(--fg-bright);
}
.er-card-header:active { cursor: grabbing; }
.er-card-title { font-size: 12px; font-weight: 700; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.er-card-rows { padding: 4px 0; }
.er-row { display: flex; align-items: center; gap: 4px; height: 22px; padding: 0 8px; font-size: 11px; }
.er-row:hover { background: var(--bg-hover); }
.er-row-icon { width: 12px; flex-shrink: 0; font-size: 9px; }
.er-row-name { flex: 1; color: var(--fg-dim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-mono); }
.er-row-name.pk { color: var(--fg-bright); font-weight: 700; }
.er-row-type { color: var(--fg-muted); font-size: 9.5px; flex-shrink: 0; max-width: 60px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.er-row-link {
  width: 14px; height: 14px; background: none; border: none; color: var(--fg-muted);
  cursor: pointer; font-size: 11px; flex-shrink: 0; opacity: 0; border-radius: 3px;
}
.er-row:hover .er-row-link { opacity: 1; }
.er-row-link:hover { color: var(--accent); background: var(--bg-active); }

.er-rel-popover {
  position: absolute; bottom: 16px; left: 50%; transform: translateX(-50%);
  display: flex; align-items: center; gap: 10px;
  background: var(--bg-panel); border: 1px solid var(--border); border-radius: 8px;
  padding: 8px 12px; box-shadow: 0 8px 24px rgba(0,0,0,0.4);
  font-size: 11.5px; color: var(--fg-dim); font-family: var(--font-mono);
}
.er-rel-del { background: rgba(248,81,73,0.15); border: 1px solid rgba(248,81,73,0.3); color: #f85149; border-radius: 5px; padding: 3px 10px; cursor: pointer; font-size: 11px; }
.er-rel-close { background: none; border: none; color: var(--fg-muted); cursor: pointer; font-size: 14px; }

.er-modal-overlay {
  position: absolute; inset: 0; background: rgba(0,0,0,0.6); backdrop-filter: blur(2px);
  display: flex; align-items: center; justify-content: center; z-index: 50;
}
.er-modal {
  background: var(--bg-panel); border: 1px solid var(--border); border-radius: 12px;
  padding: 16px; width: 340px; box-shadow: 0 20px 60px rgba(0,0,0,0.6);
}
.er-modal-title { font-size: 13.5px; font-weight: 700; color: var(--fg-bright); margin-bottom: 12px; }
.er-modal-row { display: flex; gap: 8px; }
.er-modal-field { flex: 1; margin-bottom: 10px; }
.er-modal-field label { display: block; font-size: 10px; color: var(--fg-muted); margin-bottom: 3px; }
.er-modal-field select, .er-modal-input {
  width: 100%; background: var(--bg-mid); border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg); font-size: 12px; padding: 5px 7px; outline: none; font-family: var(--font-ui);
}
.er-modal-arrow { text-align: center; font-size: 11px; color: var(--accent); margin-bottom: 8px; }
.er-modal-actions { display: flex; gap: 8px; margin-top: 6px; }
.er-modal-cancel {
  flex: 1; background: none; border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg-muted); font-size: 12px; padding: 6px; cursor: pointer;
}
.er-modal-confirm {
  flex: 2; background: var(--accent); border: none; border-radius: 6px;
  color: var(--accent-fg); font-size: 12px; font-weight: 600; padding: 6px; cursor: pointer;
}
</style>
