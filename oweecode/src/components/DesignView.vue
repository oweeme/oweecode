<script setup lang="ts">
import { ref, computed, nextTick, onBeforeUnmount } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog'
import grapesjs, { type Editor } from 'grapesjs'
import presetWebpage from 'grapesjs-preset-webpage'
import pluginForms from 'grapesjs-plugin-forms'
import 'grapesjs/dist/css/grapes.min.css'
import { VueFlow, Handle, Position, type Node, type Edge, type Connection, type EdgeChange, type GraphNode } from '@vue-flow/core'
import { Background } from '@vue-flow/background'
import { Controls } from '@vue-flow/controls'
import '@vue-flow/core/dist/style.css'
import '@vue-flow/core/dist/theme-default.css'
import '@vue-flow/controls/dist/style.css'
import { useI18n } from '../composables/useI18n'
import { useEditorStore } from '../composables/useEditorStore'

type DesignFormat = 'html' | 'vue'

interface DesignPage {
  id: string
  title: string
  description: string
  file: string
  x: number
  y: number
}

interface DesignEdge {
  id: string
  source: string
  target: string
}

interface DesignManifest {
  name: string
  format: DesignFormat
  pages: DesignPage[]
  edges: DesignEdge[]
}

const MANIFEST_NAME = 'oweeme-design.json'

// DaisyUI ships a precompiled stylesheet with every component class already
// baked in (no Tailwind build step needed) — used only for 'html' projects,
// kept out of the 'vue' path so it doesn't clash with the user's own
// Quasar/Tailwind setup in their real project.
const DAISYUI_CSS_URL = 'https://cdn.jsdelivr.net/npm/daisyui@4/dist/full.min.css'

const BASE_BLOCK_CSS = `
.ow-navbar { display:flex; align-items:center; justify-content:space-between; padding:14px 28px; background:#111827; color:#fff; font-family:system-ui,sans-serif; }
.ow-navbar__brand { font-weight:700; font-size:18px; }
.ow-navbar__actions { display:flex; gap:10px; }
.ow-hero { text-align:center; padding:80px 24px; background:linear-gradient(135deg,#1e293b,#0f172a); color:#fff; font-family:system-ui,sans-serif; }
.ow-hero h1 { font-size:40px; margin:0 0 12px; }
.ow-hero p { font-size:16px; opacity:.85; margin:0 0 24px; max-width:520px; margin-left:auto; margin-right:auto; }
.ow-btn { display:inline-block; padding:10px 22px; border-radius:6px; background:#6366f1; color:#fff; border:none; font-size:14px; font-weight:600; cursor:pointer; text-decoration:none; font-family:system-ui,sans-serif; }
.ow-btn:hover { background:#4f46e5; }
.ow-card { border:1px solid #e2e8f0; border-radius:10px; padding:22px; max-width:360px; background:#fff; box-shadow:0 2px 8px rgba(0,0,0,.06); font-family:system-ui,sans-serif; }
.ow-card h3 { margin:0 0 8px; font-size:18px; color:#111827; }
.ow-card p { margin:0; font-size:14px; color:#475569; }
.ow-container { min-height:120px; border:2px dashed #94a3b8; border-radius:8px; }
`

