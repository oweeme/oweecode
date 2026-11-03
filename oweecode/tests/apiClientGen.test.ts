import { describe, expect, it } from 'vitest'
import { generateClient } from '../src/composables/apiClientGen'

const coll = { name: 'Eseni API', requests: [
  { name: 'Listar clientes', method: 'GET', url: 'https://api.eseni.com/v1/clientes', headers: [], params: [{ key: 'page', value: '{{page}}', enabled: true }], bodyType: 'none', body: '', auth: { type: 'bearer' } },
  { name: 'Crear pedido', method: 'POST', url: 'https://api.eseni.com/v1/pedidos', headers: [], params: [], bodyType: 'json', body: '{"cliente":"{{clienteId}}"}', auth: { type: 'api-key', apiKeyName: 'X-Key', apiKeyIn: 'header' } },
] }

describe('generateClient', () => {
  it('turns requests into methods whose {{placeholders}} are parameters (names keep their case)', () => {
    const ts = generateClient(coll as any, 'ts')[0]
    expect(ts.path).toBe('src/api/EseniApiClient.ts')
    expect(ts.content).toContain('listarClientes(page: string | number)')
    expect(ts.content).toContain('crearPedido(clienteId: string | number)')
    expect(ts.content).toContain('baseUrl = "https://api.eseni.com"')
  })
  it('Qt C++ is a header-only class with async callbacks', () => {
    const h = generateClient(coll as any, 'qtcpp')[0].content
    expect(h).toContain('class EseniApiClient : public QObject')
    expect(h).toContain('void listarClientes(const QString &page, Callback cb)')
    expect(h).toContain('#pragma once')
  })
  it('QML is a .pragma library module', () => expect(generateClient(coll as any, 'qml')[0].content).toContain('.pragma library'))
  it('an empty collection generates nothing', () => expect(generateClient({ name: 'x', requests: [] } as any, 'ts')).toEqual([]))
})
