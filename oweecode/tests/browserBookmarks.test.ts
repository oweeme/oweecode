import { describe, expect, it } from 'vitest'
import { isBookmarked, labelFor, loadBookmarks, normalizeUrl, saveBookmarks, toggleBookmark } from '../src/composables/browserBookmarks'

const mem = () => { const m = new Map<string, string>(); return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v) } }

describe('browser bookmarks', () => {
  it('treats localhost:9000/ and http://localhost:9000 as the same page', () => {
    expect(normalizeUrl('localhost:9000/')).toBe('http://localhost:9000')
    expect(normalizeUrl('http://localhost:9000/admin/')).toBe('http://localhost:9000/admin')
    expect(normalizeUrl('  ')).toBe('')
  })
  it('toggles: adds, then removes the same page', () => {
    let l = toggleBookmark([], 'localhost:9000/admin')
    expect(l).toEqual([{ url: 'http://localhost:9000/admin', title: 'localhost:9000/admin' }])
    expect(isBookmarked(l, 'http://localhost:9000/admin/')).toBe(true)
    l = toggleBookmark(l, 'http://localhost:9000/admin')
    expect(l).toEqual([])
    expect(toggleBookmark([], '')).toEqual([])
  })
  it('keeps one list per project and survives broken storage', () => {
    const s = mem()
    saveBookmarks('/p/a', [{ url: 'http://localhost:80', title: 'web' }], s)
    expect(loadBookmarks('/p/a', s)).toHaveLength(1)
    expect(loadBookmarks('/p/b', s)).toEqual([])
    s.setItem('browser_bm:/p/c', '{not json')
    expect(loadBookmarks('/p/c', s)).toEqual([])
    expect(labelFor('http://localhost:8080/')).toBe('localhost:8080')
  })
})
