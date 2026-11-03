// Folder synchronization between the local folder and the server one (what FileZilla calls a synchronized comparison).
// Files are compared by relative path and size; the plan lists what would be copied or removed so the user can review it first.

export interface SyncFile { rel: string; size: number }
export interface SyncCopy extends SyncFile { reason: 'new' | 'changed' }
export interface SyncPlan { copy: SyncCopy[]; remove: SyncFile[]; same: number }

export const DEFAULT_IGNORE = ['node_modules', '.git', '.DS_Store', 'Thumbs.db', '.owee.bak', '__pycache__', '.venv', 'target']

/** a name ("node_modules"), a suffix pattern ("*.log") or a relative path ("build/cache") matched against every path segment */
export function isIgnored(rel: string, patterns: string[]): boolean {
  const segs = rel.split('/').filter(Boolean)
  return patterns.some(p => {
    if (!p) return false
    if (p.startsWith('*.')) return segs[segs.length - 1]?.endsWith(p.slice(1)) ?? false
    if (p.includes('/')) return rel === p || rel.startsWith(p + '/')
    return segs.includes(p)
  })
}

export function planSync(source: SyncFile[], dest: SyncFile[], opts: { deleteExtra?: boolean; ignore?: string[] } = {}): SyncPlan {
  const ignore = opts.ignore ?? []
  const keep = (f: SyncFile) => !isIgnored(f.rel, ignore)
  const src = source.filter(keep)
  const dst = new Map(dest.filter(keep).map(f => [f.rel, f.size]))
  const plan: SyncPlan = { copy: [], remove: [], same: 0 }
  const seen = new Set<string>()
  for (const f of src) {
    seen.add(f.rel)
    const size = dst.get(f.rel)
    if (size === undefined) plan.copy.push({ ...f, reason: 'new' })
    else if (size !== f.size) plan.copy.push({ ...f, reason: 'changed' })
    else plan.same++
  }
  if (opts.deleteExtra) for (const [rel, size] of dst) if (!seen.has(rel)) plan.remove.push({ rel, size })
  const byRel = (a: SyncFile, b: SyncFile) => a.rel.localeCompare(b.rel)
  plan.copy.sort(byRel); plan.remove.sort(byRel)
  return plan
}

/** folders that must exist at the destination before the files go in, shallowest first */
export function neededDirs(files: SyncFile[], base: string): string[] {
  const dirs = new Set<string>()
  for (const f of files) {
    const parts = f.rel.split('/').slice(0, -1)
    for (let i = 1; i <= parts.length; i++) dirs.add(base + '/' + parts.slice(0, i).join('/'))
  }
  return [...dirs].sort((a, b) => a.split('/').length - b.split('/').length || a.localeCompare(b))
}
