<script setup lang="ts">
// Business-flow designer (n8n style) + project architecture map.
//  - "Flujo": blocks you connect; each block can be bound to a script/command of
//    your project in ANY language (Node, TS, Python, PHP, Go, Rust, shell…) or
//    an HTTP call, and the whole flow can be executed step by step.
//  - Also: SQL schema + sample data, code skeleton per language, AI-built flows.
//  - "Mapa del proyecto": reads imports of an existing project (JS/TS/Vue, Python,
//    PHP, Go, Rust, HTML/CSS) and draws which file depends on which.
import { ref, computed, onMounted, toRefs, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { ask, open as openDialog } from '@tauri-apps/plugin-dialog'
import { VueFlow, Handle, Position, useVueFlow, type Connection } from '@vue-flow/core'
import { Background } from '@vue-flow/background'
import { Controls } from '@vue-flow/controls'
import { MiniMap } from '@vue-flow/minimap'
import '@vue-flow/core/dist/style.css'
import '@vue-flow/core/dist/theme-default.css'
import '@vue-flow/controls/dist/style.css'
import '@vue-flow/minimap/dist/style.css'
import { loadProviderKey, type Provider } from '../composables/aiProviders'
import { useEditorStore } from '../composables/useEditorStore'
import { flowState } from '../composables/useFlowState'
import { designRequest } from '../composables/designRequest'
import { buildSql, type Dialect } from '../composables/flowSeed'
import { buildCode, CODE_LANGS, type CodeLang } from '../composables/flowCodegen'
import { evalCondition, fill, fillSql, isReadQuery, mergeOutput, RUN_LANGS, type Ctx } from '../composables/flowRunner'
import { useDatabaseStore, type DbConnection } from '../composables/useDatabaseStore'
import { useI18n, type ExtraKey } from '../composables/useI18n'
import { extraEn } from '../composables/i18nExtra'
import { FLOW_KINDS, MAP_KINDS, kindInfo, type BlockKind } from '../composables/flowKinds'

const store = useEditorStore()
const { t } = useI18n()
const { connections: dbConns } = useDatabaseStore()
const { addEdges, fitView, setCenter, getViewport, setViewport } = useVueFlow()
const { nodeResults, resultTab, mode, nodes, edges, flowName, selectedId, framework, mapNotice, showAi, aiText, dialect, seedRows, codeLang, runInput, runLog, nodeStatus, running, showLog } = toRefs(flowState)

// block names end up as table / function names in the generated SQL and code: they stay English ASCII in every language
const EN_KIND = Object.fromEntries(Object.entries(extraEn).filter(([k]) => k.startsWith('wk_')).map(([k, v]) => [k.slice(3), v])) as Record<string, string>
const kl = (k: BlockKind) => t('wk_' + k.key as ExtraKey)
const RUNNABLE = ['function', 'db', 'email', 'trigger']   // kinds that can be bound to a command
let counter = 0

const status = ref('')
const savedFlows = ref<string[]>([])
let cancelRun = false

const root = computed(() => store.state.rootPath)
const flowsDir = computed(() => `${root.value}/.owee/flows`)
const safeName = () => flowName.value.trim().replace(/[^\w\-]+/g, '-').replace(/^-+|-+$/g, '') || 'flujo'
const selected = computed<any>(() => (nodes.value as any[]).find(n => n.id === selectedId.value) ?? null)

function say(msg: string) { status.value = msg; setTimeout(() => { if (status.value === msg) status.value = '' }, 4500) }

// ── Flow editing ────────────────────────────────────────────────────────────
function addBlock(kind: string) {
  counter++
  const info = kindInfo(kind)
  const id = `n${Date.now().toString(36)}${counter}`
  nodes.value.push({
    id, type: 'block',
    position: { x: 80 + (counter % 6) * 40, y: 80 + (counter % 8) * 60 },
    data: { kind, label: EN_KIND[kind] ?? info.label, desc: '', columns: '', cond: '', sql: '', dbConn: '',
      run: { lang: 'node', file: '', command: '', cwd: '' },
      http: { method: 'GET', url: '', headers: '', body: '' } },
  })
  selectedId.value = id
}

function onConnect(c: Connection) {
  addEdges([{ ...c, id: `e${c.source}-${c.sourceHandle ?? 'o'}-${c.target}`, animated: true, label: c.sourceHandle === 'yes' ? 'Sí' : c.sourceHandle === 'no' ? 'No' : undefined }])
}

function removeSelected() {
  if (!selectedId.value) return
  const id = selectedId.value
  nodes.value = nodes.value.filter(n => n.id !== id)
  edges.value = edges.value.filter(e => e.source !== id && e.target !== id)
  selectedId.value = null
}

// Picking a language (or file) fills the command from a template; the command stays editable.
function updateCommand(n: any) {
  const r = n.data.run
  const tpl = RUN_LANGS.find(l => l.key === r.lang)?.template ?? ''
  if (r.lang !== 'custom') r.command = tpl.replace('{file}', r.file || 'archivo')
}

async function refreshSaved() {
  if (!root.value) { savedFlows.value = []; return }
  try {
    const list = await invoke<{ name: string; is_dir: boolean }[]>('list_dir', { path: flowsDir.value, showHidden: true })
    savedFlows.value = list.filter(f => !f.is_dir && f.name.endsWith('.json')).map(f => f.name.replace(/\.json$/, ''))
  } catch { savedFlows.value = [] }
}

// No project open? Offer to create one (default ~/Dev/<name>) or pick a folder,
// so everything the user designs is always saved inside a real project.
async function ensureProject(): Promise<boolean> {
  if (root.value) return true
  const slug = flowName.value.trim().toLowerCase().replace(/[^a-z0-9\-_]+/g, '-').replace(/^-+|-+$/g, '') || 'mi-proyecto'
  try {
    const home = await invoke<string>('get_home_dir')
    const dir = `${home}/Dev/${slug}`
    const create = await ask(t('wAskCreate', { dir }),
      { title: t('wAskTitle'), kind: 'info', okLabel: t('wAskOk'), cancelLabel: t('wAskOther') })
    let target: string | null = dir
    if (create) await invoke('create_dir_cmd', { path: dir })
    else {
      const picked = await openDialog({ directory: true, title: t('wPickDir') })
      target = typeof picked === 'string' ? picked : null
    }
    if (!target) return false
    store.setRootPath(target)
    return true
  } catch (e) { say(t('wErrCreate', { e: String(e) })); return false }
}

async function saveFlow() {
  if (!(await ensureProject())) return
  const name = safeName()
  try {
    await invoke('create_dir_cmd', { path: flowsDir.value })
    const payload = {
      version: 2, name,
      nodes: nodes.value.map(n => ({ id: n.id, position: n.position, data: n.data })),
      edges: edges.value.map(e => ({ id: e.id, source: e.source, target: e.target, sourceHandle: e.sourceHandle, targetHandle: e.targetHandle, label: e.label })),
    }
    await invoke('save_file', { path: `${flowsDir.value}/${name}.json`, content: JSON.stringify(payload, null, 2) })
    flowName.value = name
    say(t('wSaved', { name }))
    refreshSaved()
  } catch (e) { say(t('wErrSave', { e: String(e) })) }
}

async function loadFlow(name: string) {
  if (!name) return
  try {
    const raw = await invoke<string>('open_file', { path: `${flowsDir.value}/${name}.json` })
    const d = JSON.parse(raw)
    nodes.value = (d.nodes ?? []).map((n: any) => ({
      ...n, type: 'block',
      // flows saved by older versions lack the run/http config
      data: { cond: '', columns: '', sql: '', dbConn: '', run: { lang: 'node', file: '', command: '', cwd: '' }, http: { method: 'GET', url: '', headers: '', body: '' }, ...n.data },
    }))
    edges.value = (d.edges ?? []).map((e: any) => ({ ...e, animated: true }))
    flowName.value = name
    selectedId.value = null
    nodeStatus.value = {}
    setTimeout(() => fitView({ padding: 0.2 }), 50)
  } catch (e) { say(t('wErrOpen', { e: String(e) })) }
}

function newFlow() { nodes.value = []; edges.value = []; selectedId.value = null; flowName.value = 'mi-flujo'; nodeStatus.value = {}; nodeResults.value = {}; runLog.value = [] }

async function copyMermaid() {
  const idMap = new Map(nodes.value.map((n, i) => [n.id, `N${i + 1}`]))
  const esc = (s: string) => String(s ?? '').replace(/["\n]/g, ' ')
  const lines = ['flowchart LR']
  for (const n of nodes.value) {
    const id = idMap.get(n.id), l = esc(n.data.label)
    lines.push(n.data.kind === 'condition' ? `  ${id}{"${l}"}` : n.data.kind === 'trigger' || n.data.kind === 'end' ? `  ${id}(["${l}"])` : `  ${id}["${l}"]`)
  }
  for (const e of edges.value) {
    const lbl = e.label ? `|${esc(String(e.label))}|` : ''
    lines.push(`  ${idMap.get(e.source)} -->${lbl} ${idMap.get(e.target)}`)
  }
  try { await navigator.clipboard.writeText(lines.join('\n')); say(t('wMermaidCopied')) }
  catch { say(t('wErrCopy')) }
}

// ── Generate files into the project ─────────────────────────────────────────
async function writeProjectFile(rel: string, content: string) {
  const full = `${root.value}/${rel}`
  await invoke('create_dir_cmd', { path: full.split('/').slice(0, -1).join('/') })
  await invoke('save_file', { path: full, content })
  store.openFile(full)
  refreshSaved()
}

async function generateSeed() {
  const { sql, tables } = buildSql(nodes.value, dialect.value as Dialect, seedRows.value, safeName())
  if (!tables) { say(t('wAddDb')); return }
  if (!(await ensureProject())) return
  try { await writeProjectFile(`database/${safeName()}.sql`, sql); say(t('wSqlDone', { n: tables, file: safeName() })) }
  catch (e) { say(`Error: ${e}`) }
}

async function generateCode() {
  if (!nodes.value.length) { say(t('wFlowEmpty')); return }
  if (!(await ensureProject())) return
  const lang = CODE_LANGS.find(l => l.key === codeLang.value)!
  try { await writeProjectFile(`flows/${safeName()}.${lang.ext}`, buildCode(nodes.value, edges.value, lang.key as CodeLang, safeName())); say(t('wCodeDone', { lang: lang.label, file: safeName(), ext: lang.ext })) }
  catch (e) { say(`Error: ${e}`) }
}

// ── Run the flow ────────────────────────────────────────────────────────────
function log(level: 'info' | 'ok' | 'err', text: string) { runLog.value.push({ level, text }) }
const clip = (s: string, n = 1500) => (s.length > n ? s.slice(0, n) + '…' : s)
const indent = (s: string) => s.split('\n').map(l => '   ' + l).join('\n')

function dbUrl(c: DbConnection): string {
  if (c.driver === 'sqlite') return `sqlite://${c.filepath}?mode=rwc`
  return `${c.driver === 'postgres' ? 'postgres' : 'mysql'}://${c.user}:${encodeURIComponent(c.password)}@${c.host}:${c.port}/${c.database}`
}
// Uses the saved connections of the Database panel; connects on demand.
async function ensureDb(connId: string): Promise<DbConnection> {
  const c = dbConns.value.find(x => x.id === connId)
  if (!c) throw new Error(t('wPickConnBlock'))
  try { await invoke('db_list_tables', { id: c.id }) }
  catch { await invoke('db_connect', { id: c.id, driver: c.driver, url: dbUrl(c) }) }
  return c
}

async function runSeedOnConnection() {
  const { sql, tables } = buildSql(nodes.value, dialect.value as Dialect, seedRows.value, safeName())
  if (!tables) { say(t('wAddDbCols')); return }
  const c = dbConns.value.find(x => x.id === flowState.dbConnId)
  if (!c) { say(t('wPickConn')); return }
  const ok = await ask(t('wAskSeed', { n: tables, rows: seedRows.value, conn: c.name, driver: c.driver }),
    { title: 'Sembrar datos', kind: 'warning', okLabel: 'Crear y sembrar', cancelLabel: 'Cancelar' })
  if (!ok) return
  try {
    await ensureDb(c.id)
    const n = await invoke<number>('db_execute', { id: c.id, sql })
    say(t('wSeedOk', { n: tables, conn: c.name, rows: n }))
  } catch (e) { say(t('wSeedErr', { e: String(e) })) }
}

let currentDetail: any = {}
async function runStep(n: any, ctx: Ctx): Promise<{ ctx: Ctx; yes?: boolean }> {
  const d = n.data
  const detail: any = (currentDetail = {})
  if (d.kind === 'condition') {
    const yes = evalCondition(String(d.cond ?? ''), ctx)
    log('info', `   ${d.cond || t('wNoCond')} → ${yes ? t('wYes') : t('wNo')}`)
    detail.expression = d.cond; detail.result = yes
    return { ctx, yes }
  }
  if (d.kind === 'http') {
    const h = d.http ?? {}
    const url = fill(h.url, ctx)
    if (!url) throw new Error(t('wNoUrl'))
    let headers: Record<string, string> = {}
    try { headers = h.headers?.trim() ? JSON.parse(fill(h.headers, ctx)) : {} } catch { throw new Error(t('wBadHeaders')) }
    const reqBody = h.body ? fill(h.body, ctx) : null
    detail.request = { method: h.method || 'GET', url, headers, body: reqBody }
    const res = await invoke<{ status: number; status_text: string; headers: Record<string, string>; body: string; time_ms: number }>('http_request', { method: h.method || 'GET', url, headers, body: reqBody })
    let body: any = res.body
    try { body = JSON.parse(res.body) } catch { /* plain text */ }
    detail.response = { status: res.status, statusText: res.status_text, timeMs: res.time_ms, headers: res.headers, body }
    log(res.status < 400 ? 'ok' : 'err', `   ${h.method || 'GET'} ${url} → ${res.status} (${res.time_ms} ms)`)
    if (res.status >= 400) throw new Error(`HTTP ${res.status}`)
    return { ctx: { ...ctx, http: { status: res.status, body } } }
  }
  if (d.kind === 'db' && String(d.sql ?? '').trim()) {
    const c = await ensureDb(d.dbConn || flowState.dbConnId)
    const sql = fillSql(d.sql, ctx, c.driver === 'mysql')
    if (isReadQuery(sql)) {
      const r = await invoke<{ columns: string[]; rows: (string | null)[][] }>('db_query', { id: c.id, sql })
      const rows = r.rows.map(row => Object.fromEntries(r.columns.map((col, i) => [col, row[i]])))
      log('ok', t('wRowsFrom', { n: rows.length, conn: c.name }))
      detail.sql = sql; detail.rows = rows
      return { ctx: { ...ctx, rows, row_count: rows.length, row: rows[0] ?? null } }
    }
    const affected = await invoke<number>('db_execute', { id: c.id, sql })
    log('ok', t('wRowsAff', { n: affected, conn: c.name }))
    detail.sql = sql; detail.affected = affected
    return { ctx: { ...ctx, affected } }
  }
  const cmd = String(d.run?.command ?? '').trim()
  if (RUNNABLE.includes(d.kind) && cmd) {
    const r = await invoke<{ stdout: string; stderr: string; exit_code: number; timed_out: boolean }>('flow_run_command', {
      root: root.value, cwd: d.run?.cwd || null, command: fill(cmd, ctx), input: JSON.stringify(ctx),
    })
    detail.command = fill(cmd, ctx); detail.stdout = r.stdout; detail.stderr = r.stderr; detail.exitCode = r.exit_code
    if (r.stdout.trim()) log('info', indent(clip(r.stdout.trim())))
    if (r.stderr.trim()) log(r.exit_code === 0 ? 'info' : 'err', indent(clip(r.stderr.trim())))
    if (r.exit_code !== 0) throw new Error(r.timed_out ? t('wTimeout') : t('wExitCode', { code: r.exit_code }))
    return { ctx: mergeOutput(ctx, r.stdout) }
  }
  return { ctx }   // design-only block: passes the data through
}

// Steps that really act on something outside the editor (so the user confirms them first)
function describeStep(n: any): string | null {
  const d = n.data
  if (d.kind === 'db' && String(d.sql ?? '').trim()) return t('wDescSql', { label: d.label, conn: dbConns.value.find(c => c.id === (d.dbConn || flowState.dbConnId))?.name ?? t('wNoConn') })
  if (d.kind === 'http' && d.http?.url && !/^(GET|HEAD)$/i.test(d.http.method || 'GET')) return `${d.label}: ${d.http.method} ${d.http.url}`
  if (RUNNABLE.includes(d.kind) && String(d.run?.command ?? '').trim()) return `${d.label}: ${d.run.command}`
  return null
}

// Run one block alone, with the data the previous block produced in the last run (or the input JSON)
async function runSingle(n: any) {
  if (running.value) return
  let input: Ctx = {}
  try { input = runInput.value.trim() ? JSON.parse(runInput.value) : {} } catch { say(t('wBadInput')); showLog.value = true; return }
  const upstream = edges.value.map(e => (e.target === n.id ? nodeResults.value[e.source] : null)).find(Boolean)
  const base: Ctx = upstream?.after ?? input
  const act = describeStep(n)
  if (act) {
    if (!n.data.http && !(await ensureProject())) return
    const ok = await ask(t('wAskBlock', { act, src: upstream ? t('wSrcUp') : t('wSrcIn') }), { title: t('wTestTitle'), kind: 'warning', okLabel: t('wExec'), cancelLabel: t('wCancel') })
    if (!ok) return
  }
  running.value = true; showLog.value = true
  nodeStatus.value = { ...nodeStatus.value, [n.id]: 'running' }
  log('info', `▶ Probando "${n.data.label}"…`)
  const t1 = Date.now()
  try {
    const r = await runStep(n, JSON.parse(JSON.stringify(base)))
    nodeStatus.value = { ...nodeStatus.value, [n.id]: 'ok' }
    nodeResults.value = { ...nodeResults.value, [n.id]: { ok: true, ms: Date.now() - t1, detail: currentDetail, after: JSON.parse(JSON.stringify(r.ctx)) } }
    log('ok', `✔ ${n.data.label} (${Date.now() - t1} ms)`)
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    nodeStatus.value = { ...nodeStatus.value, [n.id]: 'err' }
    nodeResults.value = { ...nodeResults.value, [n.id]: { ok: false, ms: Date.now() - t1, error: msg, detail: currentDetail, after: base } }
    log('err', `✖ ${n.data.label}: ${msg}`)
  } finally { running.value = false }
}

// What to show in the result panel of the selected block
const pretty = (v: unknown) => (typeof v === 'string' ? v : JSON.stringify(v, null, 2))
const result = computed(() => (selected.value ? nodeResults.value[selected.value.id] : null))
const resultTabs = computed(() => {
  const r = result.value; if (!r) return []
  const d = r.detail ?? {}
  const tabs: { key: 'response' | 'data' | 'request' | 'output'; label: string }[] = []
  if (d.response || d.rows || d.affected !== undefined || d.result !== undefined) tabs.push({ key: 'response', label: d.response ? t('wTabResp') : d.rows ? t('wTabRows') : t('wTabResult') })
  if (d.request || d.sql || d.command || d.expression) tabs.push({ key: 'request', label: d.request ? t('wTabReq') : d.sql ? 'SQL' : d.command ? t('wTabCmd') : t('wTabCond') })
  if (d.stdout !== undefined || d.stderr || r.error) tabs.push({ key: 'output', label: t('wTabOut') })
  tabs.push({ key: 'data', label: t('wTabData') })
  return tabs
})
const resultText = computed(() => {
  const r = result.value; if (!r) return ''
  const d = r.detail ?? {}
  const tab = resultTabs.value.some(rt => rt.key === resultTab.value) ? resultTab.value : resultTabs.value[0]?.key
  if (tab === 'response') return d.response ? `${d.response.status} ${d.response.statusText ?? ''} · ${d.response.timeMs} ms\n\n${pretty(d.response.body)}` : d.rows ? pretty(d.rows) : d.affected !== undefined ? `${d.affected} fila(s) afectadas` : String(d.result)
  if (tab === 'request') return d.request ? `${d.request.method} ${d.request.url}\n${Object.entries(d.request.headers ?? {}).map(([k, v]) => `${k}: ${v}`).join('\n')}${d.request.body ? `\n\n${d.request.body}` : ''}` : d.sql ?? d.command ?? d.expression ?? ''
  if (tab === 'output') return [r.error ? `✖ ${r.error}` : '', d.stdout ? `— stdout —\n${d.stdout}` : '', d.stderr ? `— stderr —\n${d.stderr}` : '', d.exitCode !== undefined ? t('wExitLbl', { code: d.exitCode }) : ''].filter(Boolean).join('\n\n')
  return pretty(r.after)
})
const activeTab = computed(() => (resultTabs.value.some(rt => rt.key === resultTab.value) ? resultTab.value : resultTabs.value[0]?.key))
const headersText = computed(() => { const h = result.value?.detail?.response?.headers; return h ? Object.entries(h).map(([k, v]) => `${k}: ${v}`).join('\n') : '' })
async function copyResult() { try { await navigator.clipboard.writeText(resultText.value); say(t('wCopied')) } catch { say(t('wErrCopy')) } }

async function runFlow() {
  if (running.value) { cancelRun = true; return }
  if (!nodes.value.length) { say(t('wFlowEmpty')); return }
  let input: Ctx = {}
  try { input = runInput.value.trim() ? JSON.parse(runInput.value) : {} } catch { say(t('wBadInput')); showLog.value = true; return }
  const acts = nodes.value.map(describeStep).filter(Boolean) as string[]
  if (acts.length) {
    if (nodes.value.some(n => RUNNABLE.includes(n.data.kind) && String(n.data.run?.command ?? '').trim()) && !(await ensureProject())) return
    const ok = await ask(t('wAskFlow', { n: acts.length, list: acts.map(x => '• ' + x).join('\n') }),
      { title: t('wAskFlowTitle'), kind: 'warning', okLabel: t('wExec'), cancelLabel: t('wCancel') })
    if (!ok) return
  }

  running.value = true; cancelRun = false; showLog.value = true
  runLog.value = []; nodeStatus.value = {}; nodeResults.value = {}
  const byId = new Map(nodes.value.map(n => [n.id, n]))
  let ctx: Ctx = input
  let steps = 0
  const t0 = Date.now()
  log('info', t('wRunning', { name: safeName() }))

  const visit = async (id: string): Promise<void> => {
    const n = byId.get(id)
    if (!n || n.data.kind === 'note') return
    if (cancelRun) throw new Error(t('wStopped'))
    if (++steps > 200) throw new Error(t('wLoop'))
    nodeStatus.value = { ...nodeStatus.value, [id]: 'running' }
    log('info', `${kindInfo(n.data.kind).icon} ${n.data.label}`)
    let result: { ctx: Ctx; yes?: boolean }
    const t1 = Date.now()
    try { result = await runStep(n, ctx) }
    catch (e) {
      nodeStatus.value = { ...nodeStatus.value, [id]: 'err' }
      nodeResults.value = { ...nodeResults.value, [id]: { ok: false, ms: Date.now() - t1, error: e instanceof Error ? e.message : String(e), detail: currentDetail, after: ctx } }
      throw new Error(`${n.data.label}: ${e instanceof Error ? e.message : e}`)
    }
    ctx = result.ctx
    nodeResults.value = { ...nodeResults.value, [id]: { ok: true, ms: Date.now() - t1, detail: currentDetail, after: JSON.parse(JSON.stringify(ctx)) } }
    nodeStatus.value = { ...nodeStatus.value, [id]: 'ok' }
    if (n.data.kind === 'end') return
    let outs = edges.value.filter(e => e.source === id)
    if (n.data.kind === 'condition') outs = outs.filter(e => (e.sourceHandle === 'no') === !result.yes)
    for (const e of outs) await visit(e.target)
  }

  try {
    const starts = nodes.value.filter(n => n.data.kind === 'trigger')
    const first = starts.length ? starts : nodes.value.filter(n => n.data.kind !== 'note' && !edges.value.some(e => e.target === n.id))
    for (const s of first) await visit(s.id)
    log('ok', t('wFinished', { s: ((Date.now() - t0) / 1000).toFixed(1) }))
    log('info', t('wFinalData', { d: clip(JSON.stringify(ctx), 800) }))
  } catch (e) { log('err', `✖ ${e instanceof Error ? e.message : e}`) }
  finally { running.value = false; cancelRun = false }
}

// ── AI assistant: describe the business, get a flow ─────────────────────────
const aiLoading = ref(false)
const AI_SYSTEM = `Eres un analista de negocio. El usuario describe un proceso. Responde SOLO con un bloque JSON (sin explicación) con esta forma:
{"nodes":[{"id":"a","kind":"trigger|function|http|db|condition|email|end","label":"short English name, ASCII letters only, no accents (it becomes a table or function name)","desc":"qué hace y reglas de negocio","columns":"solo para kind db: una columna por línea, ej:\\nnombre text\\nemail text unique\\ncliente_id int","cond":"solo condition: ej. total > 100"}],"edges":[{"from":"a","to":"b","label":"Sí|No (solo si sale de una condition)"}]}
Reglas: empieza con un trigger, termina con end, máximo 14 nodos, nombres (label) en inglés, solo letras ASCII; desc y las demás palabras en el idioma del usuario.`

async function generateWithAi() {
  const prompt = aiText.value.trim()
  if (!prompt || aiLoading.value) return
  const provider = localStorage.getItem('ai_provider') || 'claude'
  const apiKey = provider === 'ollama' ? 'ollama' : loadProviderKey(provider as Provider)
  if (!apiKey) { say(t('wNeedKey')); return }
  const model = localStorage.getItem('ai_model') ?? 'claude-sonnet-4-6'
  aiLoading.value = true
  try {
    const reply = await invoke<string>('call_ai', { provider, apiKey, model, system: AI_SYSTEM, messages: [{ role: 'user', content: prompt }] })
    const m = reply.match(/```(?:json)?\s*([\s\S]*?)```/)
    const data = JSON.parse((m ? m[1] : reply).trim())
    const list: any[] = data.nodes ?? []
    const links: any[] = data.edges ?? []
    const depth = new Map<string, number>(); list.forEach(n => depth.set(n.id, 0))
    for (let pass = 0; pass < list.length; pass++) for (const e of links) depth.set(e.to, Math.max(depth.get(e.to) ?? 0, (depth.get(e.from) ?? 0) + 1))
    const rows: Record<number, number> = {}
    nodes.value = list.map(n => {
      const d = depth.get(n.id) ?? 0; const row = rows[d] = (rows[d] ?? -1) + 1
      const kind = FLOW_KINDS.some(k => k.key === n.kind) ? n.kind : 'function'
      return { id: String(n.id), type: 'block', position: { x: 60 + d * 280, y: 60 + row * 130 }, data: { kind, label: n.label ?? extraEn.wStep, desc: n.desc ?? '', columns: n.columns ?? '', cond: n.cond ?? '',
        run: { lang: 'node', file: '', command: '', cwd: '' }, http: { method: 'GET', url: '', headers: '', body: '' } } }
    })
    edges.value = links.map((e, i) => ({ id: `ai${i}`, source: String(e.from), target: String(e.to), animated: true, label: e.label || undefined, sourceHandle: /^(s[ií]|yes)$/i.test(String(e.label ?? '')) ? 'yes' : /^no$/i.test(String(e.label ?? '')) ? 'no' : undefined }))
    selectedId.value = null; nodeStatus.value = {}
    setTimeout(() => fitView({ padding: 0.2 }), 80)
    say(t('wAiDone'))
  } catch (e) { say(t('wAiBad', { e: String(e) })) }
  finally { aiLoading.value = false }
}


// ── Camera: remember zoom/position per mode ─────────────────────────────────
const viewKey = () => (mode.value === 'map' ? (flowState.isolated ? 'iso' : 'map') : 'flow')
function saveViewport() { flowState.viewports[viewKey()] = { ...getViewport() } }
function restoreViewport(fallbackFit = true, delay = 60) {
  setTimeout(() => {
    const v = flowState.viewports[viewKey()]
    if (v) setViewport(v)
    else if (fallbackFit) fitView({ padding: 0.15 })
  }, delay)
}

// ── Search + relations ──────────────────────────────────────────────────────
const search = ref('')
const searchOpen = ref(false)
interface PoolItem { id: string; name: string; sub: string }
const pool = computed<PoolItem[]>(() => {
  if (mode.value === 'map' && flowState.graph) return (flowState.graph as ProjectGraph).nodes.map(n => ({ id: n.id, name: n.name, sub: n.id }))
  return (nodes.value as any[]).map(n => ({ id: n.id, name: String(n.data.label), sub: String(n.data.desc ?? '') }))
})
const searchResults = computed(() => {
  const q = search.value.trim().toLowerCase()
  if (!q) return []
  return pool.value.filter(i => i.name.toLowerCase().includes(q) || i.sub.toLowerCase().includes(q)).slice(0, 14)
})

function focusNode(id: string) {
  searchOpen.value = false
  const g = flowState.graph as ProjectGraph | null
  if (mode.value === 'map' && g) {
    const kind = g.nodes.find(n => n.id === id)?.kind
    if (kind && flowState.hiddenKinds.includes(kind)) { flowState.hiddenKinds = flowState.hiddenKinds.filter(k => k !== kind); if (!flowState.isolated) renderMap(false) }
    if (flowState.isolated && !(nodes.value as any[]).some(n => n.id === id)) { flowState.isolated = null; renderMap(false) }
  }
  selectedId.value = id
  setTimeout(() => {
    const n = (nodes.value as any[]).find(x => x.id === id)
    if (n) setCenter(n.position.x + 80, n.position.y + 30, { zoom: 1.1, duration: 450 })
  }, 60)
}

// Direct relations of a file (map mode)
const relations = computed(() => {
  const g = flowState.graph as ProjectGraph | null
  const id = selectedId.value
  if (mode.value !== 'map' || !g || !id) return { imports: [] as string[], usedBy: [] as string[] }
  return { imports: g.edges.filter(e => e.from === id).map(e => e.to), usedBy: g.edges.filter(e => e.to === id).map(e => e.from) }
})

// Dim everything that is not directly related to the selected file
function applyHighlight() {
  const id = selectedId.value
  const rel = relations.value
  const keep = new Set<string>(id ? [id, ...rel.imports, ...rel.usedBy] : [])
  const on = mode.value === 'map' && !!id && !flowState.isolated
  for (const n of nodes.value as any[]) n.class = on && !keep.has(n.id) ? 'dim' : ''
  for (const e of edges.value as any[]) {
    const hit = on && (e.source === id || e.target === id)
    e.style = on ? (hit ? { stroke: e.source === id ? '#3fb950' : '#e5a50a', strokeWidth: 2.2 } : { opacity: 0.06 }) : undefined
    e.animated = hit
  }
}
watch([selectedId, mode], applyHighlight)

function isolate(id: string) {
  saveViewport()
  flowState.isolated = id
  renderMap(false)
}
function leaveIsolation() {
  flowState.isolated = null
  renderMap(false)
  restoreViewport(true, 80)
}

function renderIsolated(g: ProjectGraph, id: string) {
  const byId = new Map(g.nodes.map(n => [n.id, n]))
  const levels = (dir: 'in' | 'out'): string[][] => {
    const seen = new Set<string>([id]); let frontier = [id]; const out: string[][] = []
    for (let d = 0; d < flowState.isoDepth; d++) {
      const next: string[] = []
      for (const f of frontier) for (const e of g.edges) {
        const [from, to] = dir === 'out' ? [e.from, e.to] : [e.to, e.from]
        if (from === f && !seen.has(to)) { seen.add(to); next.push(to) }
      }
      if (!next.length) break
      out.push(next); frontier = next
    }
    return out
  }
  const placed = new Map<string, { x: number; y: number }>([[id, { x: 0, y: 0 }]])
  const lay = (cols: string[][], sign: number) => cols.forEach((ids, ci) => ids.forEach((nid, i) => {
    if (!placed.has(nid)) placed.set(nid, { x: sign * 320 * (ci + 1), y: (i - (ids.length - 1) / 2) * 84 })
  }))
  lay(levels('in'), -1); lay(levels('out'), 1)
  nodes.value = [...placed.entries()].map(([nid, pos]) => {
    const n = byId.get(nid)!
    return { id: nid, type: 'block', position: pos, class: nid === id ? 'center' : '', data: { kind: n.kind, label: n.name, desc: n.id, file: n.id, lines: n.lines, lang: n.lang } }
  })
  edges.value = g.edges.filter(e => placed.has(e.from) && placed.has(e.to)).map((e, i) => ({ id: `i${i}`, source: e.from, target: e.to, animated: e.from === id || e.to === id }))
  selectedId.value = id
  setTimeout(() => fitView({ padding: 0.2, duration: 300 }), 80)
}

// ── Project map ─────────────────────────────────────────────────────────────
interface ProjectGraph { framework: string; nodes: { id: string; name: string; kind: string; lines: number; lang: string }[]; edges: { from: string; to: string }[]; truncated: boolean }

async function loadMap() {
  if (!root.value) { say(t('wOpenFirst')); return }
  say(t('wAnalyzing'))
  try {
    const g = await invoke<ProjectGraph>('scan_project_graph', { root: root.value })
    flowState.graph = g
    framework.value = g.framework
    mapNotice.value = g.truncated ? t('wBigProject') : ''
    renderMap(true)
    say(`${g.nodes.length} archivos, ${g.edges.length} dependencias`)
  } catch (e) { say(`Error: ${e}`) }
}

function renderMap(fit = true) {
  const g = flowState.graph as ProjectGraph | null
  if (!g) return
  if (flowState.isolated && g.nodes.some(n => n.id === flowState.isolated)) return renderIsolated(g, flowState.isolated)
  const order = MAP_KINDS.map(k => k.key)
  const visible = g.nodes.filter(n => !flowState.hiddenKinds.includes(n.kind))
  const rows: Record<string, number> = {}
  nodes.value = visible.map(n => {
    const col = Math.max(0, order.indexOf(n.kind))
    const row = rows[n.kind] = (rows[n.kind] ?? -1) + 1
    return { id: n.id, type: 'block', position: { x: col * 280, y: row * 84 }, data: { kind: n.kind, label: n.name, desc: n.id, file: n.id, lines: n.lines, lang: n.lang } }
  })
  const ids = new Set(visible.map(n => n.id))
  edges.value = g.edges.filter(e => ids.has(e.from) && ids.has(e.to)).map((e, i) => ({ id: `m${i}`, source: e.from, target: e.to }))
  if (fit) restoreViewport(true, 80)
  applyHighlight()
}

function toggleKind(k: string) {
  const s = new Set(flowState.hiddenKinds)
  s.has(k) ? s.delete(k) : s.add(k)
  flowState.hiddenKinds = [...s]
  renderMap(false)
}

function setMode(m: 'flow' | 'map') {
  if (m === mode.value) return
  saveViewport()
  if (mode.value === 'flow') flowState.flowBackup = { nodes: nodes.value, edges: edges.value }
  mode.value = m
  selectedId.value = null
  if (m === 'flow') { nodes.value = flowState.flowBackup.nodes; edges.value = flowState.flowBackup.edges; restoreViewport(true, 60) }
  else if (flowState.graph) { nodes.value = []; edges.value = []; renderMap(true) }   // reuse the last analysis: no re-scan, same camera
  else { nodes.value = []; edges.value = []; loadMap() }
}

// Files the visual designers can open: Quasar/Vue pages and HTML, and Qt/GTK .ui files
const designable = computed(() => /\.(vue|html?|ui|qml)$/i.test(String(selected.value?.data.file ?? '')))
function openInDesign() {
  const f = selected.value?.data.file
  if (!f) return
  designRequest.path = `${root.value}/${f}`
  store.openDesignTab()
}

function openSelectedFile() {
  const f = selected.value?.data.file
  if (f) store.openFile(`${root.value}/${f}`)
}

// Coming back to this tab: the state is still there, just re-fit the view.
onMounted(() => { refreshSaved(); if (nodes.value.length) restoreViewport(true, 150) })
</script>

<template>
  <div class="flow-view">
    <div class="flow-toolbar">
      <div class="flow-seg">
        <button :class="{ on: mode === 'flow' }" @click="setMode('flow')">{{ t('wFlowTab') }}</button>
        <button :class="{ on: mode === 'map' }" @click="setMode('map')">{{ t('wMapTab') }}</button>
      </div>

      <div class="flow-search">
        <input v-model="search" class="flow-input" :placeholder="mode === 'map' ? t('wSearchFile') : t('wSearchBlock')" @focus="searchOpen = true" @input="searchOpen = true" @keydown.enter="searchResults[0] && focusNode(searchResults[0].id)" @keydown.esc="searchOpen = false" />
        <div v-if="searchOpen && searchResults.length" class="flow-search-pop">
          <button v-for="r in searchResults" :key="r.id" @mousedown.prevent="focusNode(r.id)">
            <b>{{ r.name }}</b><small>{{ r.sub }}</small>
          </button>
        </div>
      </div>

      <template v-if="mode === 'flow'">
        <button v-for="k in FLOW_KINDS" :key="k.key" class="flow-btn" :style="{ borderColor: k.color }" :title="t('wAddTip', { label: kl(k) })" @click="addBlock(k.key)">
          <span :style="{ color: k.color }">{{ k.icon }}</span> {{ kl(k) }}
        </button>
        <div class="flow-spacer" />
        <button class="flow-btn run" :class="{ stop: running }" @click="runFlow">{{ running ? t('wStop') : t('wRun') }}</button>
        <input v-model="flowName" class="flow-input" :placeholder="t('wFlowName')" />
        <button class="flow-btn accent" @click="saveFlow">{{ t('wSave') }}</button>
        <select class="flow-input" @change="loadFlow(($event.target as HTMLSelectElement).value); ($event.target as HTMLSelectElement).value = ''" @focus="refreshSaved">
          <option value="">{{ t('wOpenDots') }}</option>
          <option v-for="f in savedFlows" :key="f" :value="f">{{ f }}</option>
        </select>
        <button class="flow-btn" @click="newFlow">{{ t('wNew') }}</button>
        <button class="flow-btn" @click="copyMermaid">Mermaid</button>
        <button class="flow-btn accent" @click="showAi = !showAi">✨ IA</button>
      </template>
      <template v-else>
        <span class="flow-fw" v-if="framework">{{ framework }}</span>
        <button v-for="k in MAP_KINDS" :key="k.key" class="flow-btn chip" :class="{ off: flowState.hiddenKinds.includes(k.key) }" :style="{ borderColor: k.color }" @click="toggleKind(k.key)">
          <span :style="{ color: k.color }">{{ k.icon }}</span> {{ kl(k) }}
        </button>
        <div class="flow-spacer" />
        <template v-if="flowState.isolated">

          <select v-model.number="flowState.isoDepth" class="flow-input" :title="t('wDepth')" @change="renderMap(true)"><option :value="1">{{ t('wLevel1') }}</option><option :value="2">{{ t('wLevel2') }}</option><option :value="3">{{ t('wLevel3') }}</option></select>
          <button class="flow-btn accent" @click="leaveIsolation">{{ t('wFullMap') }}</button>
        </template>
        <button class="flow-btn" @click="loadMap">{{ t('wRescan') }}</button>
      </template>
      <span v-if="status" class="flow-status">{{ status }}</span>
    </div>

    <div v-if="mode === 'flow' && showAi" class="flow-ai">
      <input v-model="aiText" class="flow-input" style="flex:1" :placeholder="t('wAiPh')" @keydown.enter="generateWithAi" />
      <button class="flow-btn accent" :disabled="aiLoading" @click="generateWithAi">{{ aiLoading ? t('wGenerating') : t('wGenerate') }}</button>
    </div>
    <div v-if="mode === 'flow'" class="flow-gen">
      <span>{{ t('wGenerateLbl') }}</span>
      <select v-model="dialect" class="flow-input"><option value="sqlite">SQLite</option><option value="mysql">MySQL / MariaDB</option><option value="postgres">PostgreSQL</option></select>
      <input v-model.number="seedRows" type="number" min="1" max="500" class="flow-input" style="width:60px" :title="t('wSeedRowsTip')" />
      <button class="flow-btn" @click="generateSeed">{{ t('wSeedBtn') }}</button>
      <select v-model="flowState.dbConnId" class="flow-input" :title="t('wDbConnTip')">
        <option value="">{{ t('wDbConn') }}</option>
        <option v-for="c in dbConns" :key="c.id" :value="c.id">{{ c.name }}</option>
      </select>
      <button class="flow-btn" :disabled="!flowState.dbConnId" @click="runSeedOnConnection" :title="t('wSeedRunTip')">{{ t('wSeedRun') }}</button>
      <select v-model="codeLang" class="flow-input" :title="t('wCodeLangTip')"><option v-for="l in CODE_LANGS" :key="l.key" :value="l.key">{{ l.label }}</option></select>
      <button class="flow-btn" @click="generateCode">{{ t('wCodeBtn') }}</button>
      <button class="flow-btn" @click="showLog = !showLog">{{ showLog ? t('wHideRun') : t('wShowRun') }}</button>
      <span class="flow-gen-hint">{{ root ? t('wProject', { root }) : t('wNoProject') }}</span>
    </div>
    <div v-if="mapNotice && mode === 'map'" class="flow-notice">{{ mapNotice }}</div>

    <div class="flow-body">
      <VueFlow v-model:nodes="nodes" v-model:edges="edges" :delete-key-code="['Delete', 'Backspace']" :min-zoom="0.05" @move-end="saveViewport" @connect="onConnect" @node-click="(e: any) => selectedId = e.node.id" @pane-click="selectedId = null">
        <Background />
        <Controls />
        <MiniMap pannable zoomable class="flow-minimap" node-color="#64748b" mask-color="rgba(0,0,0,0.55)" />
        <template #node-block="p">
          <div class="blk" :class="[{ sel: p.id === selectedId, note: p.data.kind === 'note' }, nodeStatus[p.id] ? `st-${nodeStatus[p.id]}` : '']" :style="{ borderColor: kindInfo(p.data.kind).color }">
            <Handle type="target" :position="Position.Left" />
            <div class="blk-head" :style="{ background: kindInfo(p.data.kind).color + '33' }">
              <span :style="{ color: kindInfo(p.data.kind).color }">{{ kindInfo(p.data.kind).icon }}</span>
              <span class="blk-title">{{ p.data.label }}</span>
              <span v-if="nodeStatus[p.id] === 'running'" class="blk-badge">⏳</span>
              <span v-else-if="nodeStatus[p.id] === 'ok'" class="blk-badge ok">✔</span>
              <span v-else-if="nodeStatus[p.id] === 'err'" class="blk-badge err">✖</span>
              <span v-else-if="mode === 'flow' && (p.data.run?.command || (p.data.kind === 'http' && p.data.http?.url))" class="blk-badge" :title="t('wConnectedTip')">⚙</span>
            </div>
            <div v-if="p.data.desc" class="blk-desc">{{ p.data.desc }}</div>
            <template v-if="p.data.kind === 'condition'">
              <Handle id="yes" type="source" :position="Position.Right" :style="{ top: '30%' }" />
              <Handle id="no" type="source" :position="Position.Right" :style="{ top: '75%' }" />
            </template>
            <Handle v-else-if="p.data.kind !== 'end'" type="source" :position="Position.Right" />
          </div>
        </template>
      </VueFlow>

      <aside v-if="selected" class="flow-side">
        <div class="flow-side-title">{{ t('wBlock') }}</div>
        <label>{{ t('wName') }}</label>
        <input v-model="selected.data.label" class="flow-input wide" :readonly="mode === 'map'" />
        <template v-if="mode === 'flow'">
          <label>{{ t('wType') }}</label>
          <select v-model="selected.data.kind" class="flow-input wide">
            <option v-for="k in FLOW_KINDS" :key="k.key" :value="k.key">{{ kl(k) }}</option>
          </select>
          <label>{{ t('wDesc') }}</label>
          <textarea v-model="selected.data.desc" class="flow-input wide" rows="3" :placeholder="t('wDescPh')" />

          <template v-if="selected.data.kind === 'db'">
            <label>{{ t('wColumns') }}</label>
            <textarea v-model="selected.data.columns" class="flow-input wide" rows="5" placeholder="nombre text&#10;email text unique&#10;precio decimal&#10;cliente_id int&#10;activo bool" />
            <div class="flow-meta" v-html="t('wColumnsMeta')" />
            <label>{{ t('wRunSql') }}</label>
            <select v-model="selected.data.dbConn" class="flow-input wide">
              <option value="">{{ t('wGeneralConn') }}</option>
              <option v-for="c in dbConns" :key="c.id" :value="c.id">{{ c.name }} · {{ c.driver }}</option>
            </select>
            <textarea v-model="selected.data.sql" class="flow-input wide" rows="4" placeholder="SELECT * FROM pedidos WHERE cliente_id = {{cliente_id}}" />
            <div class="flow-meta" v-html="t('wSqlMeta')" />
          </template>

          <template v-if="selected.data.kind === 'condition'">
            <label>{{ t('wCond') }}</label>
            <input v-model="selected.data.cond" class="flow-input wide" placeholder="total > 100 && estado == &quot;ok&quot;" />
            <div class="flow-meta">{{ t('wCondMeta') }}</div>
          </template>

          <template v-if="selected.data.kind === 'http'">
            <label>{{ t('wRequest') }}</label>
            <div class="flow-row">
              <select v-model="selected.data.http.method" class="flow-input" style="width:90px"><option>GET</option><option>POST</option><option>PUT</option><option>PATCH</option><option>DELETE</option></select>
              <input v-model="selected.data.http.url" class="flow-input" style="flex:1" placeholder="https://api.com/pedidos/{{id}}" />
            </div>
            <label>Headers (JSON)</label>
            <input v-model="selected.data.http.headers" class="flow-input wide" placeholder='{"Authorization":"Bearer {{token}}"}' />
            <label>Body</label>
            <textarea v-model="selected.data.http.body" class="flow-input wide" rows="3" placeholder='{"cliente":"{{cliente}}"}' />
            <div class="flow-meta" v-html="t('wHttpMeta')" />
          </template>

          <template v-if="['function', 'db', 'email', 'trigger'].includes(selected.data.kind)">
            <label>{{ t('wConnect') }}</label>
            <div class="flow-row">
              <select v-model="selected.data.run.lang" class="flow-input" @change="updateCommand(selected)">
                <option v-for="l in RUN_LANGS" :key="l.key" :value="l.key">{{ l.label }}</option>
              </select>
              <input v-model="selected.data.run.file" class="flow-input" style="flex:1" placeholder="scripts/cobrar.py" @input="updateCommand(selected)" />
            </div>
            <label>{{ t('wCommand') }}</label>
            <input v-model="selected.data.run.command" class="flow-input wide" placeholder="python3 scripts/cobrar.py" @input="selected.data.run.lang = 'custom'" />
            <label>{{ t('wFolderOpt') }}</label>
            <input v-model="selected.data.run.cwd" class="flow-input wide" placeholder="backend" />
            <div class="flow-meta" v-html="t('wRunMeta')" />
          </template>
          <div class="res">
            <div class="res-head">
              <span>{{ t('wResult') }}</span>
              <button class="flow-btn run" :disabled="running" @click="runSingle(selected)" :title="t('wTestOneTip')">{{ t('wTestOne') }}</button>
            </div>
            <template v-if="result">
              <div class="res-chips">
                <span :class="result.ok ? 'chip-ok' : 'chip-err'">{{ result.ok ? '✔ OK' : '✖ Error' }}</span>
                <span v-if="result.detail?.response" :class="result.detail.response.status < 400 ? 'chip-ok' : 'chip-err'">{{ result.detail.response.status }} {{ result.detail.response.statusText }}</span>
                <span class="chip-dim">{{ result.ms }} ms</span>
              </div>
              <div class="res-tabs"><button v-for="rt in resultTabs" :key="rt.key" :class="{ on: activeTab === rt.key }" @click="resultTab = rt.key">{{ rt.label }}</button></div>
              <pre class="res-body">{{ resultText }}</pre>
              <details v-if="headersText && activeTab === 'response'"><summary>{{ t('wRespHeaders') }}</summary><pre class="res-body">{{ headersText }}</pre></details>
              <button class="flow-btn" @click="copyResult">{{ t('wCopy') }}</button>
            </template>
            <div v-else class="flow-meta">{{ t('wNotYet') }}</div>
          </div>
          <button class="flow-btn danger" @click="removeSelected">{{ t('wRemove') }}</button>
        </template>
        <template v-else>
          <label>{{ t('wFile') }}</label>
          <code class="flow-path">{{ selected.data.file }}</code>
          <div class="flow-meta">{{ selected.data.lang }} · {{ selected.data.lines }} {{ t('wLines') }} · {{ kl(kindInfo(selected.data.kind)) }}</div>
          <div class="flow-row" style="margin-top:8px; flex-wrap:wrap">
            <button class="flow-btn accent" @click="openSelectedFile">{{ t('wOpenFile') }}</button>
            <button v-if="designable" class="flow-btn run" @click="openInDesign" :title="t('wOpenDesignTip')">{{ t('wOpenDesign') }}</button>
            <button v-if="!flowState.isolated" class="flow-btn" @click="isolate(selected.id)">{{ t('wIsolate') }}</button>
          </div>
          <label>{{ t('wUses', { n: relations.imports.length }) }}</label>
          <div class="rel-list"><button v-for="id in relations.imports" :key="id" @click="focusNode(id)" :title="id">→ {{ id.split('/').pop() }}<small>{{ id }}</small></button><span v-if="!relations.imports.length" class="rel-none">{{ t('wNone') }}</span></div>
          <label>{{ t('wUsedBy', { n: relations.usedBy.length }) }}</label>
          <div class="rel-list"><button v-for="id in relations.usedBy" :key="id" @click="focusNode(id)" :title="id">← {{ id.split('/').pop() }}<small>{{ id }}</small></button><span v-if="!relations.usedBy.length" class="rel-none">{{ t('wNobody') }}</span></div>
        </template>
      </aside>
    </div>

    <div v-if="mode === 'flow' && showLog" class="flow-log">
      <div class="flow-log-head">
        <span>{{ t('wRunHead') }}</span>
        <input v-model="runInput" class="flow-input" style="flex:1" :placeholder="t('wInputPh')" />
        <button class="flow-btn" @click="runLog = []; nodeStatus = {}">{{ t('wClear') }}</button>
      </div>
      <div class="flow-log-body">
        <div v-if="!runLog.length" class="flow-log-empty">{{ t('wLogEmpty') }}</div>
        <pre v-for="(l, i) in runLog" :key="i" :class="l.level">{{ l.text }}</pre>
      </div>
    </div>
    <div v-if="mode === 'flow' && nodes.length === 0" class="flow-empty">{{ t('wEmptyHint') }}</div>
  </div>
</template>

<style scoped>
.flow-view { display: flex; flex-direction: column; height: 100%; width: 100%; background: var(--bg-darkest, #0f1012); position: relative; }
.flow-toolbar { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 6px 10px; border-bottom: 1px solid var(--border); background: var(--bg-dark, #141416); }
.flow-seg { display: flex; border: 1px solid var(--border); border-radius: 5px; overflow: hidden; margin-right: 6px; }
.flow-seg button { background: none; border: none; color: var(--fg-muted); font-size: 12px; padding: 5px 10px; cursor: pointer; }
.flow-seg button.on { background: var(--bg-hover); color: var(--fg-bright, #fff); }
.flow-btn { background: none; border: 1px solid var(--border); color: var(--fg); font-size: 11px; padding: 4px 8px; border-radius: 4px; cursor: pointer; }
.flow-btn:hover { background: var(--bg-hover); }
.flow-btn.accent { border-color: var(--accent); color: var(--accent); }
.flow-btn.run { border-color: #3fb950; color: #3fb950; font-weight: 600; }
.flow-btn.run.stop { border-color: #f85149; color: #f85149; }
.flow-btn.danger { border-color: #f85149; color: #f85149; margin-top: 10px; }
.flow-btn.chip.off { opacity: 0.35; }
.res { order: -1; margin: 4px 0 12px; border-bottom: 1px solid var(--border); padding-bottom: 8px; }
.res summary { font-size: 11px; color: var(--fg-muted); cursor: pointer; margin: 4px 0; }
.res-head { display: flex; align-items: center; justify-content: space-between; gap: 6px; font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: .05em; color: var(--fg-muted); margin-bottom: 6px; }
.res-chips { display: flex; flex-wrap: wrap; gap: 5px; margin-bottom: 6px; } .res-chips span { font-size: 10.5px; padding: 1px 7px; border-radius: 10px; border: 1px solid var(--border); }
.chip-ok { color: #3fb950; border-color: #3fb95055 !important; } .chip-err { color: #f85149; border-color: #f8514955 !important; } .chip-dim { color: var(--fg-muted); }
.res-tabs { display: flex; flex-wrap: wrap; gap: 2px; } .res-tabs button { background: none; border: 1px solid var(--border); color: var(--fg-muted); font-size: 10.5px; padding: 3px 7px; border-radius: 4px 4px 0 0; cursor: pointer; } .res-tabs button.on { background: var(--bg-hover); color: var(--fg-bright, #fff); }
.res-body { max-height: 260px; overflow: auto; background: var(--bg-darkest); border: 1px solid var(--border); border-radius: 0 4px 4px 4px; padding: 8px; font-size: 10.5px; line-height: 1.4; white-space: pre-wrap; word-break: break-word; margin: 0 0 6px; user-select: text; }
.flow-spacer { flex: 1; }
.flow-search { position: relative; }
.flow-search .flow-input { width: 190px; }
.flow-search-pop { position: absolute; top: 100%; left: 0; z-index: 50; width: 380px; max-height: 340px; overflow: auto; background: var(--bg-dark, #141416); border: 1px solid var(--border); border-radius: 6px; box-shadow: 0 8px 24px rgba(0,0,0,0.5); }
.flow-search-pop button, .rel-list button { display: block; width: 100%; text-align: left; background: none; border: none; color: var(--fg); font-size: 12px; padding: 5px 10px; cursor: pointer; }
.flow-search-pop button:hover, .rel-list button:hover { background: var(--bg-hover); }
.flow-search-pop small, .rel-list small { display: block; font-size: 10px; color: var(--fg-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.rel-list { max-height: 170px; overflow: auto; border: 1px solid var(--border); border-radius: 4px; margin-bottom: 6px; }
.rel-list button { padding: 3px 8px; }
.rel-none { display: block; padding: 4px 8px; font-size: 11px; color: var(--fg-muted); }
.flow-body :deep(.vue-flow__node.dim) { opacity: 0.12; transition: opacity 0.15s; }
.flow-body :deep(.vue-flow__node.center .blk) { box-shadow: 0 0 0 3px var(--accent, #2e9e87); }
.flow-row { display: flex; gap: 6px; margin-bottom: 8px; }
.flow-input { background: var(--bg-darkest); border: 1px solid var(--border); color: var(--fg); font-size: 12px; padding: 4px 7px; border-radius: 4px; }
.flow-input.wide { width: 100%; box-sizing: border-box; margin-bottom: 8px; resize: vertical; font-family: inherit; }
.flow-status { font-size: 11px; color: var(--accent); }
.flow-fw { font-size: 11px; color: var(--fg-muted); padding: 0 6px; }
.flow-ai { display: flex; gap: 6px; padding: 6px 10px; border-bottom: 1px solid var(--border); }
.flow-gen { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 5px 10px; border-bottom: 1px solid var(--border); font-size: 11px; color: var(--fg-muted); }
.flow-gen-hint { margin-left: auto; font-size: 10px; opacity: 0.8; max-width: 40%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.flow-notice { font-size: 11px; color: #f0883e; padding: 4px 12px; }
.flow-body { flex: 1; display: flex; min-height: 0; }
.flow-body :deep(.vue-flow) { flex: 1; }
.flow-body :deep(.flow-minimap) { background: var(--bg-dark, #141416); border: 1px solid var(--border); border-radius: 6px; }
.flow-side { width: 300px; border-left: 1px solid var(--border); background: var(--bg-dark, #141416); padding: 12px; overflow: auto; display: flex; flex-direction: column; }
.flow-side > * { flex-shrink: 0; }
.flow-side-title { order: -2; }
.flow-side label { display: block; font-size: 10px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--fg-muted); margin: 6px 0 3px; }
.flow-side-title { font-size: 12px; font-weight: 600; color: var(--fg-bright, #fff); margin-bottom: 6px; }
.flow-path { display: block; font-size: 11px; word-break: break-all; color: var(--fg); background: var(--bg-darkest); padding: 4px 6px; border-radius: 4px; }
.flow-meta { font-size: 11px; color: var(--fg-muted); margin: 6px 0 10px; line-height: 1.45; }
.flow-meta code { background: var(--bg-darkest); padding: 0 3px; border-radius: 3px; }
.flow-empty { position: absolute; left: 50%; top: 55%; transform: translate(-50%, 0); font-size: 12px; color: var(--fg-muted); max-width: 380px; text-align: center; pointer-events: none; }
.flow-log { height: 190px; border-top: 1px solid var(--border); background: var(--bg-dark, #141416); display: flex; flex-direction: column; }
.flow-log-head { display: flex; align-items: center; gap: 8px; padding: 5px 10px; font-size: 11px; color: var(--fg-muted); border-bottom: 1px solid var(--border); }
.flow-log-body { flex: 1; overflow: auto; padding: 6px 10px; font-family: var(--font-mono, monospace); font-size: 11px; user-select: text; }
.flow-log-body pre { margin: 0; white-space: pre-wrap; color: var(--fg); }
.flow-log-body pre.ok { color: #3fb950; }
.flow-log-body pre.err { color: #f85149; }
.flow-log-empty { color: var(--fg-muted); font-family: inherit; }
.blk { min-width: 170px; max-width: 240px; background: var(--bg-dark, #1b1c20); border: 1.5px solid; border-radius: 8px; font-size: 12px; color: var(--fg); box-shadow: 0 2px 8px rgba(0,0,0,0.35); }
.blk.sel { box-shadow: 0 0 0 2px var(--accent, #2e9e87); }
.blk.note { border-style: dashed; }
.blk.st-running { box-shadow: 0 0 0 2px #e5a50a; }
.blk.st-ok { box-shadow: 0 0 0 2px #3fb950; }
.blk.st-err { box-shadow: 0 0 0 2px #f85149; }
.blk-head { display: flex; align-items: center; gap: 6px; padding: 6px 10px; border-radius: 6px 6px 0 0; }
.blk-title { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }
.blk-badge { font-size: 11px; opacity: 0.85; }
.blk-badge.ok { color: #3fb950; }
.blk-badge.err { color: #f85149; }
.blk-desc { padding: 6px 10px 8px; font-size: 10.5px; color: var(--fg-muted); word-break: break-word; max-height: 64px; overflow: hidden; }
</style>