// Mirrors daisyui.com/components/ — grouped the same way their own docs are,
// so it's easy to cross-reference. Every entry is plain markup + DaisyUI
// classes; only the modal/dropdown rely on DaisyUI's CSS-only/native-<dialog>
// tricks, no extra JS wiring needed on our side.
const DAISY_BLOCKS: { id: string; label: string; category: string; content: string }[] = [
  // Acciones
  { id: 'daisy-button', label: 'Botón', category: 'DaisyUI · Acciones', content: `<button class="btn btn-primary">Botón</button>` },
  { id: 'daisy-button-group', label: 'Grupo de botones (Join)', category: 'DaisyUI · Acciones', content: `<div class="join"><button class="btn join-item">1</button><button class="btn join-item">2</button><button class="btn join-item">3</button></div>` },
  { id: 'daisy-dropdown', label: 'Menú desplegable', category: 'DaisyUI · Acciones', content: `<div class="dropdown"><div tabindex="0" role="button" class="btn m-1">Menú</div><ul tabindex="0" class="dropdown-content menu bg-base-100 rounded-box z-1 w-52 p-2 shadow-sm"><li><a>Opción 1</a></li><li><a>Opción 2</a></li></ul></div>` },
  { id: 'daisy-modal', label: 'Modal', category: 'DaisyUI · Acciones', content: `<button class="btn" onclick="ow_modal_1.showModal()">Abrir modal</button><dialog id="ow_modal_1" class="modal"><div class="modal-box"><h3 class="text-lg font-bold">Título</h3><p class="py-4">Contenido del modal.</p><div class="modal-action"><form method="dialog"><button class="btn">Cerrar</button></form></div></div></dialog>` },
  { id: 'daisy-swap', label: 'Swap (alternar ícono)', category: 'DaisyUI · Acciones', content: `<label class="swap swap-rotate"><input type="checkbox" /><div class="swap-on">ON</div><div class="swap-off">OFF</div></label>` },

  // Visualización de datos
  { id: 'daisy-card', label: 'Tarjeta', category: 'DaisyUI · Datos', content: `<div class="card w-96 bg-base-100 shadow-xl"><div class="card-body"><h2 class="card-title">Título de la tarjeta</h2><p>Contenido de la tarjeta.</p><div class="card-actions justify-end"><button class="btn btn-primary">Ver más</button></div></div></div>` },
  { id: 'daisy-avatar', label: 'Avatar', category: 'DaisyUI · Datos', content: `<div class="avatar"><div class="w-24 rounded-full"><img src="https://img.daisyui.com/images/profile/demo/2@94.webp" /></div></div>` },
  { id: 'daisy-badge', label: 'Badge', category: 'DaisyUI · Datos', content: `<div class="badge badge-primary">Nuevo</div>` },
  { id: 'daisy-table', label: 'Tabla', category: 'DaisyUI · Datos', content: `<div class="overflow-x-auto"><table class="table"><thead><tr><th>Nombre</th><th>Cargo</th><th>Ciudad</th></tr></thead><tbody><tr><td>Ana</td><td>Diseñadora</td><td>La Paz</td></tr><tr><td>Luis</td><td>Desarrollador</td><td>Santa Cruz</td></tr></tbody></table></div>` },
  { id: 'daisy-stat', label: 'Estadística', category: 'DaisyUI · Datos', content: `<div class="stats shadow"><div class="stat"><div class="stat-title">Visitas</div><div class="stat-value">89,400</div><div class="stat-desc">21% más que el mes pasado</div></div></div>` },
  { id: 'daisy-timeline', label: 'Línea de tiempo', category: 'DaisyUI · Datos', content: `<ul class="timeline timeline-vertical"><li><div class="timeline-start">2024</div><div class="timeline-middle"><span class="badge badge-primary badge-xs"></span></div><div class="timeline-end timeline-box">Lanzamiento</div></li><li><div class="timeline-start">2025</div><div class="timeline-middle"><span class="badge badge-primary badge-xs"></span></div><div class="timeline-end timeline-box">Nueva versión</div></li></ul>` },
  { id: 'daisy-chat', label: 'Burbujas de chat', category: 'DaisyUI · Datos', content: `<div class="chat chat-start"><div class="chat-bubble">¡Hola! ¿Cómo estás?</div></div><div class="chat chat-end"><div class="chat-bubble chat-bubble-primary">¡Todo bien, gracias!</div></div>` },
  { id: 'daisy-collapse', label: 'Acordeón (Collapse)', category: 'DaisyUI · Datos', content: `<div class="collapse collapse-arrow bg-base-100 border border-base-300"><input type="checkbox" /><div class="collapse-title font-semibold">¿Cómo funciona?</div><div class="collapse-content text-sm">Acá va la respuesta o el contenido desplegable.</div></div>` },
  { id: 'daisy-list', label: 'Lista', category: 'DaisyUI · Datos', content: `<ul class="list bg-base-100 rounded-box shadow-md"><li class="list-row"><div>Elemento uno</div></li><li class="list-row"><div>Elemento dos</div></li></ul>` },

  // Navegación
  { id: 'daisy-navbar', label: 'Navbar', category: 'DaisyUI · Navegación', content: `<div class="navbar bg-base-100 shadow-sm"><div class="flex-1"><a class="btn btn-ghost text-xl">Marca</a></div><div class="flex-none"><button class="btn btn-primary">Acción</button></div></div>` },
  { id: 'daisy-breadcrumbs', label: 'Breadcrumbs', category: 'DaisyUI · Navegación', content: `<div class="breadcrumbs text-sm"><ul><li><a>Inicio</a></li><li><a>Categoría</a></li><li>Producto</li></ul></div>` },
  { id: 'daisy-menu', label: 'Menú lateral', category: 'DaisyUI · Navegación', content: `<ul class="menu bg-base-200 rounded-box w-56"><li><a>Inicio</a></li><li><a>Servicios</a></li><li><a>Contacto</a></li></ul>` },
  { id: 'daisy-tabs', label: 'Pestañas', category: 'DaisyUI · Navegación', content: `<div role="tablist" class="tabs tabs-lift"><a role="tab" class="tab tab-active">Tab 1</a><a role="tab" class="tab">Tab 2</a><a role="tab" class="tab">Tab 3</a></div>` },
  { id: 'daisy-pagination', label: 'Paginación', category: 'DaisyUI · Navegación', content: `<div class="join"><button class="join-item btn">1</button><button class="join-item btn btn-active">2</button><button class="join-item btn">3</button></div>` },
  { id: 'daisy-steps', label: 'Pasos (Steps)', category: 'DaisyUI · Navegación', content: `<ul class="steps"><li class="step step-primary">Cuenta</li><li class="step step-primary">Envío</li><li class="step">Pago</li><li class="step">Confirmación</li></ul>` },
  { id: 'daisy-footer', label: 'Footer', category: 'DaisyUI · Navegación', content: `<footer class="footer bg-neutral text-neutral-content p-10"><nav><h6 class="footer-title">Servicios</h6><a class="link link-hover">Branding</a><a class="link link-hover">Diseño</a></nav><nav><h6 class="footer-title">Empresa</h6><a class="link link-hover">Nosotros</a><a class="link link-hover">Contacto</a></nav></footer>` },

  // Feedback
  { id: 'daisy-alert', label: 'Alerta', category: 'DaisyUI · Feedback', content: `<div class="alert alert-info"><span>Mensaje informativo para el usuario.</span></div>` },
  { id: 'daisy-toast', label: 'Toast', category: 'DaisyUI · Feedback', content: `<div class="toast toast-top toast-end"><div class="alert alert-info"><span>Nuevo mensaje.</span></div></div>` },
  { id: 'daisy-progress', label: 'Barra de progreso', category: 'DaisyUI · Feedback', content: `<progress class="progress progress-primary w-56" value="60" max="100"></progress>` },
  { id: 'daisy-radial-progress', label: 'Progreso radial', category: 'DaisyUI · Feedback', content: `<div class="radial-progress" style="--value:70;" role="progressbar">70%</div>` },
  { id: 'daisy-loading', label: 'Cargando (spinner)', category: 'DaisyUI · Feedback', content: `<span class="loading loading-spinner loading-lg"></span>` },
  { id: 'daisy-skeleton', label: 'Skeleton', category: 'DaisyUI · Feedback', content: `<div class="flex flex-col gap-4 w-52"><div class="skeleton h-32 w-full"></div><div class="skeleton h-4 w-28"></div><div class="skeleton h-4 w-full"></div></div>` },
  { id: 'daisy-tooltip', label: 'Tooltip', category: 'DaisyUI · Feedback', content: `<div class="tooltip" data-tip="Información adicional"><button class="btn">Pasá el mouse</button></div>` },

  // Entrada de datos
  { id: 'daisy-input', label: 'Campo de texto', category: 'DaisyUI · Formularios', content: `<input type="text" placeholder="Escribí acá" class="input input-bordered w-full max-w-xs" />` },
  { id: 'daisy-textarea', label: 'Área de texto', category: 'DaisyUI · Formularios', content: `<textarea class="textarea textarea-bordered w-full" placeholder="Escribí tu mensaje..."></textarea>` },
  { id: 'daisy-select', label: 'Select', category: 'DaisyUI · Formularios', content: `<select class="select select-bordered w-full max-w-xs"><option disabled selected>Elegí una opción</option><option>Opción 1</option><option>Opción 2</option></select>` },
  { id: 'daisy-checkbox', label: 'Checkbox', category: 'DaisyUI · Formularios', content: `<label class="label cursor-pointer gap-2 justify-start"><input type="checkbox" class="checkbox" checked /><span class="label-text">Acepto los términos</span></label>` },
  { id: 'daisy-radio', label: 'Radio', category: 'DaisyUI · Formularios', content: `<div class="flex gap-4"><label class="label gap-2"><input type="radio" name="ow-radio" class="radio" checked /> Opción A</label><label class="label gap-2"><input type="radio" name="ow-radio" class="radio" /> Opción B</label></div>` },
  { id: 'daisy-toggle', label: 'Toggle', category: 'DaisyUI · Formularios', content: `<label class="label cursor-pointer gap-2 justify-start"><input type="checkbox" class="toggle toggle-primary" checked /><span class="label-text">Notificaciones</span></label>` },
  { id: 'daisy-range', label: 'Range (slider)', category: 'DaisyUI · Formularios', content: `<input type="range" min="0" max="100" value="40" class="range range-primary" />` },
  { id: 'daisy-rating', label: 'Rating (estrellas)', category: 'DaisyUI · Formularios', content: `<div class="rating"><input type="radio" name="ow-rating" class="mask mask-star" /><input type="radio" name="ow-rating" class="mask mask-star" checked /><input type="radio" name="ow-rating" class="mask mask-star" /><input type="radio" name="ow-rating" class="mask mask-star" /><input type="radio" name="ow-rating" class="mask mask-star" /></div>` },
  { id: 'daisy-file-input', label: 'Subir archivo', category: 'DaisyUI · Formularios', content: `<input type="file" class="file-input file-input-bordered w-full max-w-xs" />` },
  { id: 'daisy-form', label: 'Formulario de acceso', category: 'DaisyUI · Formularios', content: `<div class="card w-96 bg-base-100 shadow-xl p-6"><h2 class="text-xl font-bold mb-4">Iniciar sesión</h2><div class="form-control mb-3"><label class="label"><span class="label-text">Email</span></label><input type="email" placeholder="tu@email.com" class="input input-bordered" /></div><div class="form-control mb-4"><label class="label"><span class="label-text">Contraseña</span></label><input type="password" placeholder="********" class="input input-bordered" /></div><button class="btn btn-primary w-full">Ingresar</button></div>` },

  // Layout
  { id: 'daisy-hero', label: 'Hero', category: 'DaisyUI · Layout', content: `<div class="hero bg-base-200 min-h-[420px]"><div class="hero-content text-center"><div class="max-w-md"><h1 class="text-5xl font-bold">Título</h1><p class="py-6">Descripción del producto o servicio.</p><button class="btn btn-primary">Empezar</button></div></div></div>` },
  { id: 'daisy-divider', label: 'Divisor', category: 'DaisyUI · Layout', content: `<div class="divider">O</div>` },
  { id: 'daisy-indicator', label: 'Indicador (badge flotante)', category: 'DaisyUI · Layout', content: `<div class="indicator"><span class="indicator-item badge badge-secondary">Nuevo</span><button class="btn">Notificaciones</button></div>` },
  { id: 'daisy-carousel', label: 'Carrusel', category: 'DaisyUI · Layout', content: `<div class="carousel w-full"><div class="carousel-item w-full"><img src="https://img.daisyui.com/images/stock/photo-1625726411847-8cbb60cc71e6.webp" class="w-full" /></div></div>` },
]

