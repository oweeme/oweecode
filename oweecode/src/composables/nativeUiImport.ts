// Import existing Qt Designer (.ui) and GTK (GtkBuilder .ui) files into the native designer's model.
// Common widgets are mapped; layouts the model doesn't have (grid, form, tabs…) are flattened and reported.
import { newNode, type UiNode, type WType } from './nativeUi'

export interface ImportResult { root: UiNode; target: 'qtwidgets' | 'gtk4' | 'qml'; notes: string[] }

const kids = (el: Element, tag: string) => Array.from(el.children).filter(c => c.tagName === tag)
const txt = (el: Element | null | undefined) => el?.textContent ?? ''

// ── Qt Designer ──
function qtProp(w: Element, name: string): Element | null {
  return kids(w, 'property').find(p => p.getAttribute('name') === name) ?? null
}
const qtStr = (w: Element, name: string) => txt(qtProp(w, name)?.querySelector(':scope > string'))
const qtNum = (w: Element, name: string, d = 0) => { const n = Number(txt(qtProp(w, name)?.querySelector(':scope > number'))); return Number.isFinite(n) && qtProp(w, name) ? n : d }

export function importQtUi(xml: string): ImportResult {
  const doc = new DOMParser().parseFromString(xml, 'application/xml')
  if (doc.querySelector('parsererror')) throw new Error('El archivo .ui no es un XML válido')
  const rootW = doc.querySelector('ui > widget')
  if (!rootW) throw new Error('No encontré el widget raíz en el .ui')
  const notes = new Set<string>()
  const win = newNode('window', { text: qtStr(rootW, 'windowTitle') || 'Ventana', name: rootW.getAttribute('name') === 'MainWindow' ? '' : '' })
  const geo = qtProp(rootW, 'geometry')
  if (geo) { win.width = Number(txt(geo.querySelector('width'))) || 640; win.height = Number(txt(geo.querySelector('height'))) || 480 }

  const widgetNode = (w: Element): UiNode[] => {
    const cls = w.getAttribute('class') ?? '', name = w.getAttribute('name') ?? ''
    const base = { name }
    const items = (lw: Element) => kids(lw, 'item').map(i => txt(qtProp(i, 'text')?.querySelector('string'))).filter(Boolean).join('\n')
    switch (cls) {
      case 'QPushButton': case 'QToolButton': return [newNode('button', { ...base, text: qtStr(w, 'text') })]
      case 'QLabel': return [newNode('label', { ...base, text: qtStr(w, 'text') })]
      case 'QLineEdit': return [newNode('lineedit', { ...base, placeholder: qtStr(w, 'placeholderText') })]
      case 'QTextEdit': case 'QPlainTextEdit': case 'QTextBrowser': return [newNode('textedit', { ...base, placeholder: qtStr(w, 'placeholderText') })]
      case 'QCheckBox': case 'QRadioButton': return [newNode('checkbox', { ...base, text: qtStr(w, 'text'), value: txt(qtProp(w, 'checked')?.querySelector('bool')) === 'true' ? 1 : 0 })]
      case 'QComboBox': case 'QFontComboBox': return [newNode('combobox', { ...base, items: items(w) })]
      case 'QSlider': case 'QDial': case 'QScrollBar': return [newNode('slider', { ...base, min: qtNum(w, 'minimum', 0), max: qtNum(w, 'maximum', 99), value: qtNum(w, 'value', 0) })]
      case 'QProgressBar': return [newNode('progress', { ...base, min: qtNum(w, 'minimum', 0), max: qtNum(w, 'maximum', 100), value: qtNum(w, 'value', 0) })]
      case 'QSpinBox': case 'QDoubleSpinBox': return [newNode('spinbox', { ...base, min: qtNum(w, 'minimum', 0), max: qtNum(w, 'maximum', 99), value: qtNum(w, 'value', 0) })]
      case 'QListWidget': case 'QListView': case 'QTreeWidget': case 'QTableWidget': case 'QTableView':
        if (cls !== 'QListWidget' && cls !== 'QListView') notes.add(`${cls} se muestra como lista simple`)
        return [newNode('listbox', { ...base, items: items(w) })]
      case 'Line': return [newNode('separator', base)]
      case 'QFrame': {
        const shape = txt(qtProp(w, 'frameShape')?.querySelector('enum'))
        if (/Line$/.test(shape)) return [newNode('separator', base)]
        return containerFrom(w, base, 'vbox')
      }
      case 'QGroupBox': return containerFrom(w, { ...base, text: qtStr(w, 'title') }, 'frame')
      case 'QWidget': case 'QMainWindow': case 'QScrollArea': case 'QStackedWidget': case 'QDialog': case 'QSplitter': case 'QTabWidget': case 'QToolBox': {
        if (cls === 'QTabWidget' || cls === 'QStackedWidget' || cls === 'QToolBox') notes.add(`${cls}: las páginas se apilan una debajo de otra`)
        return containerFrom(w, base, 'vbox')
      }
      case 'QMenuBar': case 'QStatusBar': case 'QToolBar': case 'QDockWidget': notes.add(`${cls} no se incluye en el diseño (se conserva solo el contenido)`); return []
      default: notes.add(`Widget no soportado: ${cls} (se muestra como etiqueta)`); return [newNode('label', { ...base, text: `[${cls}] ${name}` })]
    }
  }
  const containerFrom = (w: Element, props: Partial<UiNode>, type: WType): UiNode[] => {
    const box = newNode(type, props)
    const lay = kids(w, 'layout')[0]
    if (lay) box.children = layoutKids(lay)
    else box.children = kids(w, 'widget').flatMap(widgetNode)   // absolute positioning: kept in file order
    // a plain wrapper widget with a single layout inside is just that layout
    if (type === 'vbox' && !props.name && box.children.length === 1 && (box.children[0].type === 'vbox' || box.children[0].type === 'hbox') && lay) return [box.children[0]]
    return [box]
  }
  const layoutKids = (lay: Element): UiNode[] => {
    const cls = lay.getAttribute('class') ?? ''
    if (cls === 'QGridLayout' || cls === 'QFormLayout') notes.add(`${cls}: se convirtió en una caja vertical (fila por fila)`)
    const items = kids(lay, 'item')
    const sorted = cls === 'QGridLayout' ? items.slice().sort((a, b) => (Number(a.getAttribute('row')) - Number(b.getAttribute('row'))) || (Number(a.getAttribute('column')) - Number(b.getAttribute('column')))) : items
    return sorted.flatMap(it => {
      const w = kids(it, 'widget')[0], l = kids(it, 'layout')[0], sp = kids(it, 'spacer')[0]
      if (w) return widgetNode(w)
      if (sp) return [newNode('spacer', { name: sp.getAttribute('name') ?? '' })]
      if (l) {
        const lc = l.getAttribute('class') ?? ''
        const box = newNode(lc === 'QHBoxLayout' ? 'hbox' : 'vbox', { spacing: qtNum(l, 'spacing', 8) })
        box.children = layoutKids(l)
        return [box]
      }
      return []
    })
  }
  const rootLayout = kids(rootW, 'layout')[0]
  if (rootLayout) { win.children = layoutKids(rootLayout); win.spacing = qtNum(rootLayout, 'spacing', 8); win.margin = qtNum(rootLayout, 'leftMargin', 12) }
  else win.children = kids(rootW, 'widget').flatMap(widgetNode)
  // QMainWindow > centralwidget > layout: the central widget is just the window's content
  while (win.children.length === 1 && win.children[0].type === 'vbox' && (win.children[0].name === 'centralwidget' || !win.children[0].name)) win.children = win.children[0].children
  return { root: win, target: 'qtwidgets', notes: [...notes] }
}

