import { describe, expect, it } from 'vitest'
import { baseName, enterAction, formatBytes, joinPath, nextIndex, parentPath, rangeSet, summarize, typeAheadIndex, type NavItem } from '../src/composables/fileNav'

const it_ = (name: string, is_dir = false, size = 0): NavItem => ({ name, path: '/x/' + name, is_dir, size })
const items = [it_('admin', true), it_('app', true), it_('assets', true), it_('index.html', false, 756), it_('Zeta.php', false, 2048)]

describe('parentPath (Backspace)', () => {
  it('goes one level up and stops at the root', () => {
    expect(parentPath('/a/b/c')).toBe('/a/b')
    expect(parentPath('/a/b/')).toBe('/a')
    expect(parentPath('/a')).toBe('/')
    expect(parentPath('/')).toBe('/')
    expect(parentPath('')).toBe('/')
  })
  it('helpers', () => {
    expect(baseName('/a/b/c/')).toBe('c')
    expect(joinPath('/a', 'b')).toBe('/a/b')
    expect(joinPath('/', 'b')).toBe('/b')
  })
})

describe('movement keys', () => {
  it('moves and clamps', () => {
    expect(nextIndex('ArrowDown', 0, 5)).toBe(1)
    expect(nextIndex('ArrowDown', 4, 5)).toBe(4)
    expect(nextIndex('ArrowUp', 0, 5)).toBe(0)
    expect(nextIndex('Home', 3, 5)).toBe(0)
    expect(nextIndex('End', 0, 5)).toBe(4)
    expect(nextIndex('PageDown', 1, 50, 10)).toBe(11)
    expect(nextIndex('PageUp', 3, 50, 10)).toBe(0)
    expect(nextIndex('x', 0, 5)).toBeNull()
    expect(nextIndex('ArrowDown', 0, 0)).toBeNull()
  })
  it('starts from the first item when nothing is focused', () => expect(nextIndex('ArrowDown', -1, 5)).toBe(1))
})

describe('selection', () => {
  it('rangeSet is order independent and includes both ends', () => {
    expect([...rangeSet(items, 1, 3)]).toEqual(['/x/app', '/x/assets', '/x/index.html'])
    expect([...rangeSet(items, 3, 1)]).toEqual([...rangeSet(items, 1, 3)])
    expect(rangeSet(items, -5, 99).size).toBe(5)
  })
  it('summarize', () => {
    expect(summarize(items, new Set())).toBe('5 elementos: 3 carpetas, 2 archivos (2.7 KB)')
    expect(summarize(items, new Set(['/x/index.html', '/x/Zeta.php']))).toBe('2 de 5 seleccionados (2.7 KB)')
    expect(formatBytes(1536)).toBe('1.5 KB')
  })
})

describe('type-ahead', () => {
  it('jumps to the first name that starts with the typed text (case-insensitive)', () => {
    expect(typeAheadIndex(items, 'i', 0)).toBe(3)
    expect(typeAheadIndex(items, 'z', 0)).toBe(4)
    expect(typeAheadIndex(items, 'ass', 0)).toBe(2)
    expect(typeAheadIndex(items, 'q', 0)).toBe(-1)
  })
  it('a single letter cycles through matches', () => {
    expect(typeAheadIndex(items, 'a', 0)).toBe(1)   // after the current one (admin)
    expect(typeAheadIndex(items, 'a', 1)).toBe(2)
    expect(typeAheadIndex(items, 'a', 2)).toBe(0)   // wraps around
    expect(typeAheadIndex(items, 'aa', 0)).toBe(1)  // pressing the same letter again also cycles
  })
})

describe('Enter', () => {
  it('a single folder is entered, a file or several items are transferred', () => {
    expect(enterAction(items, new Set(), 0)).toMatchObject({ kind: 'enter' })
    expect(enterAction(items, new Set(['/x/index.html']), 3)).toMatchObject({ kind: 'transfer' })
    expect(enterAction(items, new Set(['/x/admin', '/x/app']), 0)).toMatchObject({ kind: 'transfer', entries: [{ name: 'admin' }, { name: 'app' }] })
    expect(enterAction([], new Set(), -1)).toEqual({ kind: 'none' })
  })
})
