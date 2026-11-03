<script setup lang="ts">
// Visual designer for native desktop UIs: Qt Widgets, QML and GTK4. Build the window with
// widgets, see the generated code live, and write it into the project (Python or C++).
import { ref, computed, onMounted } from 'vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { importUiFile } from '../composables/nativeUiImport'
import { invoke } from '@tauri-apps/api/core'
import NativeUiNode from './NativeUiNode.vue'
import { useEditorStore } from '../composables/useEditorStore'
import { ensureProject } from '../composables/ensureProject'
import { nativeUi as S, dragState } from '../composables/useNativeUiState'
import { WIDGETS, isContainer, newNode, generateNative, sampleLogin, sampleSettings, type UiNode, type WType, type Target, type CodeLang } from '../composables/nativeUi'
import { GAME_TEMPLATES } from '../composables/nativeGames'
import { useI18n } from '../composables/useI18n'
import { nativeText, gameLabel } from '../composables/nativeText'

const { t, locale } = useI18n()
const emit = defineEmits<{ back: [] }>()
const store = useEditorStore()
const status = ref('')
const savedDesigns = ref<string[]>([])
const root = computed(() => store.state.rootPath)

const TARGETS: { key: Target; label: string; langs: { key: CodeLang; label: string }[] }[] = [
  { key: 'qtwidgets', label: 'Qt Widgets (.ui)', langs: [{ key: 'pyside6', label: 'Python · PySide6' }, { key: 'pyqt6', label: 'Python · PyQt6' }, { key: 'cpp', label: 'C++ · CMake' }] },
  { key: 'qml', label: 'QML (Qt Quick)', langs: [{ key: 'pyside6', label: 'Python · PySide6' }] },
  { key: 'gtk4', label: 'GTK 4 (.ui)', langs: [{ key: 'pygobject', label: 'Python · PyGObject' }] },
]
const langs = computed(() => TARGETS.find(t => t.key === S.target)!.langs)
function onTarget() { if (!langs.value.some(l => l.key === S.lang)) S.lang = langs.value[0].key; S.fileIndex = 0 }

const say = (m: string) => { status.value = m; setTimeout(() => { if (status.value === m) status.value = '' }, 5000) }

// ── tree helpers ──
function find(n: UiNode, id: string): UiNode | null { if (n.id === id) return n; for (const c of n.children) { const r = find(c, id); if (r) return r } return null }
function parentOf(n: UiNode, id: string): UiNode | null { for (const c of n.children) { if (c.id === id) return n; const r = parentOf(c, id); if (r) return r } return null }
const selected = computed(() => (S.selectedId ? find(S.root, S.selectedId) : null))

function addWidget(type: WType) {
  const target = selected.value && isContainer(selected.value.type) ? selected.value : (selected.value ? parentOf(S.root, selected.value.id) : null) ?? S.root
  const n = newNode(type)
  target.children.push(n)
  S.selectedId = n.id
}
function removeSelected() {
  const sel = selected.value
  if (!sel || sel.type === 'window') return
  const p = parentOf(S.root, sel.id); if (!p) return
  p.children = p.children.filter(c => c.id !== sel.id); S.selectedId = null
}
function moveSelected(delta: number) {
  const sel = selected.value; if (!sel) return
  const p = parentOf(S.root, sel.id); if (!p) return
  const i = p.children.findIndex(c => c.id === sel.id), j = i + delta
  if (j < 0 || j >= p.children.length) return
  ;[p.children[i], p.children[j]] = [p.children[j], p.children[i]]
}
function duplicateSelected() {
  const sel = selected.value; if (!sel || sel.type === 'window') return
  const p = parentOf(S.root, sel.id); if (!p) return
  const clone = (n: UiNode): UiNode => ({ ...n, id: newNode('label').id, name: n.name ? n.name + '_copia' : '', children: n.children.map(clone) })
  const c = clone(sel); p.children.splice(p.children.findIndex(x => x.id === sel.id) + 1, 0, c); S.selectedId = c.id
}