// ── GTK (GtkBuilder) ──
const gProp = (o: Element, name: string) => kids(o, 'property').find(p => p.getAttribute('name') === name || p.getAttribute('name') === name.replace(/-/g, '_'))
const gStr = (o: Element, name: string) => txt(gProp(o, name))
const gNum = (o: Element, name: string, d = 0) => { const p = gProp(o, name); const n = Number(txt(p)); return p && Number.isFinite(n) ? n : d }
const gHandler = (o: Element) => kids(o, 'signal')[0]?.getAttribute('handler') ?? ''

export function importGtkUi(xml: string): ImportResult {
  const doc = new DOMParser().parseFromString(xml, 'application/xml')
  if (doc.querySelector('parsererror')) throw new Error('El archivo .ui no es un XML válido')
  const rootObj = doc.querySelector('interface > template') ?? doc.querySelector('interface > object')
  if (!rootObj) throw new Error('No encontré la ventana en el .ui de GTK')
  const notes = new Set<string>()
  const win = newNode('window', { text: gStr(rootObj, 'title') || 'Ventana', width: gNum(rootObj, 'default-width', 640), height: gNum(rootObj, 'default-height', 480) })

  const childObjs = (o: Element) => kids(o, 'child').flatMap(c => kids(c, 'object'))
  const convert = (o: Element): UiNode[] => {
    const cls = o.getAttribute('class') ?? '', id = o.getAttribute('id') ?? '', h = gHandler(o)
    const base = { name: id, handler: h }
    const expand = gStr(o, 'hexpand') === 'True' || gStr(o, 'vexpand') === 'True'
    switch (cls) {
      case 'GtkBox': case 'GtkCenterBox': case 'AdwClamp': case 'GtkViewport': case 'GtkOverlay': case 'GtkRevealer': case 'AdwBin': {
        const horiz = gStr(o, 'orientation') === 'horizontal'
        const box = newNode(horiz ? 'hbox' : 'vbox', { name: id, spacing: gNum(o, 'spacing', 8), expand })
        box.children = childObjs(o).flatMap(convert)
        // a box with nothing but `expand` and no children is a spacer
        if (!box.children.length && expand) return [newNode('spacer', { name: id })]
        return [box]
      }
      case 'GtkGrid': case 'GtkStack': case 'GtkNotebook': case 'GtkPaned': { notes.add(`${cls}: se convirtió en una caja vertical`); const b = newNode('vbox', { name: id }); b.children = childObjs(o).flatMap(convert); return [b] }
      case 'GtkFrame': {
        const b = newNode('frame', { name: id, text: gStr(o, 'label') }); b.children = childObjs(o).flatMap(convert)
        if (b.children.length === 1 && b.children[0].type === 'vbox') b.children = b.children[0].children   // the frame's own inner box
        return [b]
      }
      case 'GtkScrolledWindow': {
        const inner = childObjs(o).flatMap(convert)
        if (inner.length === 1 && inner[0].type === 'textedit') return [{ ...inner[0], name: id || inner[0].name }]
        if (inner.length === 1 && inner[0].type === 'listbox') return inner
        const b = newNode('vbox', { name: id, expand: true }); b.children = inner; return [b]
      }
      case 'GtkLabel': return [newNode('label', { ...base, text: gStr(o, 'label') })]
      case 'GtkButton': case 'GtkToggleButton': case 'GtkLinkButton': case 'GtkMenuButton': return [newNode('button', { ...base, text: gStr(o, 'label') || gStr(o, 'icon-name'), expand })]
      case 'GtkEntry': case 'GtkSearchEntry': case 'GtkPasswordEntry': return [newNode('lineedit', { ...base, placeholder: gStr(o, 'placeholder-text') })]
      case 'GtkTextView': return [newNode('textedit', { ...base })]
      case 'GtkCheckButton': case 'GtkSwitch': return [newNode('checkbox', { ...base, text: gStr(o, 'label'), value: gStr(o, 'active') === 'True' ? 1 : 0 })]
      case 'GtkDropDown': case 'GtkComboBoxText': {
        const items = Array.from(o.querySelectorAll('GtkStringList item, items item')).map(i => txt(i)).join('\n')
        return [newNode('combobox', { ...base, items })]
      }
      case 'GtkScale': case 'GtkSpinButton': {
        const adj = o.querySelector('object[class="GtkAdjustment"]')
        const min = adj ? gNum(adj, 'lower', 0) : 0, max = adj ? gNum(adj, 'upper', 100) : 100, value = adj ? gNum(adj, 'value', 0) : 0
        return [newNode(cls === 'GtkScale' ? 'slider' : 'spinbox', { ...base, min, max, value })]
      }
      case 'GtkProgressBar': return [newNode('progress', { name: id, min: 0, max: 100, value: Math.round(gNum(o, 'fraction', 0) * 100) })]
      case 'GtkListBox': return [newNode('listbox', { ...base, items: Array.from(o.querySelectorAll('child > object[class="GtkLabel"] > property[name="label"]')).map(p => txt(p)).join('\n'), expand: true })]
      case 'GtkImage': case 'GtkPicture': return [newNode('image', { name: id, text: gStr(o, 'file') })]
      case 'GtkSeparator': return [newNode('separator', { name: id })]
      default: notes.add(`Widget no soportado: ${cls} (se muestra como etiqueta)`); return [newNode('label', { name: id, text: `[${cls}] ${id}` })]
    }
  }
  const content = childObjs(rootObj).flatMap(convert)
  // the usual "window > one vertical box" wrapper is the window's own layout
  if (content.length === 1 && content[0].type === 'vbox') { win.children = content[0].children; win.spacing = content[0].spacing; win.margin = gNum(childObjs(rootObj)[0], 'margin-top', 12) }
  else win.children = content
  return { root: win, target: 'gtk4', notes: [...notes] }
}

