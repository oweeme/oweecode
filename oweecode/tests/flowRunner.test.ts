import { describe, expect, it } from 'vitest'
import { evalCondition, fill, fillSql, isReadQuery, mergeOutput } from '../src/composables/flowRunner'

describe('evalCondition (no eval: safe for flows from other people)', () => {
  it('compares numbers, strings and booleans', () => {
    expect(evalCondition('total > 100 && estado == "ok"', { total: 150, estado: 'ok' })).toBe(true)
    expect(evalCondition('total > 100 && estado == "ok"', { total: 50, estado: 'ok' })).toBe(false)
    expect(evalCondition('http.body.vip == true', { http: { body: { vip: true } } })).toBe(true)
  })
  it('supports negation, truthiness and ||', () => {
    expect(evalCondition('!paid', { paid: false })).toBe(true)
    expect(evalCondition('a.b >= 3 || x', { a: { b: 1 }, x: 0 })).toBe(false)
    expect(evalCondition('user.active', { user: { active: 1 } })).toBe(true)
  })
  it('never executes code', () => {
    const w = globalThis as any
    w.__pwned = false
    evalCondition('(w.__pwned = true)', {})
    evalCondition('constructor.constructor("globalThis.__pwned=true")()', {})
    expect(w.__pwned).toBe(false)
  })
})

describe('fill / fillSql', () => {
  it('inserts nested values', () => expect(fill('hola {{user.name}} {{n}}', { user: { name: 'Ana' }, n: 3 })).toBe('hola Ana 3'))
  it('quotes and escapes SQL values so they cannot break the query', () => {
    expect(fillSql("x = {{n}} AND s = {{s}} AND z = {{z}}", { n: 5, s: "O'Brien", z: null })).toBe("x = 5 AND s = 'O''Brien' AND z = NULL")
    expect(fillSql('s = {{s}}', { s: "'; DROP TABLE a; --" })).toBe("s = '''; DROP TABLE a; --'")
  })
  it('doubles backslashes only for MySQL', () => {
    expect(fillSql('s = {{s}}', { s: 'a\\b' }, true)).toBe("s = 'a\\\\b'")
    expect(fillSql('s = {{s}}', { s: 'a\\b' }, false)).toBe("s = 'a\\b'")
  })
  it('tells reads from writes', () => {
    expect(isReadQuery(' select 1')).toBe(true)
    expect(isReadQuery('DELETE FROM a')).toBe(false)
  })
})

describe('mergeOutput', () => {
  it('merges a JSON object and keeps plain text in output', () => {
    expect(mergeOutput({ a: 1 }, '{"b":2}')).toEqual({ a: 1, b: 2 })
    expect(mergeOutput({ a: 1 }, 'hola\n')).toEqual({ a: 1, output: 'hola' })
  })
})