// Drop: a palette widget or an existing one lands on a container (appended) or next to a widget (after it)
function onDrop(targetId: string) {
  const payload = dragState.payload; dragState.payload = null
  const target = find(S.root, targetId); if (!payload || !target) return
  let node: UiNode
  if (payload.kind === 'new') node = newNode(payload.type as WType)
  else {
    const mv = find(S.root, payload.id); if (!mv || mv.id === targetId || find(mv, targetId)) return   // can't drop into itself
    parentOf(S.root, mv.id)!.children = parentOf(S.root, mv.id)!.children.filter(c => c.id !== mv.id)
    node = mv
  }
  if (isContainer(target.type)) target.children.push(node)
  else { const p = parentOf(S.root, target.id)!; p.children.splice(p.children.findIndex(c => c.id === target.id) + 1, 0, node) }
  S.selectedId = node.id
}
function startPaletteDrag(type: WType, e: DragEvent) { dragState.payload = { kind: 'new', type }; e.dataTransfer?.setData('text/plain', type) }

function applyTemplate(v: string) {
  if (v === 'blank') S.root = newNode('window')
  else if (v === 'login') S.root = sampleLogin()
  else if (v === 'settings') S.root = sampleSettings()
  S.selectedId = null
}

// ── generated code ──
const files = computed(() => generateNative(S.root, S.target, S.lang, S.name))
const current = computed(() => files.value[Math.min(S.fileIndex, files.value.length - 1)])

async function copyCurrent() { try { await navigator.clipboard.writeText(current.value.content); say(t('uCopied')) } catch { say(t('uErrCopy')) } }

async function writeFiles(list: { path: string; content: string }[]) {
  for (const f of list) {
    const full = `${root.value}/${f.path}`
    await invoke('create_dir_cmd', { path: full.split('/').slice(0, -1).join('/') })
    await invoke('save_file', { path: full, content: f.content })
  }
}
async function generate(): Promise<boolean> {
  if (!(await ensureProject(S.name))) return false
  try {
    await writeFiles(files.value)
    const code = files.value.find(f => /\.(py|cpp)$/.test(f.path) && !f.path.endsWith('main.cpp')) ?? files.value[0]
    store.openFile(`${root.value}/${code.path}`)
    say(`Generado: ${files.value.map(f => f.path).join(', ')}`)
    return true
  } catch (e) { say(`Error: ${e}`); return false }
}
const runCommand = computed(() => {
  const n = (S.name.normalize('NFD').replace(/[̀-ͯ]/g, '').replace(/[^a-zA-Z0-9_]+/g, '_').replace(/^_+|_+$/g, '') || 'interfaz').toLowerCase()
  if (S.target === 'qtwidgets') return S.lang === 'cpp' ? `cd cpp_${n} && cmake -B build -S . && cmake --build build -j && ./build/${n}` : `python3 ${n}.py`
  return S.target === 'qml' ? `qml6 qml/${n}.qml` : `python3 ${n}_gtk.py`
})
async function generateAndRun() { if (await generate()) window.dispatchEvent(new CustomEvent('owee-run', { detail: runCommand.value })) }

// ── save / open the design itself ──
const designsDir = computed(() => `${root.value}/.owee/ui`)
async function refreshSaved() {
  if (!root.value) { savedDesigns.value = []; return }
  try { const l = await invoke<{ name: string; is_dir: boolean }[]>('list_dir', { path: designsDir.value, showHidden: true }); savedDesigns.value = l.filter(f => f.name.endsWith('.json')).map(f => f.name.replace(/\.json$/, '')) }
  catch { savedDesigns.value = [] }
}
async function saveDesign() {
  if (!(await ensureProject(S.name))) return
  try {
    await invoke('create_dir_cmd', { path: designsDir.value })
    await invoke('save_file', { path: `${designsDir.value}/${S.name}.json`, content: JSON.stringify({ version: 1, name: S.name, target: S.target, lang: S.lang, root: S.root }, null, 2) })
    say(t('uSaved', { name: S.name })); refreshSaved()
  } catch (e) { say(`Error: ${e}`) }
}
async function openDesign(name: string) {
  if (!name) return
  try {
    const d = JSON.parse(await invoke<string>('open_file', { path: `${designsDir.value}/${name}.json` }))
    S.name = name; S.target = d.target ?? 'qtwidgets'; S.lang = d.lang ?? 'pyside6'; S.root = d.root; S.selectedId = null; onTarget()
  } catch (e) { say(t('uErrOpen', { e: String(e) })) }
}

// ── games ──
async function createGame(id: string) {
  const g = GAME_TEMPLATES.find(x => x.id === id); if (!g) return
  if (!(await ensureProject('mi-juego'))) return
  try {
    await writeFiles([{ path: g.file, content: g.code }])
    store.openFile(`${root.value}/${g.file}`)
    say(`${g.file} creado. Ejecutalo con: ${g.run}`)
    window.dispatchEvent(new CustomEvent('owee-run', { detail: g.run }))
  } catch (e) { say(`Error: ${e}`) }
}

