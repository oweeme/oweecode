import { describe, expect, it } from 'vitest'
import { buildQuasarSfc, parseQuasarSfc, quasarToHtml, htmlToQuasar, templateToCanvas, canvasToTemplate } from '../src/composables/quasarConvert'
import { splitVueSfc, joinVueSfc, splitHtml, joinHtml } from '../src/composables/designFile'

describe('Quasar <-> canvas', () => {
  const real = `<div class="row q-pa-md"><q-input v-model.number="valor" outlined label="Nombre" :rules="[v => !!v || 'requerido']" @update:model-value="cambio" /><q-select v-model="ciudad" :options="ciudades" label="Ciudad" /><q-btn color="primary" :label="textoBoton" @click.prevent="guardar" /><KAvatar :tituloGrande="x" #prepend /><div class="vacio" /><q-input v-model="a"><template #prepend><q-icon name="x" /></template></q-input></div>`

  it('keeps bindings, events, slots, expressions and the case of components/props', () => {
    const { html } = templateToCanvas(real)
    const back = canvasToTemplate(html, null, null).template
    for (const frag of ['v-model.number="valor"', ':options="ciudades"', ':label="textoBoton"', '@click.prevent="guardar"', '@update:model-value="cambio"',
      "[v => !!v || 'requerido']", '<KAvatar', ':tituloGrande="x"', '#prepend', '<template #prepend>', '<q-icon name="x"']) expect(back).toContain(frag)
  })
  it('self-closed <div /> does not swallow its siblings', () => {
    const back = canvasToTemplate(templateToCanvas('<div class="a" /><p>hola</p>').html, null, null).template
    expect(back).toContain('<p>hola</p>')
    expect(back.indexOf('<p>')).toBeGreaterThan(back.indexOf('</div>'))
  })
  it('never produces attribute names the DOM would reject (that left the canvas blank)', () => {
    const html = quasarToHtml('<q-btn @click="x" label="a" /><div @mouseenter="m" #default="p">t</div>')
    expect(html).not.toContain('@click')
    expect(() => { const d = document.createElement('div'); d.innerHTML = html; d.querySelectorAll('*').forEach(e => Array.from(e.attributes).forEach(a => e.setAttribute(a.name, a.value))) }).not.toThrow()
  })
  it('new blocks get a reactive form model', () => {
    const { template, form } = htmlToQuasar('<div data-q="q-input" class="qf-input">Nombre</div><label data-q="q-checkbox" class="qf-checkbox">Acepto</label>')
    expect(template).toContain('v-model="form.nombre"')
    expect(form).toEqual({ nombre: "''", acepto: 'false' })
  })
  it('a generated page re-opens and re-saves to the same thing', () => {
    const html = '<div data-q="q-btn" class="qf-btn" data-qa-color="primary">Guardar</div>'
    const sfc = buildQuasarSfc(html, '.x{color:red}')
    expect(buildQuasarSfc(parseQuasarSfc(sfc).html, parseQuasarSfc(sfc).css)).toBe(sfc)
  })
})

describe('writing back into an existing file', () => {
  const sfc = `<template>\n  <q-page padding>\n    <q-btn label="x" />\n    <template v-if="ok"><p>a</p></template>\n  </q-page>\n</template>\n\n<script setup lang="ts">\nconst tpl = '<template>no</template>'\nfunction go() {}\n</script>\n\n<style scoped>\n.a { color: red }\n</style>\n`
  it('only replaces the root <template>, keeping script and style (even when a string contains "<template>")', () => {
    const p = splitVueSfc(sfc)!
    expect(p.inner).toContain('v-if="ok"')
    const out = joinVueSfc(p, '<q-page>nuevo</q-page>')
    expect(out).toContain('function go() {}')
    expect(out).toContain(".a { color: red }")
    expect(out).toContain("const tpl = '<template>no</template>'")
    expect(out).toContain('<q-page>nuevo</q-page>')
  })
  it('HTML: replaces body and style, keeps head', () => {
    const html = '<!doctype html><html><head><title>T</title><style>.a{color:red}</style></head><body><h1>Hi</h1></body></html>'
    expect(splitHtml(html).body).toBe('<h1>Hi</h1>')
    const out = joinHtml(html, '<h1>Nuevo</h1>', '.a{color:blue}')
    expect(out).toContain('<title>T</title>')
    expect(out).toContain('<h1>Nuevo</h1>')
    expect(out).toContain('.a{color:blue}')
  })
})