export function importUiFile(text: string): ImportResult {
  if (!/^\s*<(\?xml|ui|interface)/.test(text)) return importQml(text)
  return /<interface[\s>]/.test(text) ? importGtkUi(text) : importQtUi(text)
}

// ── QML (Qt Quick Controls) ──
// A small tolerant parser for the QML object syntax: `Type { prop: value; Child { ... } function f() { } }`.
interface QmlObj { type: string; props: Record<string, string>; children: QmlObj[] }

function parseQml(src: string): QmlObj {
  let i = 0
  const n = src.length
  const skipWs = () => {
    for (;;) {
      while (i < n && /\s/.test(src[i])) i++
      if (src.startsWith('//', i)) { while (i < n && src[i] !== '\n') i++; continue }
      if (src.startsWith('/*', i)) { const e = src.indexOf('*/', i + 2); i = e < 0 ? n : e + 2; continue }
      break
    }
  }
  const skipString = () => { const q = src[i++]; while (i < n && src[i] !== q) { if (src[i] === '\\') i++; i++ } i++ }
  // reads a property value up to `;`, or a newline once brackets are balanced and the line isn't continuing
  const readValue = (): string => {
    const start = i
    let depth = 0
    while (i < n) {
      const c = src[i]
      if (c === '"' || c === "'" || c === '`') { skipString(); continue }
      if (src.startsWith('//', i) && depth === 0) break
      if (c === '{' || c === '[' || c === '(') depth++
      else if (c === '}' || c === ']' || c === ')') { if (depth === 0) break; depth-- }
      else if (c === ';' && depth === 0) break
      else if (c === '\n' && depth === 0) {
        const prev = src.slice(start, i).trimEnd(), nextCh = src.slice(i).trimStart()[0]
        if (!/[+\-*/%&|?:,=<>(]$/.test(prev) && !/^[+\-*/%&|?:.,<>=]/.test(nextCh ?? '')) break
      }
      i++
    }
    return src.slice(start, i).trim()
  }
  const skipBalanced = () => {   // from just before `{` or `(`
    let depth = 0
    while (i < n) {
      const c = src[i]
      if (c === '"' || c === "'" || c === '`') { skipString(); continue }
      if (c === '{' || c === '(' || c === '[') depth++
      if (c === '}' || c === ')' || c === ']') { depth--; if (depth === 0) { i++; return } }
      i++
    }
  }
  const parseObject = (type: string): QmlObj => {   // `{` already consumed
    const obj: QmlObj = { type, props: {}, children: [] }
    for (;;) {
      skipWs()
      if (i >= n || src[i] === '}') { i++; return obj }
      if (src[i] === ';') { i++; continue }
      const m = /^[A-Za-z_][\w.]*/.exec(src.slice(i, i + 120))
      if (!m) { i++; continue }
      let word = m[0]; i += word.length; skipWs()
      if (word === 'function' || word === 'signal' || word === 'enum' || word === 'component') {
        while (i < n && src[i] !== '{' && src[i] !== '\n') i++
        if (src[i] === '{') skipBalanced()
        continue
      }
      if (word === 'property' || word === 'readonly' || word === 'required' || word === 'default') {   // property <type> name: value
        const t = /^(?:property\s+)?(?:var|int|real|string|bool|color|url|[A-Za-z_][\w.<>]*)\s+([A-Za-z_]\w*)/.exec(src.slice(i - word.length, i + 120))
        const nm = /^(?:\w+\s+)*?([A-Za-z_]\w*)\s*:/.exec(src.slice(i, i + 200))
        const colon = src.indexOf(':', i); const brace = src.indexOf('{', i); const nl = src.indexOf('\n', i)
        if (colon >= 0 && (nl < 0 || colon < nl)) { i = colon + 1; readValue() } else if (brace >= 0 && (nl < 0 || brace < nl)) { i = brace; skipBalanced() } else { while (i < n && src[i] !== '\n') i++ }
        void t; void nm
        continue
      }
      if (src[i] === ':') { i++; skipWs(); const v = readValue(); obj.props[word] = v; continue }
      if (src[i] === '{') { i++; const child = parseObject(word); obj.children.push(child); continue }
      // `on Type { }` / attached handlers etc.: skip the line
      while (i < n && src[i] !== '\n' && src[i] !== '}') i++
      void word
    }
  }
  skipWs()
  while (/^(import|pragma)\b/.test(src.slice(i, i + 8))) { while (i < n && src[i] !== '\n') i++; skipWs() }
  const m = /^[A-Za-z_][\w.]*/.exec(src.slice(i, i + 120))
  if (!m) throw new Error('No encontré un objeto raíz en el QML')
  i += m[0].length; skipWs()
  if (src[i] !== '{') throw new Error('El QML no tiene el formato esperado')
  i++
  return parseObject(m[0])
}

const qmlStr = (v: string | undefined) => {
  if (v == null) return ''
  const m = /^\s*(["'`])((?:\\.|(?!\1)[^\\])*)\1\s*$/.exec(v)
  if (m) return m[2].replace(/\\n/g, '\n').replace(/\\"/g, '"').replace(/\\'/g, "'")
  return v.replace(/qsTr\(\s*(["'])(.*?)\1\s*\)/, '$2').replace(/^["']|["']$/g, '')
}
const qmlNum = (v: string | undefined, d = 0) => { const x = Number(v); return v != null && v.trim() !== '' && Number.isFinite(x) ? x : d }
const qmlList = (v: string | undefined) => [...(v ?? '').matchAll(/(["'])((?:\\.|(?!\1)[^\\])*)\1/g)].map(m => m[2]).join('\n')
const qmlHandler = (o: QmlObj, ...names: string[]) => {
  for (const nme of names) {
    let v = (o.props[nme] ?? '').trim(); if (!v) continue
    if (v.startsWith('{')) v = v.slice(1).trim()                               // { guardar(); ... } -> first call
    if (/^(root|window|parent)\.(close|show|hide|quit|forceActiveFocus|destroy)\s*\(/.test(v)) continue   // built-in, not the user's function
    const m = /^(?:root\.|window\.)?([A-Za-z_]\w*)\s*\(/.exec(v)
    if (m && !/^(console|Qt|Math|JSON)$/.test(m[1])) return m[1]
  }
  return ''
}

export function importQml(src: string): ImportResult {
  const root = parseQml(src)
  const notes = new Set<string>()
  const win = newNode('window', { text: qmlStr(root.props.title) || 'Ventana', width: qmlNum(root.props.width, 640), height: qmlNum(root.props.height, 480) })

  const conv = (o: QmlObj, parent: 'v' | 'h'): UiNode[] => {
    const id = o.props.id ?? ''
    const base = { name: id }
    const fillH = /fillHeight\s*$/.test(Object.keys(o.props).find(k => k === 'Layout.fillHeight') ?? '') || o.props['Layout.fillHeight'] === 'true'
    const kidsOf = (p: 'v' | 'h') => o.children.flatMap(c => conv(c, p))
    switch (o.type) {
      case 'ColumnLayout': case 'Column': case 'RowLayout': case 'Row': case 'Flow': case 'Item': case 'Pane': case 'Frame': case 'Page': case 'Control': {
        const horiz = o.type === 'RowLayout' || o.type === 'Row'
        if (o.type === 'Item' && !o.children.length) return [newNode('spacer', base)]
        const b = newNode(horiz ? 'hbox' : 'vbox', { ...base, spacing: qmlNum(o.props.spacing, 8), expand: fillH })
        b.children = kidsOf(horiz ? 'h' : 'v'); return [b]
      }
      case 'GridLayout': case 'Grid': notes.add(`${o.type}: se convirtió en una caja vertical`); { const b = newNode('vbox', base); b.children = kidsOf('v'); return [b] }
      case 'GroupBox': { const b = newNode('frame', { ...base, text: qmlStr(o.props.title) }); b.children = o.children.flatMap(c => conv(c, 'v')); if (b.children.length === 1 && b.children[0].type === 'vbox' && !b.children[0].name) b.children = b.children[0].children; return [b] }
      case 'ScrollView': case 'Flickable': return o.children.flatMap(c => conv(c, parent))
      case 'Label': case 'Text': return [newNode('label', { ...base, text: qmlStr(o.props.text) })]
      case 'Button': case 'ToolButton': case 'RoundButton': case 'DelayButton': return [newNode('button', { ...base, text: qmlStr(o.props.text), handler: qmlHandler(o, 'onClicked', 'onPressed') })]
      case 'TextField': case 'TextInput': case 'SearchField': return [newNode('lineedit', { ...base, placeholder: qmlStr(o.props.placeholderText), handler: qmlHandler(o, 'onTextChanged', 'onTextEdited', 'onAccepted') })]
      case 'TextArea': case 'TextEdit': return [newNode('textedit', { ...base, placeholder: qmlStr(o.props.placeholderText), handler: qmlHandler(o, 'onTextChanged') })]
      case 'CheckBox': case 'Switch': case 'RadioButton': case 'CheckDelegate': return [newNode('checkbox', { ...base, text: qmlStr(o.props.text), value: o.props.checked === 'true' ? 1 : 0, handler: qmlHandler(o, 'onToggled', 'onCheckedChanged') })]
      case 'ComboBox': return [newNode('combobox', { ...base, items: qmlList(o.props.model), handler: qmlHandler(o, 'onActivated', 'onCurrentIndexChanged') })]
      case 'Slider': case 'Dial': return [newNode('slider', { ...base, min: qmlNum(o.props.from, 0), max: qmlNum(o.props.to, 1), value: qmlNum(o.props.value, 0), handler: qmlHandler(o, 'onMoved', 'onValueChanged') })]
      case 'ProgressBar': return [newNode('progress', { ...base, min: qmlNum(o.props.from, 0), max: qmlNum(o.props.to, 1), value: qmlNum(o.props.value, 0) })]
      case 'SpinBox': return [newNode('spinbox', { ...base, min: qmlNum(o.props.from, 0), max: qmlNum(o.props.to, 99), value: qmlNum(o.props.value, 0), handler: qmlHandler(o, 'onValueModified', 'onValueChanged') })]
      case 'ListView': case 'GridView': case 'Repeater': return [newNode('listbox', { ...base, items: qmlList(o.props.model), expand: true })]
      case 'Image': case 'AnimatedImage': return [newNode('image', { ...base, text: qmlStr(o.props.source) })]
      case 'Rectangle': if (qmlNum(o.props.height, 99) <= 2 && !o.children.length) return [newNode('separator', base)]; notes.add('Rectangle: se convirtió en una caja'); { const b = newNode('vbox', base); b.children = kidsOf('v'); return [b] }
      default: notes.add(`Tipo no soportado: ${o.type} (se muestra como etiqueta)`); return [newNode('label', { ...base, text: `[${o.type}]${id ? ' ' + id : ''}` })]
    }
  }
  // the usual `ApplicationWindow { ColumnLayout { ... } }`: the layout is the window's own content
  let kids = root.children.flatMap(c => conv(c, 'v'))
  while (kids.length === 1 && kids[0].type === 'vbox' && !kids[0].name) { win.spacing = kids[0].spacing; kids = kids[0].children }
  win.children = kids
  return { root: win, target: 'qml', notes: [...notes] }
}