function registerCustomBlocks(ed: Editor, format: DesignFormat) {
  const bm = ed.BlockManager
  bm.add('ow-navbar', {
    label: 'Barra de navegación', category: 'Profesional',
    content: `<div class="ow-navbar"><span class="ow-navbar__brand">Mi Sitio</span><div class="ow-navbar__actions"><a class="ow-btn" href="#">Acción</a></div></div>`,
  })
  bm.add('ow-hero', {
    label: 'Encabezado / Hero', category: 'Profesional',
    content: `<section class="ow-hero"><h1>Título llamativo</h1><p>Subtítulo o descripción breve que explica tu producto o servicio.</p><a class="ow-btn" href="#">Empezar</a></section>`,
  })
  bm.add('ow-card', {
    label: 'Tarjeta con borde', category: 'Profesional',
    content: `<div class="ow-card"><h3>Título de la tarjeta</h3><p>Descripción breve del contenido de esta tarjeta.</p></div>`,
  })
  bm.add('ow-button', {
    label: 'Botón', category: 'Profesional',
    content: `<a class="ow-btn" href="#">Botón</a>`,
  })
  bm.add('ow-container', {
    label: 'Contenedor vacío', category: 'Profesional',
    content: `<div class="ow-container"></div>`,
  })

  if (format === 'html') {
    for (const b of DAISY_BLOCKS) {
      bm.add(b.id, { label: b.label, category: b.category, content: b.content })
    }
  }
}

