// Quasar support for the visual designer. The GrapesJS canvas can't run Quasar's JS
// components, so Quasar blocks are drawn as plain HTML "look-alikes" tagged with
// data-q="q-btn" (+ data-qa-<prop> attributes). On save they are converted to real
// Quasar tags, and converted back when a page is opened — so the round trip is lossless
// for these components, and everything else (plain HTML, Quasar utility classes) passes through.

interface Spec {
  fake: string                       // tag used in the canvas
  label?: 'text' | 'attr'            // where the label lives: element text (default) or none
  container?: boolean                // keeps its children instead of using text as label
  model?: 'string' | 'bool' | 'number'   // gets a v-model bound to `form`
  boolAttrs?: string[]
}
const SPECS: Record<string, Spec> = {
  'q-btn': { fake: 'button', label: 'text', boolAttrs: ['flat', 'outline', 'rounded', 'unelevated', 'no-caps', 'round', 'dense'] },
  'q-input': { fake: 'div', label: 'text', model: 'string', boolAttrs: ['outlined', 'filled', 'dense', 'clearable'] },
  'q-select': { fake: 'div', label: 'text', model: 'string', boolAttrs: ['outlined', 'filled', 'dense'] },
  'q-checkbox': { fake: 'label', label: 'text', model: 'bool' },
  'q-toggle': { fake: 'label', label: 'text', model: 'bool' },
  'q-slider': { fake: 'div', label: 'text', model: 'number', boolAttrs: ['label'] },
  'q-badge': { fake: 'span', label: 'text' },
  'q-chip': { fake: 'span', label: 'text', boolAttrs: ['outline', 'removable'] },
  'q-card': { fake: 'div', container: true, boolAttrs: ['flat', 'bordered'] },
  'q-card-section': { fake: 'div', container: true },
  'q-toolbar': { fake: 'div', container: true },
  'q-toolbar-title': { fake: 'div', label: 'text' },
  'q-separator': { fake: 'hr' },
  'q-linear-progress': { fake: 'div' },
}

// The HTML parser lowercases tag/attribute names and the DOM refuses names like `@click`; Vue needs both exact.
// Before parsing, such names are renamed to inert valid ones (data-owee-enc-<escaped>, data-owee-case); after serializing
// they are put back as plain text, so nothing depends on what the DOM would do to them.
const encName = (n: string) => 'data-owee-enc-' + n.replace(/[^a-z0-9.-]/g, c => `_${c.charCodeAt(0).toString(16)}_`)
const decName = (e: string) => e.replace(/_([0-9a-f]+)_/g, (_m, h) => String.fromCharCode(parseInt(h, 16)))
const SIMPLE_ATTR = /^[a-z_:][a-z0-9_.:-]*$/

function protectNames(template: string): string {
  return template.replace(/<([A-Za-z][\w.-]*)((?:"[^"]*"|'[^']*'|[^>"'])*)>/g, (_m, tag: string, attrs: string) => {
    const fixed = attrs.replace(/("[^"]*"|'[^']*')|([^\s="'<>\/]+)/g, (m2, quoted: string | undefined, name: string | undefined) =>
      quoted !== undefined ? quoted : SIMPLE_ATTR.test(name!) ? m2 : encName(name!))
    return `<${tag}${/[A-Z]/.test(tag) ? ` data-owee-case="${tag}"` : ''}${fixed}>`
  })
}

function restoreNames(out: string): string {
  // attributes first (they may live on tags whose case is restored next)
  out = out.replace(/(\s)data-owee-enc-([a-z0-9_.-]+)(?==|\s|\/|>)/g, (_m, sp: string, e: string) => sp + decName(e))
  const names = new Map<string, string>()
  out = out.replace(/ data-owee-case="([^"]+)"/g, (_m, orig: string) => { names.set(orig.toLowerCase(), orig); return '' })
  for (const [lower, orig] of names) out = out.replace(new RegExp(`<(/?)${lower.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?=[\\s/>])`, 'g'), `<$1${orig}`)
  return out
}

