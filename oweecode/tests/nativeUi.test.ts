import { describe, expect, it } from 'vitest'
import { generateNative, sampleLogin, sampleSettings, type UiNode } from '../src/composables/nativeUi'
import { importGtkUi, importQml, importQtUi, importUiFile } from '../src/composables/nativeUiImport'

const shape = (n: UiNode): string => n.type + (n.children.length ? `(${n.children.map(shape).join(',')})` : '')
const uiOf = (d: UiNode, t: any, l: any) => generateNative(d, t, l, 'x').find(f => /\.ui$|\.qml$/.test(f.path))!.content

describe('generated files are well formed', () => {
  it.each([['qtwidgets', 'pyside6'], ['gtk4', 'pygobject']] as const)('%s XML parses', (t, l) => {
    for (const d of [sampleLogin(), sampleSettings()]) {
      const doc = new DOMParser().parseFromString(uiOf(d, t, l), 'application/xml')
      expect(doc.querySelector('parsererror')).toBeNull()
    }
  })
  it('handlers become functions in the Python/C++ code', () => {
    const files = generateNative(sampleLogin(), 'qtwidgets', 'pyside6', 'login')
    const py = files.find(f => f.path.endsWith('.py'))!.content
    expect(py).toContain('def on_entrar(self')
    expect(py).toContain('.clicked.connect(self.on_entrar)')
    const cpp = generateNative(sampleLogin(), 'qtwidgets', 'cpp', 'login').map(f => f.content).join('\n')
    expect(cpp).toContain('void on_entrar();')
    expect(cpp).toContain('&QPushButton::clicked, this, &MainWindow::on_entrar')
  })
  it('GTK declares every named widget (also text areas) as a template child', () => {
    const py = generateNative(sampleSettings(), 'gtk4', 'pygobject', 's').find(f => f.path.endsWith('.py'))!.content
    expect(py).toContain('notas = Gtk.Template.Child()')
    expect(py).toContain('notas_view = Gtk.Template.Child()')
  })
})

describe('import gives back the same widget tree', () => {
  it.each([['qtwidgets', 'pyside6', importQtUi], ['gtk4', 'pygobject', importGtkUi], ['qml', 'pyside6', importQml]] as const)('%s round trip', (t, l, fn) => {
    for (const d of [sampleLogin(), sampleSettings()]) expect(shape(fn(uiOf(d, t, l)).root)).toBe(shape(d))
  })
  it('importUiFile picks the right parser', () => {
    expect(importUiFile(uiOf(sampleLogin(), 'gtk4', 'pygobject')).target).toBe('gtk4')
    expect(importUiFile(uiOf(sampleLogin(), 'qtwidgets', 'pyside6')).target).toBe('qtwidgets')
    expect(importUiFile(uiOf(sampleLogin(), 'qml', 'pyside6')).target).toBe('qml')
  })
  it('reports what it cannot represent (Qt Designer main window)', () => {
    const r = importQtUi(`<ui version="4.0"><widget class="QMainWindow" name="W"><widget class="QWidget" name="centralwidget"><layout class="QGridLayout" name="g"><item row="0" column="0"><widget class="QCalendarWidget" name="cal"/></item></layout></widget><widget class="QMenuBar" name="m"/></widget></ui>`)
    expect(r.notes.join(' ')).toMatch(/QGridLayout/)
    expect(r.notes.join(' ')).toMatch(/QCalendarWidget/)
    expect(r.root.children[0].type).toBe('label')
  })
  it('QML: parses a real-looking file with properties, signals and functions', () => {
    const r = importQml(`import QtQuick\nimport QtQuick.Controls\nApplicationWindow { id: root; width: 520; height: 420; title: qsTr("Pedidos")\n property int total: 0\n signal enviado(string id)\n function calcular(x) { for (var i = 0; i < x.length; i++) { total += x[i] } return total }\n ColumnLayout { TextField { id: txt; placeholderText: "Nombre"; onTextChanged: buscar(text) }\n Button { text: "Cancelar"; onClicked: root.close() }\n Button { text: "Guardar"; onClicked: { guardar(); console.log("ok") } } } }`)
    expect(r.root.text).toBe('Pedidos')
    expect(r.root.width).toBe(520)
    expect(r.root.children.map(c => c.handler)).toEqual(['buscar', '', 'guardar'])
  })
})