const { t } = useI18n()
const store = useEditorStore()

type Mode = 'start' | 'map' | 'editor'
const mode = ref<Mode>('start')

const projectDir = ref<string | null>(null)
const manifest = ref<DesignManifest | null>(null)
const currentPageId = ref<string | null>(null)

const newName = ref('')
const newFormat = ref<DesignFormat>('html')
const newDir = ref(store.state.rootPath || '')
const creating = ref(false)
const openingError = ref('')
const newPageTitle = ref('')

const aiPrompt = ref('')
const aiLoading = ref(false)
const aiError = ref('')

const canvasEl = ref<HTMLElement | null>(null)
const statusMsg = ref('')
let editor: Editor | null = null

function slugify(name: string): string {
  return name.trim().toLowerCase().replace(/[^a-z0-9_-]+/gi, '-').replace(/^-+|-+$/g, '') || 'pagina'
}

function uniquePageId(title: string): string {
  const base = slugify(title)
  if (!manifest.value) return base
  let id = base
  let n = 2
  while (manifest.value.pages.some(p => p.id === id)) { id = `${base}-${n}`; n++ }
  return id
}

async function pickNewDir() {
  const selected = await openDialog({ directory: true, multiple: false })
  if (selected && typeof selected === 'string') newDir.value = selected
}

function wrapHtmlDocument(title: string, html: string, css: string, daisy = false): string {
  return `<!DOCTYPE html>
<html lang="es">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>${title}</title>
${daisy ? `<link rel="stylesheet" href="${DAISYUI_CSS_URL}">\n` : ''}<style>
${css}
</style>
</head>
<body>
${html}
</body>
</html>
`
}

function wrapVueSfc(html: string, css: string): string {
  // GrapesJS's HTML output is plain markup + attributes, which is valid
  // straight inside a Vue <template> — Vue templates are a superset of HTML.
  return `<script setup lang="ts">
<\/script>

<template>
${html}
</template>

<style scoped>
${css}
</style>
`
}

function parseHtmlDocument(fileContent: string): { html: string; css: string } {
  const doc = new DOMParser().parseFromString(fileContent, 'text/html')
  return { html: doc.body.innerHTML, css: doc.querySelector('style')?.textContent ?? '' }
}

function parseVueSfc(fileContent: string): { html: string; css: string } {
  const templateMatch = fileContent.match(/<template>([\s\S]*)<\/template>/)
  const styleMatch = fileContent.match(/<style scoped>([\s\S]*)<\/style>/)
  return { html: templateMatch?.[1]?.trim() ?? '', css: styleMatch?.[1]?.trim() ?? '' }
}

// ── Project create / open ──────────────────────────────────────────────
async function createProject() {
  if (!newName.value.trim() || !newDir.value) return
  creating.value = true
  try {
    const folder = `${newDir.value}/${slugify(newName.value)}`
    await invoke('create_dir_cmd', { path: `${folder}/pages` })
    const ext = newFormat.value === 'vue' ? 'vue' : 'html'
    const home: DesignPage = { id: 'home', title: 'Inicio', description: '', file: `pages/home.${ext}`, x: 60, y: 100 }
    const m: DesignManifest = { name: newName.value.trim(), format: newFormat.value, pages: [home], edges: [] }
    const initialContent = newFormat.value === 'vue'
      ? wrapVueSfc('', '')
      : wrapHtmlDocument(home.title, '', '', true)
    await invoke('save_file', { path: `${folder}/${MANIFEST_NAME}`, content: JSON.stringify(m, null, 2) })
    await invoke('save_file', { path: `${folder}/${home.file}`, content: initialContent })
    projectDir.value = folder
    manifest.value = m
    mode.value = 'map'
  } finally {
    creating.value = false
  }
}

