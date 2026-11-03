import { describe, expect, it } from 'vitest'
import { applyPolicy, destinationDirs, findConflicts, renameFree, type Existing, type XferFile } from '../src/composables/conflicts'

const f = (dst: string, size = 100): XferFile => ({ src: '/local' + dst, dst, size })
const existing: Existing = new Map([['/www', new Map([['index.html', 100], ['app.js', 50]])], ['/www/css', new Map([['a.css', 10]])]])
const files = [f('/www/index.html', 100), f('/www/app.js', 999), f('/www/nuevo.txt'), f('/www/css/a.css', 10), f('/www/css/b.css')]

describe('conflicts', () => {
  it('finds only the files that already exist, with their sizes', () => {
    const c = findConflicts(files, existing)
    expect(c.map(x => x.file.dst)).toEqual(['/www/index.html', '/www/app.js', '/www/css/a.css'])
    expect(c.map(x => x.sameSize)).toEqual([true, false, true])
    expect(destinationDirs(files).sort()).toEqual(['/www', '/www/css'])
  })
  const c = findConflicts(files, existing)
  it('overwrite keeps everything', () => expect(applyPolicy(files, c, 'overwrite', existing)).toHaveLength(5))
  it('skip leaves out the existing ones', () => expect(applyPolicy(files, c, 'skip', existing).map(x => x.dst)).toEqual(['/www/nuevo.txt', '/www/css/b.css']))
  it('"different" re-sends only the files whose size changed', () => expect(applyPolicy(files, c, 'different', existing).map(x => x.dst)).toEqual(['/www/app.js', '/www/nuevo.txt', '/www/css/b.css']))
  it('rename writes next to the original without touching it', () => {
    const out = applyPolicy(files, c, 'rename', existing).map(x => x.dst)
    expect(out).toEqual(['/www/index (1).html', '/www/app (1).js', '/www/nuevo.txt', '/www/css/a (1).css', '/www/css/b.css'])
  })
  it('rename skips names that are taken, also by the same batch', () => {
    const ex: Existing = new Map([['/', new Map([['a.txt', 1], ['a (1).txt', 1]])]])
    const batch = [f('/a.txt'), { src: '/other/a.txt', dst: '/a.txt', size: 1 }]
    const out = applyPolicy(batch, findConflicts(batch, ex), 'rename', ex).map(x => x.dst)
    expect(out).toEqual(['/a (2).txt', '/a (3).txt'])
  })
  it('renameFree', () => {
    expect(renameFree('x.tar.gz', new Set())).toBe('x.tar (1).gz')
    expect(renameFree('.env', new Set())).toBe('.env (1)')
    expect(renameFree('README', new Set(['README (1)']))).toBe('README (2)')
  })
})
