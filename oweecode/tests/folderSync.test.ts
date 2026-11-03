import { describe, expect, it } from 'vitest'
import { DEFAULT_IGNORE, isIgnored, neededDirs, planSync } from '../src/composables/folderSync'

const f = (rel: string, size = 1) => ({ rel, size })

describe('folder sync plan', () => {
  it('copies new and changed files, leaves equal ones, never touches extras by default', () => {
    const p = planSync([f('a.txt', 1), f('b.txt', 2), f('c/d.txt', 3)], [f('a.txt', 1), f('b.txt', 9), f('old.txt', 4)])
    expect(p.copy).toEqual([{ rel: 'b.txt', size: 2, reason: 'changed' }, { rel: 'c/d.txt', size: 3, reason: 'new' }])
    expect(p.same).toBe(1)
    expect(p.remove).toEqual([])
  })
  it('lists what exists only at the destination when mirroring', () => {
    const p = planSync([f('a')], [f('a'), f('x/y', 5)], { deleteExtra: true })
    expect(p.remove).toEqual([{ rel: 'x/y', size: 5 }])
  })
  it('ignores node_modules, .git and *.log on both sides (an ignored extra is not deleted)', () => {
    const ignore = [...DEFAULT_IGNORE, '*.log']
    const p = planSync([f('src/a.js'), f('node_modules/x/i.js'), f('debug.log')], [f('.git/HEAD'), f('src/a.js')], { deleteExtra: true, ignore })
    expect(p.copy).toEqual([]); expect(p.remove).toEqual([]); expect(p.same).toBe(1)
  })
  it('matches ignore patterns by segment, suffix and path prefix only', () => {
    expect(isIgnored('a/node_modules/b.js', ['node_modules'])).toBe(true)
    expect(isIgnored('a/my_node_modules/b.js', ['node_modules'])).toBe(false)
    expect(isIgnored('x/y.log', ['*.log'])).toBe(true)
    expect(isIgnored('build/cache/z', ['build/cache'])).toBe(true)
    expect(isIgnored('build/cached/z', ['build/cache'])).toBe(false)
  })
  it('creates parent folders shallowest first', () => {
    expect(neededDirs([f('a/b/c.txt'), f('a/d.txt'), f('e.txt')], '/www')).toEqual(['/www/a', '/www/a/b'])
  })
})