async function openExistingProject() {
  openingError.value = ''
  const folder = await openDialog({ directory: true, multiple: false })
  if (!folder || typeof folder !== 'string') return
  try {
    const manifestRaw = await invoke<string>('open_file', { path: `${folder}/${MANIFEST_NAME}` })
    manifest.value = JSON.parse(manifestRaw) as DesignManifest
    projectDir.value = folder
    mode.value = 'map'
  } catch (e) {
    openingError.value = String(e)
  }
}

// ── Site map ────────────────────────────────────────────────────────────
const flowNodes = computed<Node[]>(() => (manifest.value?.pages ?? []).map(p => ({
  id: p.id,
  type: 'page',
  position: { x: p.x, y: p.y },
  data: { page: p },
})))

const flowEdges = computed<Edge[]>(() => (manifest.value?.edges ?? []).map(e => ({
  id: e.id, source: e.source, target: e.target,
})))

async function saveManifest() {
  if (!projectDir.value || !manifest.value) return
  await invoke('save_file', { path: `${projectDir.value}/${MANIFEST_NAME}`, content: JSON.stringify(manifest.value, null, 2) })
}

async function addPage() {
  if (!newPageTitle.value.trim() || !manifest.value || !projectDir.value) return
  const title = newPageTitle.value.trim()
  const id = uniquePageId(title)
  const ext = manifest.value.format === 'vue' ? 'vue' : 'html'
  const file = `pages/${id}.${ext}`
  const last = manifest.value.pages[manifest.value.pages.length - 1]
  const page: DesignPage = { id, title, description: '', file, x: (last?.x ?? 40) + 260, y: last?.y ?? 100 }
  const initialContent = manifest.value.format === 'vue'
    ? wrapVueSfc('', '')
    : wrapHtmlDocument(title, '', '', true)
  await invoke('save_file', { path: `${projectDir.value}/${file}`, content: initialContent })
  manifest.value.pages.push(page)
  newPageTitle.value = ''
  await saveManifest()
}

function removePage(id: string) {
  if (!manifest.value || manifest.value.pages.length <= 1) return
  manifest.value.pages = manifest.value.pages.filter(p => p.id !== id)
  manifest.value.edges = manifest.value.edges.filter(e => e.source !== id && e.target !== id)
  saveManifest()
}

function onConnect(connection: Connection) {
  if (!manifest.value || connection.source === connection.target) return
  const exists = manifest.value.edges.some(e => e.source === connection.source && e.target === connection.target)
  if (exists) return
  manifest.value.edges.push({ id: crypto.randomUUID(), source: connection.source, target: connection.target })
  saveManifest()
}

function onNodeDragStop({ node }: { node: GraphNode }) {
  const page = manifest.value?.pages.find(p => p.id === node.id)
  if (page) { page.x = node.position.x; page.y = node.position.y }
  saveManifest()
}

function onEdgesChange(changes: EdgeChange[]) {
  if (!manifest.value) return
  for (const c of changes) {
    if (c.type === 'remove') manifest.value.edges = manifest.value.edges.filter(e => e.id !== c.id)
  }
  saveManifest()
}

function onNodeDoubleClick({ node }: { node: GraphNode }) {
  openPageEditor(node.id)
}

// ── Page editor (GrapesJS) ────────────────────────────────────────────
async function mountEditor(html: string, css: string, format: DesignFormat) {
  await nextTick()
  if (!canvasEl.value) return
  editor?.destroy()
  editor = grapesjs.init({
    container: canvasEl.value,
    height: '100%',
    fromElement: false,
    storageManager: false,
    // Native HTML5 drag-and-drop across the canvas iframe boundary is
    // unreliable in WebKitGTK (Tauri's Linux webview) — GrapesJS's own
    // JS-simulated drag works everywhere, so we opt out of the native one.
    nativeDnD: false,
    plugins: [presetWebpage, pluginForms],
    canvas: format === 'html' ? { styles: [DAISYUI_CSS_URL] } : {},
    blockManager: {
      // Click-to-insert is more practical than drag for most users, and
      // sidesteps drag entirely as a fallback.
      appendOnClick: true,
    },
  })
  registerCustomBlocks(editor, format)
  editor.setStyle(`${BASE_BLOCK_CSS}\n${css}`)
  if (html) editor.setComponents(html)
}

async function openPageEditor(pageId: string) {
  if (!manifest.value || !projectDir.value) return
  const page = manifest.value.pages.find(p => p.id === pageId)
  if (!page) return
  currentPageId.value = pageId
  mode.value = 'editor'
  try {
    const raw = await invoke<string>('open_file', { path: `${projectDir.value}/${page.file}` })
    const { html, css } = manifest.value.format === 'vue' ? parseVueSfc(raw) : parseHtmlDocument(raw)
    await mountEditor(html, css, manifest.value.format)
  } catch {
    await mountEditor('', '', manifest.value.format)
  }
}

