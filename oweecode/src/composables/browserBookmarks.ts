// Bookmarks of the Browser preview, one list per project (so "http://localhost:9000/admin" is one click away next time).

export interface Bookmark { url: string; title: string }

const key = (root: string) => `browser_bm:${root || '_'}`

/** `localhost:9000/x/` and `http://localhost:9000/x` are the same favorite */
export function normalizeUrl(raw: string): string {
  const u = raw.trim()
  if (!u) return ''
  try { const x = new URL(/^https?:\/\//i.test(u) ? u : `http://${u}`); return x.toString().replace(/\/$/, '') } catch { return u }
}

export function labelFor(url: string): string {
  try { const x = new URL(url); return x.host + (x.pathname !== '/' ? x.pathname : '') } catch { return url }
}

export function loadBookmarks(root: string, storage: Pick<Storage, 'getItem'> = localStorage): Bookmark[] {
  try {
    const v = JSON.parse(storage.getItem(key(root)) ?? '[]')
    return Array.isArray(v) ? v.filter(b => b && typeof b.url === 'string').map(b => ({ url: b.url, title: String(b.title ?? labelFor(b.url)) })) : []
  } catch { return [] }
}

export function saveBookmarks(root: string, list: Bookmark[], storage: Pick<Storage, 'setItem'> = localStorage): void {
  try { storage.setItem(key(root), JSON.stringify(list)) } catch { /* private window or full storage: the list just won't persist */ }
}

export const isBookmarked = (list: Bookmark[], url: string) => !!url && list.some(b => normalizeUrl(b.url) === normalizeUrl(url))

/** adds the page, or removes it when it is already there */
export function toggleBookmark(list: Bookmark[], url: string, title?: string): Bookmark[] {
  const n = normalizeUrl(url)
  if (!n) return list
  if (isBookmarked(list, n)) return list.filter(b => normalizeUrl(b.url) !== n)
  return [...list, { url: n, title: title?.trim() || labelFor(n) }]
}