const slug = (s: string) => (s.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_+|_+$/g, '') || 'campo')

// GrapesJS turns <video>/<audio>/<iframe>… into media components and adds attributes (controls, allowfullscreen…).
// Carry them as inert boxes so the file keeps exactly what it had.
const MEDIA_TAGS = 'video,audio,iframe,object,embed'
function boxMedia(root: Element) {
  for (const el of Array.from(root.querySelectorAll(MEDIA_TAGS))) {
    const box = el.ownerDocument.createElement('div')
    for (const a of Array.from(el.attributes)) box.setAttribute(a.name, a.value)
    box.setAttribute('data-owee-tag', el.tagName.toLowerCase())
    while (el.firstChild) box.appendChild(el.firstChild)
    el.replaceWith(box)
  }
}
function unboxMedia(root: Element) {
  for (const box of Array.from(root.querySelectorAll('[data-owee-tag]'))) {
    const el = box.ownerDocument.createElement(box.getAttribute('data-owee-tag')!)
    for (const a of Array.from(box.attributes)) if (a.name !== 'data-owee-tag') el.setAttribute(a.name, a.value)
    while (box.firstChild) el.appendChild(box.firstChild)
    box.replaceWith(el)
  }
}

// ── canvas → Quasar template ──
export function htmlToQuasar(html: string): { template: string; form: Record<string, string> } {
  const doc = new DOMParser().parseFromString(`<body>${html}</body>`, 'text/html')
  unboxMedia(doc.body)
  // GrapesJS gives every <img> without `src` a placeholder picture; a dynamic image (`:src="x"`) must not keep it
  for (const img of Array.from(doc.body.querySelectorAll('img'))) {
    if ((img.hasAttribute(':src') || img.hasAttribute('v-bind:src')) && (img.getAttribute('src') ?? '').startsWith('data:image/svg+xml;base64,PHN2ZyB4bWxucz0i')) img.removeAttribute('src')
  }
  const form: Record<string, string> = {}
  const used = new Set<string>()
  const convert = (el: Element) => {
    const tag = el.getAttribute('data-q')!
    const spec = SPECS[tag]
    if (!spec) { el.removeAttribute('data-q'); return }
    const q = doc.createElement(tag)
    for (const a of Array.from(el.attributes)) {
      if (a.name === 'data-qa-x-options') q.setAttribute(':options', a.value)
      else if (a.name.startsWith('data-qa-')) q.setAttribute(a.name.slice(8), a.value)
      else if (a.name === 'class' || a.name === 'style' || a.name === 'id') {
        const v = a.value.replace(/\bqf-[\w-]+\b/g, '').replace(/\s+/g, ' ').trim()
        if (v) q.setAttribute(a.name, v)
      }
    }
    const text = (el.textContent ?? '').trim()
    if (spec.container) while (el.firstChild) q.appendChild(el.firstChild)
    else if (spec.label === 'text' && text) {
      if (tag === 'q-badge' || tag === 'q-chip' || tag === 'q-toolbar-title') q.textContent = text
      else q.setAttribute('label', text)
    }
    const bound = Array.from(q.attributes).some(a => a.name === 'v-model' || a.name.startsWith('v-model.') || a.name.startsWith('v-model:') || a.name === ':model-value' || a.name === 'model-value')
    if (spec.model && bound) {
      // already bound (to our own `form.x` or to the file's own variable): keep it, and keep `form` declared if it is ours
      const own = (q.getAttribute('v-model') ?? '').match(/^form\.(\w+)$/)
      if (own) { form[own[1]] = spec.model === 'bool' ? 'false' : spec.model === 'number' ? '0' : "''"; used.add(own[1]) }
    } else if (spec.model) {
      let name = slug(text || tag), n = 2
      while (used.has(name)) name = `${slug(text || tag)}_${n++}`
      used.add(name)
      form[name] = spec.model === 'bool' ? 'false' : spec.model === 'number' ? '0' : "''"
      q.setAttribute('v-model', `form.${name}`)
    }
    if (tag === 'q-select') {
      if (!q.hasAttribute(':options')) {
        const opts = (el.getAttribute('data-qa-options') ?? '').split(',').map(x => x.trim()).filter(Boolean)
        q.setAttribute(':options', `[${opts.map(x => `'${x.replace(/'/g, "\\'")}'`).join(', ')}]`)
      }
      q.removeAttribute('options')
    }
    if (tag === 'q-slider') for (const k of ['min', 'max']) if (q.hasAttribute(k)) { q.setAttribute(`:${k}`, q.getAttribute(k)!); q.removeAttribute(k) }
    if (tag === 'q-linear-progress') { const v = q.getAttribute('value'); q.removeAttribute('value'); q.setAttribute(':value', v ?? '0.5') }
    el.replaceWith(q)
  }
  // innermost first, so containers keep the children that were already converted
  for (let guard = 0; guard < 500; guard++) {
    const el = Array.from(doc.body.querySelectorAll('[data-q]')).find(e => !e.querySelector('[data-q]'))
    if (!el) break
    convert(el)
  }
  return { template: serialize(doc.body), form }
}