function backToMap() {
  editor?.destroy()
  editor = null
  currentPageId.value = null
  mode.value = 'map'
}

async function savePage() {
  if (!editor || !projectDir.value || !manifest.value || !currentPageId.value) return
  const page = manifest.value.pages.find(p => p.id === currentPageId.value)
  if (!page) return
  const html = editor.getHtml()
  const css = editor.getCss() ?? ''
  const content = manifest.value.format === 'vue'
    ? wrapVueSfc(html, css)
    : wrapHtmlDocument(page.title, html, css, true)
  await invoke('save_file', { path: `${projectDir.value}/${page.file}`, content })
  await saveManifest()
  statusMsg.value = t('designSaved')
  setTimeout(() => { statusMsg.value = '' }, 3000)
}

async function exportCopy(format: DesignFormat) {
  if (!editor) return
  const html = editor.getHtml()
  const css = editor.getCss() ?? ''
  const path = await saveDialog({
    defaultPath: format === 'vue' ? 'DisenoComponente.vue' : 'diseno.html',
    filters: format === 'vue' ? [{ name: 'Vue SFC', extensions: ['vue'] }] : [{ name: 'HTML', extensions: ['html'] }],
  })
  if (!path) return
  const currentPage = manifest.value?.pages.find(p => p.id === currentPageId.value)
  const content = format === 'vue'
    ? wrapVueSfc(html, css)
    : wrapHtmlDocument(currentPage?.title ?? 'diseno', html, css, manifest.value?.format === 'html')
  await invoke('save_file', { path, content })
  statusMsg.value = `${t('designExported')} ${path}`
  setTimeout(() => { statusMsg.value = '' }, 4000)
}

// ── AI-assisted generation (reuses whatever provider/key is already
// configured in the AI Assistant panel — no separate settings screen here) ──
const AI_SYSTEM_HTML = 'Sos un generador de fragmentos HTML para un editor visual de sitios web (tipo Wix). Respondé ÚNICAMENTE con un fragmento de HTML válido — sin <html>, <head> ni <body>, sin explicaciones, sin bloques de código markdown (nada de ```). El proyecto ya carga DaisyUI y Tailwind, así que podés usar sus clases (btn, card, hero, navbar, alert, badge, input input-bordered, flex, grid, p-4, text-xl, bg-base-200, etc.). El resultado debe verse profesional y quedar listo para insertarse tal cual.'
const AI_SYSTEM_VUE = 'Sos un generador de fragmentos HTML para un editor visual de sitios web. Respondé ÚNICAMENTE con un fragmento de HTML válido — sin <html>, <head> ni <body>, sin explicaciones, sin bloques de código markdown (nada de ```). NO uses clases de Tailwind ni de ningún framework de CSS: el proyecto exporta a un componente Vue propio sin esas librerías, así que usá HTML semántico simple (section, h1, p, div, button, etc.) con estilos inline solo si hace falta.'

function extractHtml(reply: string): string {
  const match = reply.match(/```(?:html)?\n?([\s\S]*?)```/)
  return (match ? match[1] : reply).trim()
}

async function generateWithAi() {
  const prompt = aiPrompt.value.trim()
  if (!prompt || !editor || !manifest.value || aiLoading.value) return

  const provider = localStorage.getItem('ai_provider') || 'claude'
  const apiKey = provider === 'ollama' ? 'ollama' : (localStorage.getItem(`ai_key_${provider}`) ?? '')
  if (!apiKey) {
    aiError.value = t('designAiNoKey')
    return
  }
  const model = localStorage.getItem('ai_model') ?? 'claude-sonnet-4-6'

  aiError.value = ''
  aiLoading.value = true
  try {
    const system = manifest.value.format === 'html' ? AI_SYSTEM_HTML : AI_SYSTEM_VUE
    const reply = await invoke<string>('call_ai', {
      provider, apiKey, model, system,
      messages: [{ role: 'user', content: prompt }],
    })
    const html = extractHtml(reply)
    const selected = editor.getSelected()
    if (selected) selected.append(html)
    else editor.getWrapper()?.append(html)
    aiPrompt.value = ''
  } catch (e) {
    aiError.value = String(e)
  } finally {
    aiLoading.value = false
  }
}

onBeforeUnmount(() => {
  editor?.destroy()
  editor = null
})
</script>