function onKey(e: KeyboardEvent) {
  const tag = (e.target as HTMLElement)?.tagName
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return
  if (e.key === 'Delete') removeSelected()
}
const flatOutline = computed(() => { const out: { n: UiNode; depth: number }[] = []; const walk = (n: UiNode, d: number) => { out.push({ n, depth: d }); n.children.forEach(c => walk(c, d + 1)) }; walk(S.root, 0); return out })
const label = (t: WType) => nativeText(WIDGETS.find(w => w.type === t)?.label ?? (t === 'window' ? 'Ventana' : t), locale.value)

// Open an existing Qt Designer / GTK .ui file as a design
async function importUi(path?: string) {
  try {
    const p = path ?? (await openDialog({ title: t('uPickFile'), multiple: false, filters: [{ name: t('uFilter'), extensions: ['ui', 'qml'] }], defaultPath: root.value || undefined }))
    if (typeof p !== 'string') return
    const r = importUiFile(await invoke<string>('open_file', { path: p }))
    S.root = r.root; S.target = r.target; S.lang = r.target === 'gtk4' ? 'pygobject' : 'pyside6'; S.selectedId = null; S.fileIndex = 0
    S.name = (p.split('/').pop() ?? 'interfaz').replace(/\.(gtk\.)?(ui|qml)$/, '')
    say(r.notes.length ? `Importado con ajustes: ${r.notes.join(' · ')}` : `Importado: ${p.split('/').pop()}`)
  } catch (e) { say(t('uErrImport', { e: String(e) })) }
}

onMounted(() => {
  S.active = true; refreshSaved()
  if (S.pendingImport) { const p = S.pendingImport; S.pendingImport = null; importUi(p) }
})
</script>

