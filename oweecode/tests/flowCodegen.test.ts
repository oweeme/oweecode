import { describe, expect, it } from 'vitest'
import { buildCode, CODE_LANGS } from '../src/composables/flowCodegen'

const nodes = [
  { id: 't', data: { kind: 'trigger', label: 'Nuevo pedido' } },
  { id: 'h', data: { kind: 'http', label: 'Consultar cliente', http: { method: 'GET', url: 'https://api.x.com/clientes/{{cliente.id}}', headers: '{"Authorization":"Bearer {{token}}"}', body: '' } } },
  { id: 'c', data: { kind: 'condition', label: 'Es VIP?' } },
  { id: 'f', data: { kind: 'function', label: 'Cobrar' } },
  { id: 'e', data: { kind: 'end', label: 'Fin' } },
]
const edges = [
  { source: 't', target: 'h' }, { source: 'h', target: 'c' },
  { source: 'c', target: 'f', sourceHandle: 'yes' }, { source: 'f', target: 'e' }, { source: 'c', target: 'e', sourceHandle: 'no' },
]

describe('buildCode', () => {
  it.each(CODE_LANGS.map(l => l.key))('%s: one function per block and a run() that follows the arrows', lang => {
    const code = buildCode(nodes as any, edges as any, lang, 'demo')
    expect(code.length).toBeGreaterThan(200)
    expect(code.toLowerCase()).toContain('run')
    // the order of the calls follows the flow: trigger -> http -> condition -> function
    const names = lang === 'python' || lang === 'rust' ? ['nuevo_pedido', 'consultar_cliente', 'es_vip', 'cobrar'] : ['nuevoPedido', 'consultarCliente', 'esVip', 'cobrar']
    const runPart = code.slice(code.search(/run\(|def run|fn run|Run\(/))
    const positions = names.map(n => runPart.indexOf(n))
    expect(positions.every(p => p >= 0)).toBe(true)
    expect([...positions].sort((a, b) => a - b)).toEqual(positions)
  })
  it('Python keeps blocks valid (a comment alone is not a statement)', () => {
    const code = buildCode(nodes as any, edges as any, 'python', 'demo')
    expect(code).toMatch(/else:\n\s+# Fin del flujo\n\s+pass/)
  })
  it('Qt C++ builds real requests from the HTTP block, reading {{vars}} from the context', () => {
    const code = buildCode(nodes as any, edges as any, 'qtcpp', 'demo')
    expect(code).toContain('httpRequest("GET"')
    expect(code).toContain('ctx.value("cliente").toObject().value("id")')
    expect(code).toContain('"Authorization"')
  })
  it('QML/JS uses callbacks (no async/await in QML)', () => {
    const code = buildCode(nodes as any, edges as any, 'qml', 'demo')
    expect(code).toContain('.pragma library')
    expect(code).not.toContain('await')
    expect(code).toContain('function(ctx) {')
  })
})