<template>
  <div class="design-view">
    <!-- Start screen: new / open project -->
    <div v-if="mode === 'start'" class="design-start">
      <div class="design-start-card">
        <h3>{{ t('designNewProjectTitle') }}</h3>
        <input v-model="newName" class="design-input" :placeholder="t('designProjectNamePlaceholder')" />
        <div class="design-format-row">
          <label><input type="radio" value="html" v-model="newFormat" /> {{ t('designFormatHtml') }}</label>
          <label><input type="radio" value="vue" v-model="newFormat" /> {{ t('designFormatVue') }}</label>
        </div>
        <div class="design-dir-row">
          <input :value="newDir" class="design-input" readonly />
          <button class="design-btn" @click="pickNewDir">{{ t('designChooseFolder') }}</button>
        </div>
        <button class="design-btn design-btn--accent" :disabled="creating || !newName.trim() || !newDir" @click="createProject">
          {{ t('designCreateProject') }}
        </button>
        <div class="design-divider">—</div>
        <button class="design-btn" @click="openExistingProject">{{ t('designOpenExisting') }}</button>
        <p v-if="openingError" class="design-error">{{ openingError }}</p>
      </div>
    </div>

    <!-- Site map: n8n-style connectable page cards -->
    <template v-else-if="mode === 'map' && manifest">
      <div class="design-toolbar">
        <span class="design-title">{{ manifest.name }}</span>
        <div class="design-spacer" />
        <input v-model="newPageTitle" class="design-input" :placeholder="t('designNewPagePlaceholder')" @keydown.enter="addPage" />
        <button class="design-btn design-btn--accent" @click="addPage">{{ t('designAddPage') }}</button>
        <button class="design-btn" @click="saveManifest">{{ t('designSaveMap') }}</button>
      </div>
      <div class="design-map">
        <VueFlow
          :nodes="flowNodes"
          :edges="flowEdges"
          :fit-view-on-init="true"
          @connect="onConnect"
          @node-drag-stop="onNodeDragStop"
          @node-double-click="onNodeDoubleClick"
          @edges-change="onEdgesChange"
        >
          <Background />
          <Controls />
          <template #node-page="nodeProps">
            <div class="site-node" @dblclick="onNodeDoubleClick({ node: nodeProps as unknown as GraphNode })">
              <Handle type="target" :position="Position.Left" />
              <div class="site-node__head">
                <input
                  class="nodrag site-node__title" v-model="nodeProps.data.page.title"
                  @click.stop @blur="saveManifest"
                />
                <button class="nodrag site-node__del" :title="t('designDeletePage')" @click.stop="removePage(nodeProps.id)">×</button>
              </div>
              <textarea
                class="nodrag site-node__desc" v-model="nodeProps.data.page.description"
                :placeholder="t('designPageDescLabel')" @click.stop @blur="saveManifest"
              />
              <Handle type="source" :position="Position.Right" />
            </div>
          </template>
        </VueFlow>
      </div>
    </template>

    <!-- Page editor (GrapesJS canvas) -->
    <template v-else-if="mode === 'editor'">
      <div class="design-toolbar">
        <button class="design-btn" @click="backToMap">{{ t('designBackToMap') }}</button>
        <div class="design-spacer" />
        <span v-if="statusMsg" class="design-status">{{ statusMsg }}</span>
        <button class="design-btn design-btn--accent" @click="savePage">{{ t('designSave') }}</button>
        <button class="design-btn" @click="exportCopy('html')">{{ t('designExportHtml') }}</button>
        <button class="design-btn" @click="exportCopy('vue')">{{ t('designExportVue') }}</button>
      </div>
      <div class="design-ai-row">
        <span class="design-ai-icon">✦</span>
        <input
          v-model="aiPrompt" class="design-input design-ai-input" :placeholder="t('designAiPlaceholder')"
          :disabled="aiLoading" @keydown.enter="generateWithAi"
        />
        <button class="design-btn design-btn--accent" :disabled="aiLoading || !aiPrompt.trim()" @click="generateWithAi">
          {{ aiLoading ? t('designAiGenerating') : t('designAiGenerate') }}
        </button>
        <span v-if="aiError" class="design-ai-error">{{ aiError }}</span>
      </div>
      <div ref="canvasEl" class="design-canvas" />
    </template>
  </div>
</template>

<style scoped>
.design-view { display:flex; flex-direction:column; height:100%; background:var(--bg-darkest); overflow:hidden; }

