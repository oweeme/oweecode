import { describe, expect, it } from 'vitest'
import { buildSql, parseColumns } from '../src/composables/flowSeed'

const nodes = [
  { data: { kind: 'db', label: 'Pedidos', columns: 'cliente_id int\ntotal decimal\nestado text\nfecha date' } },
  { data: { kind: 'db', label: 'Clientes', columns: 'nombre text\nemail text unique\nactivo bool' } },
]

describe('buildSql', () => {
  it('creates referenced tables first and seeds the requested rows', () => {
    const { sql, tables } = buildSql(nodes as any, 'sqlite', 7, 'demo')
    expect(tables).toBe(2)
    expect(sql.indexOf('CREATE TABLE IF NOT EXISTS clientes')).toBeLessThan(sql.indexOf('CREATE TABLE IF NOT EXISTS pedidos'))
    expect((sql.match(/INSERT INTO clientes/g) ?? []).length).toBe(7)
  })
  it('is deterministic', () => {
    expect(buildSql(nodes as any, 'mysql', 5, 'x').sql).toBe(buildSql(nodes as any, 'mysql', 5, 'x').sql)
  })
  it('adapts types to the dialect', () => {
    expect(buildSql(nodes as any, 'postgres', 1, 'x').sql).toContain('SERIAL PRIMARY KEY')
    expect(buildSql(nodes as any, 'mysql', 1, 'x').sql).toContain('AUTO_INCREMENT')
    expect(buildSql(nodes as any, 'sqlite', 1, 'x').sql).toContain('AUTOINCREMENT')
  })
  it('parses columns', () => {
    expect(parseColumns('id pk\nemail text unique\ncliente_id int')).toMatchObject([{ pk: true }, { unique: true }, { fk: 'cliente' }])
  })
})
