// Model + code generators for the native UI designer (Qt Widgets, QML, GTK4).
// One widget tree -> Qt Designer .ui / QML / GtkBuilder .ui, plus the Python (PySide6, PyQt6,
// PyGObject) or C++ (Qt) code that loads it and wires the handlers.

export type WType = 'window' | 'vbox' | 'hbox' | 'frame' | 'label' | 'button' | 'lineedit' | 'textedit' | 'checkbox'
  | 'combobox' | 'slider' | 'progress' | 'spinbox' | 'listbox' | 'image' | 'separator' | 'spacer'

export interface UiNode {
  id: string
  type: WType
  name: string          // objectName / id used in code ("" = auto)
  text: string          // text / label / title
  handler: string       // function name run on the main event ("" = none)
  items: string         // combobox / listbox entries, one per line
  placeholder: string
  min: number; max: number; value: number
  expand: boolean       // grow to fill the free space
  width: number; height: number    // window size
  spacing: number; margin: number  // containers
  children: UiNode[]
}

export type Target = 'qtwidgets' | 'qml' | 'gtk4'
export type CodeLang = 'pyside6' | 'pyqt6' | 'cpp' | 'pygobject'

export const WIDGETS: { type: WType; label: string; icon: string; container?: boolean }[] = [
  { type: 'vbox', label: 'Caja vertical', icon: '▥', container: true },
  { type: 'hbox', label: 'Caja horizontal', icon: '▤', container: true },
  { type: 'frame', label: 'Grupo con título', icon: '▢', container: true },
  { type: 'label', label: 'Etiqueta', icon: 'Aa' },
  { type: 'button', label: 'Botón', icon: '⏍' },
  { type: 'lineedit', label: 'Campo de texto', icon: '▭' },
  { type: 'textedit', label: 'Área de texto', icon: '☰' },
  { type: 'checkbox', label: 'Casilla', icon: '☑' },
  { type: 'combobox', label: 'Lista desplegable', icon: '▾' },
  { type: 'slider', label: 'Deslizador', icon: '⎯●' },
  { type: 'progress', label: 'Barra de progreso', icon: '▰' },
  { type: 'spinbox', label: 'Número', icon: '⇅' },
  { type: 'listbox', label: 'Lista', icon: '≣' },
  { type: 'image', label: 'Imagen', icon: '▣' },
  { type: 'separator', label: 'Separador', icon: '—' },
  { type: 'spacer', label: 'Espacio flexible', icon: '↔' },
]
export const isContainer = (t: WType) => t === 'window' || t === 'vbox' || t === 'hbox' || t === 'frame'

let seq = 0
export function newNode(type: WType, over: Partial<UiNode> = {}): UiNode {
  const defaults: Record<WType, Partial<UiNode>> = {
    window: { text: 'Mi aplicación', width: 640, height: 480, spacing: 8, margin: 12 },
    vbox: { spacing: 8 }, hbox: { spacing: 8 }, frame: { text: 'Grupo', spacing: 8 },
    label: { text: 'Etiqueta' }, button: { text: 'Aceptar', handler: '' },
    lineedit: { placeholder: 'Escribí aquí…' }, textedit: { placeholder: 'Texto…', expand: true },
    checkbox: { text: 'Opción' }, combobox: { items: 'Opción 1\nOpción 2\nOpción 3' },
    slider: { min: 0, max: 100, value: 50 }, progress: { min: 0, max: 100, value: 40 }, spinbox: { min: 0, max: 100, value: 1 },
    listbox: { items: 'Elemento 1\nElemento 2\nElemento 3', expand: true }, image: { text: '' }, separator: {}, spacer: { expand: true },
  }
  return { id: `w${++seq}_${Math.random().toString(36).slice(2, 6)}`, type, name: '', text: '', handler: '', items: '', placeholder: '',
    min: 0, max: 100, value: 0, expand: false, width: 0, height: 0, spacing: 8, margin: 0, children: [], ...defaults[type], ...over }
}

