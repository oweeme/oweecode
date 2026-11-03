// Keyboard/selection rules for the FTP file panes, modelled on FileZilla. Pure functions, so they can be tested.

export interface NavItem { name: string; path: string; is_dir: boolean; size: number }

/** "/a/b/c" -> "/a/b", "/a" -> "/", "/" -> "/" (also "/a/b/" and "C:/x" style are handled) */
export function parentPath(p: string): string {
  const t = p.replace(/\/+$/, '')
  if (!t) return '/'
  const cut = t.lastIndexOf('/')
  if (cut < 0) return '/'
  if (cut === 0) return '/'
  const parent = t.slice(0, cut)
  return /^[A-Za-z]:$/.test(parent) ? parent + '/' : parent
}
export const baseName = (p: string) => p.replace(/\/+$/, '').split('/').pop() ?? ''
export const joinPath = (base: string, name: string) => (base.endsWith('/') ? base : base + '/') + name

/** Where the focus goes for ↑ ↓ Home End PageUp PageDown (null if the key is not a movement key) */
export function nextIndex(key: string, idx: number, len: number, page = 10): number | null {
  if (len <= 0) return null
  const cur = idx < 0 ? 0 : idx
  switch (key) {
    case 'ArrowDown': return Math.min(cur + 1, len - 1)
    case 'ArrowUp': return Math.max(cur - 1, 0)
    case 'Home': return 0
    case 'End': return len - 1
    case 'PageDown': return Math.min(cur + Math.max(1, page), len - 1)
    case 'PageUp': return Math.max(cur - Math.max(1, page), 0)
    default: return null
  }
}

/** Paths of the items between two indexes, both included (Shift+click / Shift+arrows) */
export function rangeSet(items: NavItem[], a: number, b: number): Set<string> {
  const lo = Math.max(0, Math.min(a, b)), hi = Math.min(items.length - 1, Math.max(a, b))
  const s = new Set<string>()
  for (let i = lo; i <= hi; i++) s.add(items[i].path)
  return s
}

/** Type-ahead: the first item whose name starts with what was typed. A repeated single letter cycles through matches. */
export function typeAheadIndex(items: NavItem[], buffer: string, from: number): number {
  if (!buffer) return -1
  const b = buffer.toLowerCase()
  const n = items.length
  const repeated = b.length > 1 && b.split('').every(c => c === b[0])
  const prefix = repeated ? b[0] : b
  const start = prefix.length === 1 ? from + 1 : Math.max(from, 0)   // one letter: next match after the current one
  for (let k = 0; k < n; k++) {
    const i = (start + k) % n
    if (items[i].name.toLowerCase().startsWith(prefix)) return i
  }
  return -1
}

export type EnterAction = { kind: 'enter'; entry: NavItem } | { kind: 'transfer'; entries: NavItem[] } | { kind: 'none' }

/** Enter: several selected, or a file -> transfer (like FileZilla); a single folder -> go into it */
export function enterAction(items: NavItem[], selected: Set<string>, focusIdx: number): EnterAction {
  const sel = items.filter(i => selected.has(i.path))
  if (sel.length > 1) return { kind: 'transfer', entries: sel }
  const target = items[focusIdx] ?? sel[0]
  if (!target) return { kind: 'none' }
  return target.is_dir ? { kind: 'enter', entry: target } : { kind: 'transfer', entries: [target] }
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`
  if (n < 1048576) return `${(n / 1024).toFixed(1)} KB`
  if (n < 1073741824) return `${(n / 1048576).toFixed(1)} MB`
  return `${(n / 1073741824).toFixed(2)} GB`
}

/** "3 de 20 elementos seleccionados (1.2 MB)" / "20 elementos: 14 carpetas, 6 archivos (3.1 MB)" */
export type Tr = (key: 'fSelOf' | 'fSummary', params: Record<string, string | number>) => string

/** `tr` translates the sentence; without it the text is Spanish (the tests rely on that) */
export function summarize(items: NavItem[], selected: Set<string>, tr?: Tr): string {
  const sel = items.filter(i => selected.has(i.path))
  if (sel.length) {
    const bytes = sel.filter(i => !i.is_dir).reduce((a, i) => a + i.size, 0)
    const head = tr ? tr('fSelOf', { n: sel.length, total: items.length }) : `${sel.length} de ${items.length} seleccionados`
    return `${head}${bytes ? ` (${formatBytes(bytes)})` : ''}`
  }
  const dirs = items.filter(i => i.is_dir).length
  const bytes = items.filter(i => !i.is_dir).reduce((a, i) => a + i.size, 0)
  const head = tr ? tr('fSummary', { n: items.length, dirs, files: items.length - dirs }) : `${items.length} elementos: ${dirs} carpetas, ${items.length - dirs} archivos`
  return `${head}${bytes ? ` (${formatBytes(bytes)})` : ''}`
}