// Self-close empty Quasar elements and keep boolean attributes bare (`flat`, not `flat=""`)
function serialize(body: HTMLElement): string {
  let out = restoreNames(body.innerHTML)
  out = out.replace(/ ([^\s="'<>\/]+)=""/g, ' $1')   // also @events and #slots, not only plain names
  // the HTML serializer escapes > < & inside attribute values; Vue expressions read better (and diff cleaner) as written
  out = out.replace(/(\s[:@#\w.\-]+=")([^"]*)"/g, (_m, a, v) => `${a}${v.replace(/&gt;/g, '>').replace(/&lt;/g, '<').replace(/&amp;/g, '&')}"`)
  out = out.replace(/<(q-(?:separator|linear-progress|input|select|slider|checkbox|toggle|btn))([^>]*)><\/\1>/g, '<$1$2 />')
  return out
}

// ── Quasar template → canvas ──
export function quasarToHtml(template: string): string {
  // Vue allows `<div />` and `<q-btn />`, the HTML parser does not (it would swallow the next siblings): expand them
  const VOID = new Set(['area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta', 'source', 'track', 'wbr'])
  const expanded = template.replace(/<([A-Za-z][\w.-]*)((?:"[^"]*"|'[^']*'|[^>"'])*?)\s*\/>/g, (_m, tag: string, attrs: string) => (VOID.has(tag.toLowerCase()) ? `<${tag}${attrs}>` : `<${tag}${attrs}></${tag}>`))
  const doc = new DOMParser().parseFromString(`<body>${protectNames(expanded)}</body>`, 'text/html')
  let guard = 0
  while (guard++ < 500) {
    const el = Array.from(doc.body.querySelectorAll('*')).find(e => SPECS[e.tagName.toLowerCase()] && (SPECS[e.tagName.toLowerCase()].container || e.children.length === 0))
    if (!el) break
    const tag = el.tagName.toLowerCase()
    const spec = SPECS[tag]
    const f = doc.createElement(spec.fake)
    f.setAttribute('data-q', tag)
    f.setAttribute('class', `qf-${tag.slice(2)}`)
    for (const a of Array.from(el.attributes)) {
      if ((tag === 'q-select' && a.name === ':options') || (tag === 'q-slider' && (a.name === ':min' || a.name === ':max')) || (tag === 'q-linear-progress' && a.name === ':value')) continue   // rebuilt below
      if (a.name === 'label' && spec.label === 'text') continue
      if (a.name === 'class') f.setAttribute('class', `${f.getAttribute('class')} ${a.value}`)
      else if (a.name === 'style' || a.name === 'id') f.setAttribute(a.name, a.value)
      else f.setAttribute(`data-qa-${a.name}`, a.value)
    }
    if (tag === 'q-select') {
      const expr = (el.getAttribute(':options') ?? '').trim()
      const isLiteral = /^\[\s*(?:'(?:[^'\\]|\\.)*'\s*(?:,\s*)?)*\]$/.test(expr)
      if (isLiteral) { const m = expr.match(/'((?:[^'\\]|\\.)*)'/g); if (m) f.setAttribute('data-qa-options', m.map(x => x.slice(1, -1).replace(/\\'/g, "'")).join(',')) }
      else if (expr) f.setAttribute('data-qa-x-options', expr)   // e.g. :options="cities" -> keep the expression as is
    }
    if (tag === 'q-slider') { f.setAttribute('data-qa-min', el.getAttribute(':min') ?? '0'); f.setAttribute('data-qa-max', el.getAttribute(':max') ?? '100') }
    if (tag === 'q-linear-progress') f.setAttribute('data-qa-value', el.getAttribute(':value') ?? '0.5')
    if (spec.container) while (el.firstChild) f.appendChild(el.firstChild)
    else if (spec.label === 'text') f.textContent = el.getAttribute('label') ?? el.textContent ?? ''
    el.replaceWith(f)
  }
  boxMedia(doc.body)
  return doc.body.innerHTML
}

export function buildQuasarSfc(html: string, css: string): string {
  const { template, form } = htmlToQuasar(html)
  css = css.replace(/\*\s*\{\s*box-sizing:\s*border-box;\s*\}\s*body\s*\{\s*margin:\s*0;\s*\}/g, '').trim()   // GrapesJS defaults: Quasar already resets these
  const fields = Object.entries(form)
  const script = fields.length
    ? `<script setup lang="ts">\nimport { reactive } from 'vue'\n\nconst form = reactive({\n${fields.map(([k, v]) => `  ${k}: ${v},`).join('\n')}\n})\n<\/script>\n\n`
    : `<script setup lang="ts">\n<\/script>\n\n`
  return `${script}<template>\n  <q-page padding>\n${template}\n  </q-page>\n</template>\n\n<style scoped>\n${css}\n</style>\n`
}

export function parseQuasarSfc(file: string): { html: string; css: string } {
  const t = file.match(/<template>([\s\S]*)<\/template>/)?.[1] ?? ''
  const inner = t.replace(/^\s*<q-page[^>]*>/, '').replace(/<\/q-page>\s*$/, '')
  return { html: quasarToHtml(inner.trim()), css: file.match(/<style scoped>([\s\S]*?)<\/style>/)?.[1]?.trim() ?? '' }
}

// Look-alike styles, injected into the canvas only (never exported): Quasar utility classes + the fake components
export const QUASAR_CANVAS_CSS = `
body{font-family:Roboto,system-ui,sans-serif;color:#1d1d1d;margin:0;padding:16px}
.row{display:flex;flex-wrap:wrap}.col{flex:1 1 0}.column{display:flex;flex-direction:column}.items-center{align-items:center}.justify-center{justify-content:center}.justify-between{justify-content:space-between}
.col-1{width:8.33%}.col-2{width:16.66%}.col-3{width:25%}.col-4{width:33.33%}.col-5{width:41.66%}.col-6{width:50%}.col-7{width:58.33%}.col-8{width:66.66%}.col-9{width:75%}.col-10{width:83.33%}.col-11{width:91.66%}.col-12{width:100%}
[class*="col-md-"],[class*="col-sm-"]{width:100%}
.q-pa-xs{padding:4px}.q-pa-sm{padding:8px}.q-pa-md{padding:16px}.q-pa-lg{padding:24px}.q-ma-md{margin:16px}.q-mb-md{margin-bottom:16px}.q-mt-md{margin-top:16px}.q-gutter-md>*{margin:8px}
.text-h4{font-size:2.125rem;font-weight:400}.text-h5{font-size:1.5rem;font-weight:400}.text-h6{font-size:1.25rem;font-weight:500}.text-subtitle1{font-size:1rem}.text-caption{font-size:.75rem;opacity:.7}.text-center{text-align:center}.text-bold{font-weight:700}
.bg-primary{background:#1976d2;color:#fff}.bg-secondary{background:#26a69a;color:#fff}.bg-grey-2{background:#eee}.text-primary{color:#1976d2}.text-negative{color:#c10015}.text-positive{color:#21ba45}
[data-q]{box-sizing:border-box}
[data-owee-tag]{display:block;min-height:36px;border:1px dashed #9aa;background:#eef3f8;color:#667}[data-owee-tag]::before{content:'<' attr(data-owee-tag) '>';font:11px monospace;padding:2px 6px}
.qf-btn{background:#1976d2;color:#fff;border:0;border-radius:4px;padding:8px 16px;font:500 14px Roboto,sans-serif;text-transform:uppercase;letter-spacing:.05em;cursor:pointer;box-shadow:0 1px 5px rgba(0,0,0,.2)}
.qf-btn[data-qa-color=secondary]{background:#26a69a}.qf-btn[data-qa-color=negative]{background:#c10015}.qf-btn[data-qa-color=positive]{background:#21ba45}.qf-btn[data-qa-color=warning]{background:#f2c037;color:#1d1d1d}.qf-btn[data-qa-color=dark]{background:#1d1d1d}
.qf-btn[data-qa-flat],.qf-btn[data-qa-outline]{background:transparent;color:#1976d2;box-shadow:none}.qf-btn[data-qa-outline]{border:1px solid currentColor}.qf-btn[data-qa-rounded]{border-radius:28px}.qf-btn[data-qa-no-caps]{text-transform:none}
.qf-input,.qf-select{display:block;min-width:220px;padding:14px 12px 6px;border-bottom:1px solid rgba(0,0,0,.4);background:rgba(0,0,0,.04);border-radius:4px 4px 0 0;font-size:14px;color:rgba(0,0,0,.6);position:relative}
.qf-input[data-qa-outlined],.qf-select[data-qa-outlined]{background:transparent;border:1px solid rgba(0,0,0,.24);border-radius:4px}
.qf-select::after{content:'▾';position:absolute;right:10px;top:12px}
.qf-checkbox::before,.qf-toggle::before{content:'';display:inline-block;width:16px;height:16px;border:2px solid #1976d2;border-radius:2px;margin-right:8px;vertical-align:-3px}
.qf-toggle::before{width:30px;height:12px;border-radius:12px;background:#1976d233}
.qf-checkbox,.qf-toggle{display:inline-block;font-size:14px;margin:4px 12px 4px 0}
.qf-slider{height:4px;background:#1976d255;border-radius:2px;margin:16px 0;min-width:200px;position:relative;color:transparent}.qf-slider::after{content:'';position:absolute;left:40%;top:-5px;width:14px;height:14px;border-radius:50%;background:#1976d2}
.qf-badge{display:inline-block;background:#1976d2;color:#fff;font-size:12px;padding:2px 6px;border-radius:4px}
.qf-chip{display:inline-block;background:#e0e0e0;border-radius:16px;padding:4px 12px;font-size:13px;margin:2px}
.qf-card{display:block;background:#fff;border-radius:4px;box-shadow:0 1px 5px rgba(0,0,0,.2),0 2px 2px rgba(0,0,0,.14);max-width:420px}.qf-card[data-qa-flat]{box-shadow:none}.qf-card[data-qa-bordered]{border:1px solid rgba(0,0,0,.12);box-shadow:none}
.qf-card-section{display:block;padding:16px}
.qf-toolbar{display:flex;align-items:center;min-height:50px;padding:0 12px;background:#1976d2;color:#fff}.qf-toolbar-title{font-size:21px;font-weight:500}
.qf-separator{border:0;border-top:1px solid rgba(0,0,0,.12);margin:8px 0}
.qf-linear-progress{height:4px;background:#1976d233;position:relative;min-width:200px;color:transparent}.qf-linear-progress::after{content:'';position:absolute;left:0;top:0;bottom:0;width:50%;background:#1976d2}
`

export const QUASAR_BLOCKS: { id: string; label: string; category: string; content: string }[] = [
  { id: 'q-btn', label: 'Botón (q-btn)', category: 'Quasar · Formularios', content: '<button data-q="q-btn" class="qf-btn" data-qa-color="primary">Guardar</button>' },
  { id: 'q-btn-outline', label: 'Botón borde (outline)', category: 'Quasar · Formularios', content: '<button data-q="q-btn" class="qf-btn" data-qa-outline="" data-qa-color="primary">Cancelar</button>' },
  { id: 'q-input', label: 'Campo de texto (q-input)', category: 'Quasar · Formularios', content: '<div data-q="q-input" class="qf-input" data-qa-outlined="">Nombre</div>' },
  { id: 'q-select', label: 'Selector (q-select)', category: 'Quasar · Formularios', content: '<div data-q="q-select" class="qf-select" data-qa-outlined="" data-qa-options="La Paz,Santa Cruz,Cochabamba">Ciudad</div>' },
  { id: 'q-checkbox', label: 'Casilla (q-checkbox)', category: 'Quasar · Formularios', content: '<label data-q="q-checkbox" class="qf-checkbox">Acepto los términos</label>' },
  { id: 'q-toggle', label: 'Interruptor (q-toggle)', category: 'Quasar · Formularios', content: '<label data-q="q-toggle" class="qf-toggle">Notificaciones</label>' },
  { id: 'q-slider', label: 'Deslizador (q-slider)', category: 'Quasar · Formularios', content: '<div data-q="q-slider" class="qf-slider" data-qa-min="0" data-qa-max="100">Volumen</div>' },
  { id: 'q-card', label: 'Tarjeta (q-card)', category: 'Quasar · Contenedores', content: '<div data-q="q-card" class="qf-card"><div data-q="q-card-section" class="qf-card-section"><div class="text-h6">Título</div><div class="text-subtitle1">Contenido de la tarjeta</div></div></div>' },
  { id: 'q-toolbar', label: 'Barra (q-toolbar)', category: 'Quasar · Contenedores', content: '<div data-q="q-toolbar" class="qf-toolbar"><div data-q="q-toolbar-title" class="qf-toolbar-title">Mi aplicación</div></div>' },
  { id: 'q-row', label: 'Fila con 2 columnas', category: 'Quasar · Contenedores', content: '<div class="row q-col-gutter-md"><div class="col-12 col-md-6 q-pa-md">Columna 1</div><div class="col-12 col-md-6 q-pa-md">Columna 2</div></div>' },
  { id: 'q-badge', label: 'Insignia (q-badge)', category: 'Quasar · Datos', content: '<span data-q="q-badge" class="qf-badge" data-qa-color="primary">Nuevo</span>' },
  { id: 'q-chip', label: 'Chip (q-chip)', category: 'Quasar · Datos', content: '<span data-q="q-chip" class="qf-chip">Etiqueta</span>' },
  { id: 'q-progress', label: 'Progreso (q-linear-progress)', category: 'Quasar · Datos', content: '<div data-q="q-linear-progress" class="qf-linear-progress" data-qa-value="0.5"></div>' },
  { id: 'q-separator', label: 'Separador (q-separator)', category: 'Quasar · Datos', content: '<hr data-q="q-separator" class="qf-separator">' },
]

// ── whole-file helpers: an existing page's root <q-page> (or any single root) is kept aside so the canvas only holds its content ──
export function templateToCanvas(inner: string): { html: string; wrapperOpen: string | null; wrapperTag: string | null } {
  const t = inner.trim()
  const m = t.match(/^<(q-page|q-layout|q-page-container)\b([^>]*)>/)
  if (m && t.endsWith(`</${m[1]}>`)) return { html: quasarToHtml(t.slice(m[0].length, t.length - m[1].length - 3).trim()), wrapperOpen: m[0], wrapperTag: m[1] }
  return { html: quasarToHtml(t), wrapperOpen: null, wrapperTag: null }
}
export function canvasToTemplate(html: string, wrapperOpen: string | null, wrapperTag: string | null): { template: string; form: Record<string, string> } {
  const { template, form } = htmlToQuasar(html)
  return { template: wrapperOpen ? `  ${wrapperOpen}\n${template}\n  </${wrapperTag}>` : template, form }
}