<template>
  <div class="nu" tabindex="0" @keydown="onKey">
    <div class="nu-bar">
      <button class="nb" @click="S.active = false; emit('back')">{{ t('uBack') }}</button>
      <input v-model="S.name" class="ni" :placeholder="t('uNamePh')" style="width:130px" />
      <select v-model="S.target" class="ni" @change="onTarget"><option v-for="t in TARGETS" :key="t.key" :value="t.key">{{ t.label }}</option></select>
      <select v-model="S.lang" class="ni"><option v-for="l in langs" :key="l.key" :value="l.key">{{ l.label }}</option></select>
      <button class="nb" @click="importUi()" :title="t('uImportTip')">{{ t('uImport') }}</button>
      <select class="ni" @change="applyTemplate(($event.target as HTMLSelectElement).value); ($event.target as HTMLSelectElement).value = ''"><option value="">{{ t('uTemplate') }}</option><option value="blank">{{ t('uBlank') }}</option><option value="login">{{ t('uLogin') }}</option><option value="settings">{{ t('uSettings') }}</option></select>
      <select class="ni" @change="createGame(($event.target as HTMLSelectElement).value); ($event.target as HTMLSelectElement).value = ''"><option value="">{{ t('uGame') }}</option><option v-for="g in GAME_TEMPLATES" :key="g.id" :value="g.id">{{ gameLabel(g.label, locale) }}</option></select>
      <div class="sp" />
      <span v-if="status" class="st">{{ status }}</span>
      <select class="ni" @change="openDesign(($event.target as HTMLSelectElement).value); ($event.target as HTMLSelectElement).value = ''" @focus="refreshSaved"><option value="">{{ t('uOpenDesign') }}</option><option v-for="d in savedDesigns" :key="d" :value="d">{{ d }}</option></select>
      <button class="nb" @click="saveDesign">{{ t('uSaveDesign') }}</button>
      <button class="nb acc" @click="generate">{{ t('uGenerate') }}</button>
      <button class="nb run" @click="generateAndRun" :title="runCommand">{{ t('uGenRun') }}</button>
    </div>

    <div class="nu-body">
      <aside class="pal">
        <div class="h">Widgets <small>{{ t('uClickDrag') }}</small></div>
        <button v-for="w in WIDGETS" :key="w.type" class="wbtn" draggable="true" @dragstart="startPaletteDrag(w.type, $event)" @click="addWidget(w.type)"><span>{{ w.icon }}</span>{{ nativeText(w.label, locale) }}</button>
        <div class="h" style="margin-top:12px">{{ t('uStructure') }}</div>
        <div class="outline">
          <div v-for="o in flatOutline" :key="o.n.id" class="ol" :class="{ on: S.selectedId === o.n.id }" :style="{ paddingLeft: 8 + o.depth * 12 + 'px' }" @click="S.selectedId = o.n.id">
            {{ label(o.n.type) }}<em v-if="o.n.name || o.n.text"> · {{ (o.n.name || o.n.text).slice(0, 16) }}</em>
          </div>
        </div>
      </aside>

      <main class="stage" @click="S.selectedId = null">
        <NativeUiNode :node="S.root" :selected-id="S.selectedId" @select="S.selectedId = $event" @drop="onDrop" />
      </main>

      <aside class="props">
        <template v-if="selected">
          <div class="h">{{ label(selected.type) }}</div>
          <div class="row2">
            <button class="mini" @click="moveSelected(-1)" :title="t('uUp')">↑</button><button class="mini" @click="moveSelected(1)" :title="t('uDown')">↓</button>
            <button class="mini" @click="duplicateSelected" :title="t('uDup')">⧉</button>
            <button class="mini del" @click="removeSelected" :title="t('uDel')">🗑</button>
          </div>
          <label>{{ t('uCodeName') }}</label><input v-model="selected.name" class="ni w" :placeholder="t('uAuto')" />
          <template v-if="['window', 'frame', 'label', 'button', 'checkbox'].includes(selected.type)"><label>{{ selected.type === 'window' ? t('uTitle') : t('uText') }}</label><input v-model="selected.text" class="ni w" /></template>
          <template v-if="selected.type === 'image'"><label>{{ t('uImageFile') }}</label><input v-model="selected.text" class="ni w" placeholder="img/logo.png" /></template>
          <template v-if="['lineedit', 'textedit'].includes(selected.type)"><label>{{ t('uHelpText') }}</label><input v-model="selected.placeholder" class="ni w" /></template>
          <template v-if="['combobox', 'listbox'].includes(selected.type)"><label>{{ t('uItems') }}</label><textarea v-model="selected.items" class="ni w" rows="5" /></template>
          <template v-if="['slider', 'progress', 'spinbox'].includes(selected.type)">
            <div class="row2"><div><label>{{ t('uMin') }}</label><input v-model.number="selected.min" type="number" class="ni w" /></div><div><label>{{ t('uMax') }}</label><input v-model.number="selected.max" type="number" class="ni w" /></div><div><label>{{ t('uValue') }}</label><input v-model.number="selected.value" type="number" class="ni w" /></div></div>
          </template>
          <template v-if="selected.type === 'checkbox'"><label class="inl"><input type="checkbox" :checked="!!selected.value" @change="selected.value = ($event.target as HTMLInputElement).checked ? 1 : 0" /> {{ t('uChecked') }}</label></template>
          <template v-if="selected.type === 'window'">
            <div class="row2"><div><label>{{ t('uWidth') }}</label><input v-model.number="selected.width" type="number" class="ni w" /></div><div><label>{{ t('uHeight') }}</label><input v-model.number="selected.height" type="number" class="ni w" /></div></div>
            <div class="row2"><div><label>{{ t('uMargin') }}</label><input v-model.number="selected.margin" type="number" class="ni w" /></div><div><label>{{ t('uSpacing') }}</label><input v-model.number="selected.spacing" type="number" class="ni w" /></div></div>
          </template>
          <template v-if="['vbox', 'hbox', 'frame'].includes(selected.type)"><label>{{ t('uSpacing') }}</label><input v-model.number="selected.spacing" type="number" class="ni w" /></template>
          <template v-if="selected.type !== 'window' && selected.type !== 'separator'"><label class="inl"><input type="checkbox" v-model="selected.expand" /> {{ t('uExpand') }}</label></template>
          <template v-if="!['window', 'vbox', 'hbox', 'frame', 'label', 'progress', 'image', 'separator', 'spacer'].includes(selected.type)">
            <label>{{ t('uHandler', { when: selected.type === 'button' ? t('uOnClick') : t('uOnChange') }) }}</label>
            <input v-model="selected.handler" class="ni w" placeholder="on_guardar" />
            <div class="hint">{{ t('uHandlerHint') }}</div>
          </template>
        </template>
        <div v-else class="hint">{{ t('uPickWidget') }}</div>

        <div class="h" style="margin-top:14px">{{ t('uGenerated') }}</div>
        <div class="tabs"><button v-for="(f, i) in files" :key="f.path" :class="{ on: i === Math.min(S.fileIndex, files.length - 1) }" @click="S.fileIndex = i">{{ f.path.split('/').pop() }}</button></div>
        <pre class="code">{{ current.content }}</pre>
        <button class="nb" @click="copyCurrent">{{ t('uCopyFile') }}</button>
        <div class="hint" v-if="S.target === 'gtk4'" v-html="t('uGtkNote')" />
      </aside>
    </div>
  </div>
