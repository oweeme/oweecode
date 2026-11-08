import { describe, expect, it } from 'vitest'
import { flowLayout } from '../src/composables/flowSnapshot'

const node = (id: string, x: number, y: number, kind = 'function', extra: any = {}) => ({ id, position: { x, y }, data: { kind, label: id, desc: '' }, ...extra })

describe('flow snapshot layout', () => {
  it('moves the flow to the top-left corner with padding', () => {
    const L = flowLayout([node('a', 500, 300), node('b', 900, 420)], [])
    expect(L.boxes[0]).toMatchObject({ x: 40, y: 40 })
    expect(L.boxes[1]).toMatchObject({ x: 440, y: 160 })
    expect(L.width).toBe(440 + 200 + 40)
  })

  it('connects the right side of the source to the left side of the target', () => {
    const L = flowLayout([node('a', 0, 0), node('b', 300, 0)], [{ source: 'a', target: 'b' }])
    expect(L.links[0].from).toEqual({ x: 240, y: 55 })
    expect(L.links[0].to).toEqual({ x: 340, y: 55 })
  })

  it('uses the yes/no outputs of a condition block and keeps their label', () => {
    const c = node('c', 0, 0, 'condition', { dimensions: { width: 200, height: 100 } })
    const L = flowLayout([c, node('y', 300, 0), node('n', 300, 200)], [
      { source: 'c', target: 'y', sourceHandle: 'yes', label: 'Sí' },
      { source: 'c', target: 'n', sourceHandle: 'no', label: 'No' },
    ])
    expect(L.links.map((l) => l.from.y)).toEqual([40 + 30, 40 + 75])
    expect(L.links.map((l) => l.label)).toEqual(['Sí', 'No'])
  })

  it('skips hidden blocks (project map filters) and their connections', () => {
    const L = flowLayout([node('a', 0, 0), node('b', 300, 0, 'store', { hidden: true })], [{ source: 'a', target: 'b' }])
    expect(L.boxes.map((b) => b.id)).toEqual(['a'])
    expect(L.links).toEqual([])
  })

  it('returns an empty layout for an empty flow', () => {
    expect(flowLayout([], []).boxes).toEqual([])
  })
})