.design-start { flex:1; display:flex; align-items:center; justify-content:center; }
.design-start-card {
  width:340px; display:flex; flex-direction:column; gap:10px; padding:24px;
  background:var(--bg-dark); border:1px solid var(--border); border-radius:8px;
}
.design-start-card h3 { margin:0 0 4px; font-size:14px; color:var(--fg); }
.design-format-row { display:flex; gap:16px; font-size:12px; color:var(--fg-muted); }
.design-dir-row { display:flex; gap:6px; }
.design-dir-row .design-input { flex:1; min-width:0; }
.design-divider { text-align:center; color:var(--fg-muted); font-size:11px; }
.design-error { color:#e55; font-size:11.5px; margin:0; }

.design-input {
  background:var(--bg-mid); border:1px solid var(--border); border-radius:5px;
  color:var(--fg); font-size:12px; font-family:var(--font-ui); padding:6px 9px; outline:none;
}
.design-input:focus { border-color:var(--accent); }

.design-toolbar {
  display:flex; align-items:center; gap:8px; padding:0 10px; height:38px; flex-shrink:0;
  background:var(--bg-dark); border-bottom:1px solid var(--border);
}
.design-title { font-size:12px; font-weight:600; color:var(--fg); white-space:nowrap; }
.design-spacer { flex:1; }
.design-status { font-size:11px; color:var(--fg-muted); font-family:var(--font-mono); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; max-width:30%; }

.design-ai-row {
  display:flex; align-items:center; gap:8px; padding:6px 10px; flex-shrink:0;
  background:var(--bg-mid); border-bottom:1px solid var(--border);
}
.design-ai-icon { color:var(--accent); font-size:13px; }
.design-ai-input { flex:1; min-width:0; }
.design-ai-error { font-size:11px; color:#e55; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; max-width:35%; }

.design-btn {
  background:none; border:1px solid var(--border); border-radius:5px; color:var(--fg-muted);
  font-size:11.5px; font-family:var(--font-ui); padding:5px 11px; cursor:pointer; white-space:nowrap;
}
.design-btn:hover { background:var(--bg-hover); color:var(--fg); }
.design-btn:disabled { opacity:0.5; cursor:default; }
.design-btn--accent { border-color:var(--accent); color:var(--accent); }
.design-btn--accent:hover { background:var(--accent); color:#fff; }

.design-canvas { flex:1; min-height:0; }
.design-canvas :deep(.gjs-editor) {
  height:100%;
  /* GrapesJS's skin variables — kept as a first layer, but the concrete
     .gjs-* rules below are what actually guarantee the repaint, since not
     every panel in this version reliably resolves through these 6 vars. */
  --gjs-primary-color: var(--bg-dark);
  --gjs-secondary-color: var(--fg-muted);
  --gjs-tertiary-color: var(--bg-mid);
  --gjs-quaternary-color: var(--accent);
  --gjs-font-color: var(--fg-muted);
  --gjs-font-color-active: var(--fg);
  --gjs-main-font: var(--font-ui);
  color: var(--fg-muted) !important;
}
.design-canvas :deep(.gjs-pn-panel),
.design-canvas :deep(.gjs-pn-views-container),
.design-canvas :deep(.gjs-block-category),
.design-canvas :deep(.gjs-layer-manager),
.design-canvas :deep(.gjs-sm-sectors),
.design-canvas :deep(.gjs-trt-traits),
.design-canvas :deep(.gjs-am-assets-cont) {
  background-color: var(--bg-dark) !important;
  color: var(--fg-muted) !important;
}
.design-canvas :deep(.gjs-block-category .gjs-title),
.design-canvas :deep(.gjs-sm-sector-title),
.design-canvas :deep(.gjs-trait-category .gjs-title),
.design-canvas :deep(.gjs-title) {
  background-color: var(--bg-mid) !important;
  color: var(--fg) !important;
}
.design-canvas :deep(.gjs-block) {
  background-color: var(--bg-mid) !important;
  color: var(--fg-muted) !important;
  border-color: var(--border) !important;
}
.design-canvas :deep(.gjs-block:hover) { color: var(--fg) !important; border-color: var(--accent) !important; }
.design-canvas :deep(.gjs-layer-item),
.design-canvas :deep(.gjs-clm-select),
.design-canvas :deep(.gjs-clm-tags-btn),
.design-canvas :deep(.gjs-field),
.design-canvas :deep(.gjs-sm-btn) {
  background-color: var(--bg-mid) !important;
  color: var(--fg-muted) !important;
}
.design-canvas :deep(.gjs-cv-canvas-bg) { background-color: var(--bg-darkest) !important; }
.design-canvas :deep(.gjs-pn-btn) { color: var(--fg-muted) !important; }
.design-canvas :deep(.gjs-pn-btn:hover) { color: var(--fg) !important; }
.design-canvas :deep(.gjs-pn-active) { color: var(--accent) !important; }

/* Blocks default to ~45% width / 90px min-height, which reads as oversized
   tiles — pack them tighter, 3 per row, so more fit without scrolling. */
.design-canvas :deep(.gjs-block) {
  width:30%; min-height:56px; padding:8px 4px; margin:4px 1.5%; font-size:10px;
}
.design-canvas :deep(.gjs-block-label) { font-size:10px; }
.design-canvas :deep(.gjs-block svg) { width:24px; height:24px; }

.design-map { flex:1; min-height:0; }
.design-map :deep(.vue-flow) { background:var(--bg-darkest); }

.site-node {
  width:220px; background:var(--bg-dark); border:1px solid var(--border); border-radius:8px;
  padding:10px; box-shadow:0 2px 10px rgba(0,0,0,.25);
}
.site-node__head { display:flex; align-items:center; gap:6px; margin-bottom:6px; }
.site-node__title {
  flex:1; min-width:0; background:none; border:none; color:var(--fg); font-size:12.5px; font-weight:600;
  font-family:var(--font-ui); padding:2px 4px; outline:none; border-radius:4px;
}
.site-node__title:focus { background:var(--bg-mid); }
.site-node__del {
  background:none; border:none; color:var(--fg-muted); cursor:pointer; font-size:15px; line-height:1;
  padding:2px 6px; border-radius:4px;
}
.site-node__del:hover { background:var(--red, #c0392b); color:#fff; }
.site-node__desc {
  width:100%; min-height:44px; resize:vertical; background:var(--bg-mid); border:1px solid var(--border);
  border-radius:5px; color:var(--fg-muted); font-size:11px; font-family:var(--font-ui); padding:5px 7px; outline:none;
}
.site-node__desc:focus { border-color:var(--accent); }
</style>