</template>

<style scoped>
.nu { display: flex; flex-direction: column; height: 100%; width: 100%; background: var(--bg-darkest, #0f1012); color: var(--fg); outline: none; }
.nu-bar { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 6px 10px; border-bottom: 1px solid var(--border); background: var(--bg-dark, #141416); }
.sp { flex: 1; } .st { font-size: 11px; color: var(--accent); max-width: 40%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.nb { background: none; border: 1px solid var(--border); color: var(--fg); font-size: 11px; padding: 4px 9px; border-radius: 4px; cursor: pointer; } .nb:hover { background: var(--bg-hover); }
.nb.acc { border-color: var(--accent); color: var(--accent); } .nb.run { border-color: #3fb950; color: #3fb950; font-weight: 600; }
.ni { background: var(--bg-darkest); border: 1px solid var(--border); color: var(--fg); font-size: 12px; padding: 4px 7px; border-radius: 4px; } .ni.w { width: 100%; box-sizing: border-box; margin-bottom: 6px; resize: vertical; font-family: inherit; }
.nu-body { flex: 1; display: flex; min-height: 0; }
.pal { width: 190px; border-right: 1px solid var(--border); background: var(--bg-dark, #141416); padding: 8px; overflow: auto; }
.props { width: 330px; border-left: 1px solid var(--border); background: var(--bg-dark, #141416); padding: 10px; overflow: auto; }
.h { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: .05em; color: var(--fg-muted); margin-bottom: 6px; } .h small { text-transform: none; font-weight: 400; }
.wbtn { display: flex; align-items: center; gap: 8px; width: 100%; text-align: left; background: none; border: none; color: var(--fg); font-size: 12px; padding: 5px 6px; border-radius: 4px; cursor: grab; } .wbtn:hover { background: var(--bg-hover); } .wbtn span { width: 22px; text-align: center; color: var(--accent); }
.outline { font-size: 11px; max-height: 240px; overflow: auto; border: 1px solid var(--border); border-radius: 4px; } .ol { padding: 3px 8px; cursor: pointer; white-space: nowrap; } .ol:hover { background: var(--bg-hover); } .ol.on { background: rgba(76,154,255,.25); } .ol em { color: var(--fg-muted); font-style: normal; }
.stage { flex: 1; overflow: auto; padding: 24px; display: flex; justify-content: center; align-items: flex-start; background: repeating-conic-gradient(#17181b 0% 25%, #141517 0% 50%) 0 0 / 20px 20px; }
.stage > * { max-width: 780px; }
.props label { display: block; font-size: 10px; text-transform: uppercase; letter-spacing: .06em; color: var(--fg-muted); margin: 6px 0 3px; } .props label.inl { display: flex; align-items: center; gap: 6px; text-transform: none; font-size: 12px; color: var(--fg); letter-spacing: 0; }
.row2 { display: flex; gap: 6px; } .row2 > div { flex: 1; min-width: 0; } .mini { background: none; border: 1px solid var(--border); color: var(--fg); border-radius: 4px; width: 30px; height: 26px; cursor: pointer; margin-bottom: 4px; } .mini.del { color: #f85149; border-color: #f8514966; }
.hint { font-size: 11px; color: var(--fg-muted); line-height: 1.45; margin: 4px 0 8px; } .hint code { background: var(--bg-darkest); padding: 0 3px; border-radius: 3px; }
.tabs { display: flex; flex-wrap: wrap; gap: 2px; margin-bottom: 4px; } .tabs button { background: none; border: 1px solid var(--border); color: var(--fg-muted); font-size: 10.5px; padding: 3px 7px; border-radius: 4px 4px 0 0; cursor: pointer; } .tabs button.on { background: var(--bg-hover); color: var(--fg-bright, #fff); }
.code { max-height: 300px; overflow: auto; background: var(--bg-darkest); border: 1px solid var(--border); border-radius: 4px; padding: 8px; font-size: 10.5px; line-height: 1.4; white-space: pre; margin: 0 0 8px; user-select: text; }
</style>
