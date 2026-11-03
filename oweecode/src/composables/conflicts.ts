// What to do when a file to transfer already exists at the destination (FileZilla's "file already exists" dialog).
export interface XferFile { src: string; dst: string; size: number }
export interface Conflict { file: XferFile; dstSize: number; sameSize: boolean }
export type Policy = 'overwrite' | 'skip' | 'different' | 'rename'
/** destination folder -> (file name -> size), as listed from the destination side */
export type Existing = Map<string, Map<string, number>>

export const dirOf = (p: string) => { const i = p.lastIndexOf('/'); return i <= 0 ? '/' : p.slice(0, i) }
export const nameOf = (p: string) => p.slice(p.lastIndexOf('/') + 1)

export function destinationDirs(files: XferFile[]): string[] { return [...new Set(files.map(f => dirOf(f.dst)))] }

export function findConflicts(files: XferFile[], existing: Existing): Conflict[] {
  const out: Conflict[] = []
  for (const f of files) {
    const size = existing.get(dirOf(f.dst))?.get(nameOf(f.dst))
    if (size !== undefined) out.push({ file: f, dstSize: size, sameSize: size === f.size })
  }
  return out
}

/** "a.txt" -> "a (1).txt", "a (1).txt" if that is taken -> "a (2).txt"; dotfiles and extensionless names handled */
export function renameFree(name: string, taken: Set<string>): string {
  const dot = name.lastIndexOf('.')
  const stem = dot > 0 ? name.slice(0, dot) : name, ext = dot > 0 ? name.slice(dot) : ''
  for (let n = 1; n < 10000; n++) { const c = `${stem} (${n})${ext}`; if (!taken.has(c)) return c }
  return `${stem} (${Date.now()})${ext}`
}

export function applyPolicy(files: XferFile[], conflicts: Conflict[], policy: Policy, existing: Existing): XferFile[] {
  const conflicting = new Map(conflicts.map(c => [c.file.dst, c]))
  const result: XferFile[] = []
  const taken = new Map<string, Set<string>>()
  for (const f of files) {
    const c = conflicting.get(f.dst)
    if (!c || policy === 'overwrite') { result.push(f); continue }
    if (policy === 'skip') continue
    if (policy === 'different') { if (!c.sameSize) result.push(f); continue }
    // rename: pick a free name in that folder (also avoiding names this same batch already took)
    const dir = dirOf(f.dst)
    if (!taken.has(dir)) taken.set(dir, new Set(existing.get(dir)?.keys() ?? []))
    const t = taken.get(dir)!
    const fresh = renameFree(nameOf(f.dst), t); t.add(fresh)
    result.push({ ...f, dst: (dir === '/' ? '' : dir) + '/' + fresh })
  }
  return result
}