// ── helpers ──
const esc = (s: string) => String(s ?? '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
const ident = (s: string) => {
  const t = String(s ?? '').normalize('NFD').replace(/[̀-ͯ]/g, '').replace(/[^a-zA-Z0-9_]+/g, '_').replace(/^_+|_+$/g, '')
  return /^[0-9]/.test(t) ? `w_${t}` : t
}
const lines = (s: string) => String(s ?? '').split('\n').map(x => x.trim()).filter(Boolean)
const pad = (n: number) => '  '.repeat(n)

// Unique, valid names for every node (user-given or "<type>_<n>")
export function assignNames(root: UiNode): Map<string, string> {
  const names = new Map<string, string>(), used = new Set<string>(), counter: Record<string, number> = {}
  const walk = (n: UiNode) => {
    let base = ident(n.name) || `${n.type === 'window' ? 'main_window' : n.type}`
    if (!ident(n.name)) { counter[base] = (counter[base] ?? 0) + 1; if (counter[base] > 1 || n.type !== 'window') base = `${base}_${counter[base]}` }
    let name = base, k = 2
    while (used.has(name)) name = `${base}_${k++}`
    used.add(name); names.set(n.id, name)
    n.children.forEach(walk)
  }
  walk(root)
  return names
}
const handlerName = (n: UiNode) => ident(n.handler)

// Which event each widget reports, per toolkit
const QT_SIGNAL: Partial<Record<WType, { sig: string; args: string; cpp: string; cppArgs: string; cls: string }>> = {
  button:   { sig: 'clicked', args: '', cpp: 'clicked', cppArgs: '', cls: 'QPushButton' },
  lineedit: { sig: 'textChanged', args: 'text', cpp: 'textChanged', cppArgs: 'const QString &text', cls: 'QLineEdit' },
  textedit: { sig: 'textChanged', args: '', cpp: 'textChanged', cppArgs: '', cls: 'QPlainTextEdit' },
  checkbox: { sig: 'toggled', args: 'checked', cpp: 'toggled', cppArgs: 'bool checked', cls: 'QCheckBox' },
  combobox: { sig: 'currentIndexChanged', args: 'index', cpp: 'currentIndexChanged', cppArgs: 'int index', cls: 'QComboBox' },
  slider:   { sig: 'valueChanged', args: 'value', cpp: 'valueChanged', cppArgs: 'int value', cls: 'QSlider' },
  spinbox:  { sig: 'valueChanged', args: 'value', cpp: 'valueChanged', cppArgs: 'int value', cls: 'QSpinBox' },
  listbox:  { sig: 'currentRowChanged', args: 'row', cpp: 'currentRowChanged', cppArgs: 'int row', cls: 'QListWidget' },
}
const GTK_SIGNAL: Partial<Record<WType, { sig: string; py: string }>> = {
  button: { sig: 'clicked', py: 'self, button' }, lineedit: { sig: 'changed', py: 'self, entry' }, checkbox: { sig: 'toggled', py: 'self, button' },
  combobox: { sig: 'notify::selected', py: 'self, dropdown, _pspec' }, slider: { sig: 'value-changed', py: 'self, scale' },
  spinbox: { sig: 'value-changed', py: 'self, spin' }, listbox: { sig: 'row-selected', py: 'self, listbox, row' },
}
function flat(root: UiNode): UiNode[] { return [root, ...root.children.flatMap(flat)] }
const withHandlers = (root: UiNode) => flat(root).filter(n => handlerName(n))

// ══════════ Qt Widgets .ui (Qt Designer / uic) ══════════
export function genQtUi(root: UiNode, className = 'MainWindow'): string {
  const names = assignNames(root)
  const nm = (n: UiNode) => names.get(n.id)!
  const prop = (name: string, inner: string, extra = '') => `<property name="${name}"${extra}>${inner}</property>`
  const str = (v: string) => `<string>${esc(v)}</string>`
  const num = (v: number) => `<number>${Math.round(v)}</number>`
  const policy = (expand: boolean) => expand ? prop('sizePolicy', '<sizepolicy hsizetype="Expanding" vsizetype="Expanding"><horstretch>0</horstretch><verstretch>0</verstretch></sizepolicy>') : ''
  const layoutOf = (n: UiNode, orient: 'v' | 'h', depth: number, margin = 0) => {
    const cls = orient === 'v' ? 'QVBoxLayout' : 'QHBoxLayout'
    const m = margin ? ['left', 'top', 'right', 'bottom'].map(s => prop(`${s}Margin`, num(margin))).join('') : ''
    return `${pad(depth)}<layout class="${cls}" name="${nm(n)}_layout">${prop('spacing', num(n.spacing || 8))}${m}\n` +
      n.children.map(c => item(c, depth + 1, orient)).join('') + `${pad(depth)}</layout>\n`
  }
  const widget = (n: UiNode, depth: number): string => {
    const name = nm(n), p = pad(depth + 1)
    const w = (cls: string, body: string) => `${pad(depth)}<widget class="${cls}" name="${name}">\n${body}${pad(depth)}</widget>\n`
    const P = (s: string) => (s ? `${p}${s}\n` : '')
    switch (n.type) {
      case 'label': return w('QLabel', P(prop('text', str(n.text))) + P(prop('wordWrap', '<bool>true</bool>')) + P(policy(n.expand)))
      case 'button': return w('QPushButton', P(prop('text', str(n.text))) + P(policy(n.expand)))
      case 'lineedit': return w('QLineEdit', P(prop('placeholderText', str(n.placeholder))) + P(policy(n.expand)))
      case 'textedit': return w('QPlainTextEdit', P(prop('placeholderText', str(n.placeholder))) + P(policy(n.expand)))
      case 'checkbox': return w('QCheckBox', P(prop('text', str(n.text))) + P(prop('checked', `<bool>${n.value ? 'true' : 'false'}</bool>`)))
      case 'combobox': return w('QComboBox', lines(n.items).map(i => `${p}<item>${prop('text', str(i))}</item>\n`).join('') + P(policy(n.expand)))
      case 'slider': return w('QSlider', P(prop('minimum', num(n.min))) + P(prop('maximum', num(n.max))) + P(prop('value', num(n.value))) + P(prop('orientation', '<enum>Qt::Horizontal</enum>')))
      case 'progress': return w('QProgressBar', P(prop('minimum', num(n.min))) + P(prop('maximum', num(n.max))) + P(prop('value', num(n.value))))
      case 'spinbox': return w('QSpinBox', P(prop('minimum', num(n.min))) + P(prop('maximum', num(n.max))) + P(prop('value', num(n.value))))
      case 'listbox': return w('QListWidget', lines(n.items).map(i => `${p}<item>${prop('text', str(i))}</item>\n`).join('') + P(policy(n.expand)))
      case 'image': return w('QLabel', P(prop('text', str(n.text ? `[imagen: ${n.text}]` : '[imagen]'))) + P(prop('scaledContents', '<bool>true</bool>')) + P(policy(n.expand)))
      case 'separator': return w('QFrame', P(prop('frameShape', '<enum>QFrame::HLine</enum>')) + P(prop('frameShadow', '<enum>QFrame::Sunken</enum>')))
      case 'frame': return `${pad(depth)}<widget class="QGroupBox" name="${name}">\n${P(prop('title', str(n.text)))}${layoutOf(n, 'v', depth + 1)}${pad(depth)}</widget>\n`
      default: return ''
    }
  }
  const item = (n: UiNode, depth: number, parentOrient: 'v' | 'h'): string => {
    if (n.type === 'vbox' || n.type === 'hbox') return `${pad(depth)}<item>\n${layoutOf(n, n.type === 'vbox' ? 'v' : 'h', depth + 1)}${pad(depth)}</item>\n`
    if (n.type === 'spacer') {
      const vert = parentOrient === 'v'
      return `${pad(depth)}<item>\n${pad(depth + 1)}<spacer name="${nm(n)}">${prop('orientation', `<enum>Qt::${vert ? 'Vertical' : 'Horizontal'}</enum>`)}${prop('sizeHint', `<size><width>${vert ? 20 : 40}</width><height>${vert ? 40 : 20}</height></size>`, ' stdset="0"')}</spacer>\n${pad(depth)}</item>\n`
    }
    return `${pad(depth)}<item>\n${widget(n, depth + 1)}${pad(depth)}</item>\n`
  }
  const win = `<?xml version="1.0" encoding="UTF-8"?>\n<ui version="4.0">\n <class>${className}</class>\n <widget class="QWidget" name="${className}">\n` +
    `  <property name="geometry"><rect><x>0</x><y>0</y><width>${root.width || 640}</width><height>${root.height || 480}</height></rect></property>\n` +
    `  <property name="windowTitle">${str(root.text)}</property>\n` +
    layoutOf({ ...root, name: 'root' }, 'v', 2, root.margin).replace(`name="${nm(root)}_layout"`, 'name="rootLayout"') +
    ` </widget>\n <resources/>\n <connections/>\n</ui>\n`
  return win
}

// ══════════ QML ══════════
export function genQml(root: UiNode): string {
  const names = assignNames(root)
  const q = (s: string) => JSON.stringify(s ?? '')
  const idOf = (n: UiNode) => { const v = ident(n.name); return v ? `${v[0].toLowerCase()}${v.slice(1)}` : '' }
  const handlers = withHandlers(root)
  const call = (n: UiNode, arg: string) => `${handlerName(n)}(${arg})`
  const fill = (parent: 'v' | 'h', n: UiNode) => (parent === 'v' ? 'Layout.fillWidth: true' : (n.expand ? 'Layout.fillWidth: true' : ''))
  const out: string[] = []
  const emit = (depth: number, s: string) => out.push(pad(depth + 1) + s)
  const body = (n: UiNode, depth: number, parent: 'v' | 'h') => {
    const id = idOf(n) ? `id: ${idOf(n)}` : ''
    const props = (...p: string[]) => p.filter(Boolean)
    const block = (type: string, ps: string[]) => { emit(depth, `${type} {`); ps.filter(Boolean).forEach(x => emit(depth + 1, x)); emit(depth, '}') }
    const grow = n.expand ? 'Layout.fillHeight: true' : ''
    switch (n.type) {
      case 'vbox': case 'hbox':
        emit(depth, `${n.type === 'vbox' ? 'ColumnLayout' : 'RowLayout'} {`); emit(depth + 1, id); out.pop(); if (id) emit(depth + 1, id)
        emit(depth + 1, `spacing: ${n.spacing || 8}`); emit(depth + 1, fill(parent, n) || 'Layout.fillWidth: true'); if (grow) emit(depth + 1, grow)
        n.children.forEach(c => body(c, depth + 1, n.type === 'vbox' ? 'v' : 'h')); emit(depth, '}'); break
      case 'frame':
        emit(depth, 'GroupBox {'); if (id) emit(depth + 1, id); emit(depth + 1, `title: ${q(n.text)}`); emit(depth + 1, 'Layout.fillWidth: true')
        emit(depth + 1, 'ColumnLayout {'); emit(depth + 2, 'anchors.fill: parent'); emit(depth + 2, `spacing: ${n.spacing || 8}`)
        n.children.forEach(c => body(c, depth + 2, 'v')); emit(depth + 1, '}'); emit(depth, '}'); break
      case 'label': block('Label', props(id, `text: ${q(n.text)}`, 'wrapMode: Text.WordWrap', fill(parent, n), grow)); break
      case 'button': block('Button', props(id, `text: ${q(n.text)}`, handlerName(n) ? `onClicked: ${call(n, '')}` : '', fill(parent, n) && n.expand ? fill(parent, n) : '')); break
      case 'lineedit': block('TextField', props(id, `placeholderText: ${q(n.placeholder)}`, handlerName(n) ? `onTextChanged: ${call(n, 'text')}` : '', fill(parent, n) || 'Layout.fillWidth: true')); break
      case 'textedit':
        emit(depth, 'ScrollView {'); emit(depth + 1, 'Layout.fillWidth: true'); emit(depth + 1, 'Layout.fillHeight: true')
        emit(depth + 1, 'TextArea {'); if (id) emit(depth + 2, id); emit(depth + 2, `placeholderText: ${q(n.placeholder)}`)
        if (handlerName(n)) emit(depth + 2, `onTextChanged: ${call(n, 'text')}`); emit(depth + 1, '}'); emit(depth, '}'); break
      case 'checkbox': block('CheckBox', props(id, `text: ${q(n.text)}`, `checked: ${n.value ? 'true' : 'false'}`, handlerName(n) ? `onToggled: ${call(n, 'checked')}` : '')); break
      case 'combobox': block('ComboBox', props(id, `model: [${lines(n.items).map(q).join(', ')}]`, handlerName(n) ? `onActivated: ${call(n, 'currentIndex')}` : '', fill(parent, n) || 'Layout.fillWidth: true')); break
      case 'slider': block('Slider', props(id, `from: ${n.min}`, `to: ${n.max}`, `value: ${n.value}`, handlerName(n) ? `onMoved: ${call(n, 'value')}` : '', 'Layout.fillWidth: true')); break
      case 'progress': block('ProgressBar', props(id, `from: ${n.min}`, `to: ${n.max}`, `value: ${n.value}`, 'Layout.fillWidth: true')); break
      case 'spinbox': block('SpinBox', props(id, `from: ${n.min}`, `to: ${n.max}`, `value: ${n.value}`, handlerName(n) ? `onValueModified: ${call(n, 'value')}` : '')); break
      case 'listbox':
        block('ListView', props(id, `model: [${lines(n.items).map(q).join(', ')}]`, 'clip: true', 'Layout.fillWidth: true', 'Layout.fillHeight: true',
          'delegate: ItemDelegate { text: modelData; width: ListView.view.width' + (handlerName(n) ? `; onClicked: ${call(n, 'index')}` : '') + ' }')); break
      case 'image': block('Image', props(id, `source: ${q(n.text)}`, 'fillMode: Image.PreserveAspectFit', 'Layout.fillWidth: true', grow)); break
      case 'separator': block('Rectangle', props('Layout.fillWidth: true', 'height: 1', 'color: "#80808080"')); break
      case 'spacer': block('Item', props(parent === 'v' ? 'Layout.fillHeight: true' : 'Layout.fillWidth: true')); break
    }
  }
  emit(-1, 'ColumnLayout {'); out.pop()
  const inner: string[] = []
  const saved = out.splice(0)
  root.children.forEach(c => body(c, 2, 'v'))
  inner.push(...out.splice(0)); out.push(...saved)
  const fns = handlers.map(n => {
    const args = n.type === 'lineedit' || n.type === 'textedit' ? 'text' : n.type === 'checkbox' ? 'checked' : n.type === 'combobox' ? 'index' : n.type === 'slider' || n.type === 'spinbox' ? 'value' : n.type === 'listbox' ? 'index' : ''
    return `    function ${handlerName(n)}(${args}) {\n        console.log("${handlerName(n)}"${args ? `, ${args}` : ''})   // TODO: tu lógica\n    }`
  })
  void names
  return ['import QtQuick', 'import QtQuick.Controls', 'import QtQuick.Layouts', '',
    'ApplicationWindow {', '    id: root', '    visible: true', `    width: ${root.width || 640}`, `    height: ${root.height || 480}`, `    title: ${q(root.text)}`, '',
    '    ColumnLayout {', '        anchors.fill: parent', `        anchors.margins: ${root.margin || 12}`, `        spacing: ${root.spacing || 8}`,
    ...inner, '    }', ...(fns.length ? ['', ...fns] : []), '}', ''].join('\n')
}

// ══════════ GTK4 (GtkBuilder .ui, as a Template) ══════════
export function genGtkUi(root: UiNode, className = 'AppWindow'): string {
  const names = assignNames(root)
  const nm = (n: UiNode) => names.get(n.id)!
  const P = (name: string, v: string | number | boolean) => `<property name="${name}">${typeof v === 'boolean' ? (v ? 'True' : 'False') : esc(String(v))}</property>`
  const sig = (n: UiNode) => { const s = GTK_SIGNAL[n.type]; return s && handlerName(n) ? `<signal name="${s.sig}" handler="${handlerName(n)}"/>` : '' }
  const adj = (n: UiNode, step = 1) => `<property name="adjustment"><object class="GtkAdjustment">${P('lower', n.min)}${P('upper', n.max)}${P('value', n.value)}${P('step-increment', step)}</object></property>`
  const child = (n: UiNode, depth: number, parent: 'v' | 'h'): string => `${pad(depth)}<child>\n${obj(n, depth + 1, parent)}${pad(depth)}</child>\n`
  const obj = (n: UiNode, depth: number, parent: 'v' | 'h'): string => {
    const id = nm(n), p = pad(depth + 1)
    const grow = n.expand ? (parent === 'v' ? P('vexpand', true) : P('hexpand', true)) : ''
    const wrap = (cls: string, props: string[], extra = '') =>
      `${pad(depth)}<object class="${cls}" id="${id}">\n${props.filter(Boolean).map(x => `${p}${x}\n`).join('')}${extra}${pad(depth)}</object>\n`
    const sg = sig(n)
    switch (n.type) {
      case 'vbox': case 'hbox': return wrap('GtkBox', [P('orientation', n.type === 'vbox' ? 'vertical' : 'horizontal'), P('spacing', n.spacing || 8), grow, n.type === 'vbox' ? P('hexpand', true) : ''],
        n.children.map(c => child(c, depth + 1, n.type === 'vbox' ? 'v' : 'h')).join(''))
      case 'frame': return wrap('GtkFrame', [P('label', n.text), grow],
        `${p}<child>\n${pad(depth + 2)}<object class="GtkBox" id="${id}_box">\n${[P('orientation', 'vertical'), P('spacing', n.spacing || 8), P('margin-top', 8), P('margin-bottom', 8), P('margin-start', 8), P('margin-end', 8)].map(x => `${pad(depth + 3)}${x}\n`).join('')}${n.children.map(c => child(c, depth + 3, 'v')).join('')}${pad(depth + 2)}</object>\n${p}</child>\n`)
      case 'label': return wrap('GtkLabel', [P('label', n.text), P('wrap', true), P('xalign', 0), grow])
      case 'button': return wrap('GtkButton', [P('label', n.text), grow], sg ? `${p}${sg}\n` : '')
      case 'lineedit': return wrap('GtkEntry', [P('placeholder-text', n.placeholder), P('hexpand', true)], sg ? `${p}${sg}\n` : '')
      case 'textedit': return wrap('GtkScrolledWindow', [P('vexpand', true), P('hexpand', true)], `${p}<child>\n${pad(depth + 2)}<object class="GtkTextView" id="${id}_view"/>\n${p}</child>\n`)
      case 'checkbox': return wrap('GtkCheckButton', [P('label', n.text), P('active', !!n.value)], sg ? `${p}${sg}\n` : '')
      case 'combobox': return wrap('GtkDropDown', [grow],
        `${p}<property name="model"><object class="GtkStringList"><items>${lines(n.items).map(i => `<item translatable="no">${esc(i)}</item>`).join('')}</items></object></property>\n${sg ? `${p}${sg}\n` : ''}`)
      case 'slider': return wrap('GtkScale', [P('orientation', 'horizontal'), P('draw-value', true), P('hexpand', true), adj(n)], sg ? `${p}${sg}\n` : '')
      case 'progress': return wrap('GtkProgressBar', [P('fraction', n.max > n.min ? Math.max(0, Math.min(1, (n.value - n.min) / (n.max - n.min))) : 0), P('hexpand', true)])
      case 'spinbox': return wrap('GtkSpinButton', [adj(n)], sg ? `${p}${sg}\n` : '')
      case 'listbox': return wrap('GtkScrolledWindow', [P('vexpand', true), P('hexpand', true)],
        `${p}<child>\n${pad(depth + 2)}<object class="GtkListBox" id="${id}_list">\n${sg ? `${pad(depth + 3)}${sg.replace('handler=', 'handler=')}\n` : ''}${lines(n.items).map(i => `${pad(depth + 3)}<child><object class="GtkLabel">${P('label', i)}${P('xalign', 0)}</object></child>\n`).join('')}${pad(depth + 2)}</object>\n${p}</child>\n`)
      case 'image': return wrap('GtkImage', [n.text ? P('file', n.text) : P('icon-name', 'image-x-generic'), P('pixel-size', 96), grow])
      case 'separator': return wrap('GtkSeparator', [P('orientation', 'horizontal')])
      case 'spacer': return wrap('GtkBox', [P('orientation', 'vertical'), parent === 'v' ? P('vexpand', true) : P('hexpand', true)])
    }
    return ''
  }
  const rootBox = `${pad(2)}<child>\n${pad(3)}<object class="GtkBox" id="root_box">\n${[P('orientation', 'vertical'), P('spacing', root.spacing || 8), P('margin-top', root.margin || 12), P('margin-bottom', root.margin || 12), P('margin-start', root.margin || 12), P('margin-end', root.margin || 12)].map(x => `${pad(4)}${x}\n`).join('')}${root.children.map(c => child(c, 4, 'v')).join('')}${pad(3)}</object>\n${pad(2)}</child>\n`
  return `<?xml version="1.0" encoding="UTF-8"?>\n<interface>\n  <requires lib="gtk" version="4.0"/>\n  <template class="${className}" parent="GtkApplicationWindow">\n    ${P('title', root.text)}\n    ${P('default-width', root.width || 640)}\n    ${P('default-height', root.height || 480)}\n${rootBox.split('\n').map(l => (l ? '  ' + l.slice(0) : l)).join('\n').replace(/^ {2}/, '')}  </template>\n</interface>\n`
}

// ══════════ Code that loads the UI ══════════
const py = (s: string) => JSON.stringify(s)

export function genPySide(root: UiNode, uiRel: string, qt6 = false): string {
  const names = assignNames(root)
  const hs = withHandlers(root)
  const lib = qt6 ? 'PyQt6' : 'PySide6'
  const conns = hs.map(n => `        self.${names.get(n.id)}.${QT_SIGNAL[n.type]?.sig ?? 'clicked'}.connect(self.${handlerName(n)})`)
  const fns = [...new Set(hs.map(handlerName))].map(h => `    def ${h}(self, *args):\n        print(${py(h)}, *args)   # TODO: tu lógica`)
  const common = `import sys\nfrom pathlib import Path\n\nUI_FILE = Path(__file__).parent / ${py(uiRel)}\n`
  if (qt6) return `${common}\nfrom PyQt6 import uic\nfrom PyQt6.QtWidgets import QApplication, QWidget\n\n\nclass MainWindow(QWidget):\n    def __init__(self):\n        super().__init__()\n        uic.loadUi(str(UI_FILE), self)   # los widgets quedan como atributos: self.<nombre>\n${conns.length ? conns.join('\n') + '\n' : ''}${fns.length ? '\n' + fns.join('\n\n') + '\n' : ''}\n\ndef main():\n    app = QApplication(sys.argv)\n    window = MainWindow()\n    window.show()\n    sys.exit(app.exec())\n\n\nif __name__ == "__main__":\n    main()\n`
  return `${common}\nfrom PySide6.QtCore import QFile, QIODevice\nfrom PySide6.QtUiTools import QUiLoader\nfrom PySide6.QtWidgets import QApplication\n\n\nclass MainWindow:\n    def __init__(self):\n        loader = QUiLoader()\n        f = QFile(str(UI_FILE))\n        f.open(QIODevice.OpenModeFlag.ReadOnly)\n        self.ui = loader.load(f)   # los widgets quedan como atributos: self.ui.<nombre>\n        f.close()\n${conns.length ? conns.map(c => c.replace('self.', 'self.ui.')).join('\n') + '\n' : ''}\n    def show(self):\n        self.ui.show()\n${fns.length ? '\n' + fns.join('\n\n') + '\n' : ''}\n\ndef main():\n    app = QApplication(sys.argv)\n    window = MainWindow()\n    window.show()\n    sys.exit(app.exec())\n\n\nif __name__ == "__main__":\n    main()\n`.replace(`from PySide6`, `from ${lib}`)
}

export function genPyGObject(root: UiNode, uiRel: string): string {
  const names = assignNames(root)
  const hs = withHandlers(root)
  // every named widget becomes a Gtk.Template.Child; scrolled text areas / lists also expose the inner view
  const children = flat(root).filter(n => n.type !== 'window' && n.type !== 'spacer' && n.type !== 'separator')
    .flatMap(n => [`    ${names.get(n.id)} = Gtk.Template.Child()`, ...(n.type === 'textedit' ? [`    ${names.get(n.id)}_view = Gtk.Template.Child()`] : n.type === 'listbox' ? [`    ${names.get(n.id)}_list = Gtk.Template.Child()`] : [])])
  const cbs = [...new Map(hs.map(n => [handlerName(n), n])).values()].map(n => `    @Gtk.Template.Callback()\n    def ${handlerName(n)}(${GTK_SIGNAL[n.type]?.py ?? 'self, widget'}, *args):\n        print(${py(handlerName(n))})   # TODO: tu lógica`)
  return `import sys\nfrom pathlib import Path\n\nimport gi\n\ngi.require_version("Gtk", "4.0")\nfrom gi.repository import Gtk  # noqa: E402\n\nUI_FILE = Path(__file__).parent / ${py(uiRel)}\n\n\n@Gtk.Template(filename=str(UI_FILE))\nclass AppWindow(Gtk.ApplicationWindow):\n    __gtype_name__ = "AppWindow"\n\n${children.length ? children.join('\n') + '\n' : '    pass\n'}${cbs.length ? '\n' + cbs.join('\n\n') + '\n' : ''}\n\nclass App(Gtk.Application):\n    def __init__(self):\n        super().__init__(application_id="com.example.app")\n\n    def do_activate(self):\n        win = self.props.active_window or AppWindow(application=self)\n        win.present()\n\n\nif __name__ == "__main__":\n    sys.exit(App().run(sys.argv))\n`
}

export function genQmlLauncher(qmlRel: string): string {
  return `import sys\nfrom pathlib import Path\n\nfrom PySide6.QtGui import QGuiApplication\nfrom PySide6.QtQml import QQmlApplicationEngine\n\n\ndef main():\n    app = QGuiApplication(sys.argv)\n    engine = QQmlApplicationEngine()\n    engine.load(str(Path(__file__).parent / ${py(qmlRel)}))\n    if not engine.rootObjects():\n        sys.exit(-1)\n    sys.exit(app.exec())\n\n\nif __name__ == "__main__":\n    main()\n`
}

export function genQtCppProject(root: UiNode, projectName: string): { path: string; content: string }[] {
  const names = assignNames(root)
  const hs = withHandlers(root)
  const target = ident(projectName).toLowerCase() || 'app'
  const slots = [...new Map(hs.map(n => [handlerName(n), n])).values()]
  const sigOf = (n: UiNode) => QT_SIGNAL[n.type]!
  const header = `#pragma once\n\n#include <QWidget>\n\nQT_BEGIN_NAMESPACE\nnamespace Ui { class MainWindow; }\nQT_END_NAMESPACE\n\nclass MainWindow : public QWidget {\n    Q_OBJECT\n\npublic:\n    explicit MainWindow(QWidget *parent = nullptr);\n    ~MainWindow() override;\n\nprivate slots:\n${slots.map(n => `    void ${handlerName(n)}(${sigOf(n).cppArgs});`).join('\n')}\n\nprivate:\n    Ui::MainWindow *ui;\n};\n`
  const src = `#include "mainwindow.h"\n#include "ui_mainwindow.h"\n\n#include <QDebug>\n\nMainWindow::MainWindow(QWidget *parent) : QWidget(parent), ui(new Ui::MainWindow) {\n    ui->setupUi(this);\n${hs.map(n => `    connect(ui->${names.get(n.id)}, &${sigOf(n).cls}::${sigOf(n).cpp}, this, &MainWindow::${handlerName(n)});`).join('\n')}\n}\n\nMainWindow::~MainWindow() { delete ui; }\n\n${slots.map(n => `void MainWindow::${handlerName(n)}(${sigOf(n).cppArgs}) {\n    qDebug() << "${handlerName(n)}";   // TODO: tu lógica\n}\n`).join('\n')}`
  return [
    { path: 'CMakeLists.txt', content: `cmake_minimum_required(VERSION 3.20)\nproject(${target} LANGUAGES CXX)\n\nset(CMAKE_CXX_STANDARD 17)\nset(CMAKE_CXX_STANDARD_REQUIRED ON)\nset(CMAKE_AUTOMOC ON)\nset(CMAKE_AUTOUIC ON)\nset(CMAKE_EXPORT_COMPILE_COMMANDS ON)\n\nfind_package(Qt6 REQUIRED COMPONENTS Widgets)\n\nqt_add_executable(${target}\n    main.cpp\n    mainwindow.cpp\n    mainwindow.h\n    mainwindow.ui\n)\ntarget_link_libraries(${target} PRIVATE Qt6::Widgets)\n` },
    { path: 'main.cpp', content: `#include "mainwindow.h"\n\n#include <QApplication>\n\nint main(int argc, char *argv[]) {\n    QApplication app(argc, argv);\n    MainWindow w;\n    w.show();\n    return app.exec();\n}\n` },
    { path: 'mainwindow.h', content: header }, { path: 'mainwindow.cpp', content: src },
    { path: 'mainwindow.ui', content: genQtUi(root, 'MainWindow') },
  ]
}

export interface GenFile { path: string; content: string }

/** All files for a design. `name` is the project/file base name. */
export function generateNative(root: UiNode, target: Target, lang: CodeLang, name: string): GenFile[] {
  const n = ident(name).toLowerCase() || 'interfaz'
  if (target === 'qtwidgets') {
    if (lang === 'cpp') return genQtCppProject(root, n).map(f => ({ path: `cpp_${n}/${f.path}`, content: f.content }))
    return [{ path: `ui/${n}.ui`, content: genQtUi(root, 'MainWindow') }, { path: `${n}.py`, content: genPySide(root, `ui/${n}.ui`, lang === 'pyqt6') }]
  }
  if (target === 'qml') return [{ path: `qml/${n}.qml`, content: genQml(root) }, { path: `${n}_qml.py`, content: genQmlLauncher(`qml/${n}.qml`) }]
  return [{ path: `ui/${n}.gtk.ui`, content: genGtkUi(root, 'AppWindow') }, { path: `${n}_gtk.py`, content: genPyGObject(root, `ui/${n}.gtk.ui`) }]
}

// ── sample designs ──
export function sampleLogin(): UiNode {
  const w = newNode('window', { text: 'Iniciar sesión', width: 380, height: 320 })
  w.children = [
    newNode('label', { text: 'Bienvenido', name: 'titulo' }),
    newNode('lineedit', { name: 'usuario', placeholder: 'Usuario' }),
    newNode('lineedit', { name: 'clave', placeholder: 'Contraseña', handler: 'on_clave_changed' }),
    newNode('checkbox', { name: 'recordar', text: 'Recordarme', handler: 'on_recordar' }),
    newNode('spacer'),
    newNode('hbox', { children: [newNode('spacer'), newNode('button', { name: 'cancelar', text: 'Cancelar', handler: 'on_cancelar' }), newNode('button', { name: 'entrar', text: 'Entrar', handler: 'on_entrar' })] }),
  ]
  return w
}
export function sampleSettings(): UiNode {
  const w = newNode('window', { text: 'Configuración', width: 460, height: 420 })
  w.children = [
    newNode('frame', { text: 'General', children: [newNode('combobox', { name: 'idioma', items: 'Español\nEnglish\nPortuguês', handler: 'on_idioma' }), newNode('checkbox', { name: 'inicio', text: 'Iniciar con el sistema' })] }),
    newNode('frame', { text: 'Apariencia', children: [newNode('slider', { name: 'tamano', min: 8, max: 32, value: 14, handler: 'on_tamano' }), newNode('progress', { name: 'avance', value: 60 })] }),
    newNode('textedit', { name: 'notas', placeholder: 'Notas…' }),
    newNode('hbox', { children: [newNode('spacer'), newNode('button', { name: 'guardar', text: 'Guardar', handler: 'on_guardar' })] }),
  ]
  return w
}
