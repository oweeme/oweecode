// Helpers for running a flow: safe condition evaluation (no eval / new Function,
// so a flow file from someone else can't execute code in the app) and
// {{placeholder}} substitution from the running context.

export type Ctx = Record<string, any>

export function getPath(ctx: Ctx, path: string): any {
  const p = path.replace(/^ctx\./, '').replace(/\[(\d+)\]/g, '.$1')
  return p.split('.').filter(Boolean).reduce((o: any, k) => (o == null ? undefined : o[k]), ctx)
}

function literal(raw: string, ctx: Ctx): any {
  const v = raw.trim()
  if (/^-?\d+(\.\d+)?$/.test(v)) return Number(v)
  if (v === 'true') return true
  if (v === 'false') return false
  if (v === 'null') return null
  const q = v.match(/^(['"])(.*)\1$/)
  if (q) return q[2]
  if (/^[\w.\[\]]+$/.test(v) && getPath(ctx, v) !== undefined) return getPath(ctx, v)
  return v
}

/** Supports `a > 5`, `status == "ok"`, `!paid`, `user.active`, joined with && / ||. */
export function evalCondition(expr: string, ctx: Ctx): boolean {
  const clause = (c: string): boolean => {
    const m = c.trim().match(/^(!?)\s*([\w.\[\]]+)\s*(==|!=|>=|<=|>|<)?\s*(.*)$/)
    if (!m) return false
    const [, neg, left, op, right] = m
    const l = getPath(ctx, left)
    let res: boolean
    if (!op) res = !!l
    else {
      const r = literal(right, ctx)
      // eslint-disable-next-line eqeqeq
      res = op === '==' ? l == r : op === '!=' ? l != r : op === '>' ? l > r : op === '<' ? l < r : op === '>=' ? l >= r : l <= r
    }
    return neg ? !res : res
  }
  return expr.split('||').some(or => or.split('&&').every(clause))
}

export function fill(template: string, ctx: Ctx): string {
  return String(template ?? '').replace(/\{\{\s*([\w.\[\]]+)\s*\}\}/g, (_, p) => {
    const v = getPath(ctx, p)
    return v == null ? '' : typeof v === 'object' ? JSON.stringify(v) : String(v)
  })
}

export const RUN_LANGS: { key: string; label: string; template: string }[] = [
  { key: 'node', label: 'Node.js', template: 'node {file}' },
  { key: 'ts', label: 'TypeScript (tsx)', template: 'npx tsx {file}' },
  { key: 'python', label: 'Python', template: 'python3 {file}' },
  { key: 'php', label: 'PHP', template: 'php {file}' },
  { key: 'go', label: 'Go', template: 'go run {file}' },
  { key: 'rust', label: 'Rust (cargo)', template: 'cargo run --quiet' },
  { key: 'dart', label: 'Dart', template: 'dart run {file}' },
  { key: 'shell', label: 'Shell', template: 'sh {file}' },
  { key: 'custom', label: 'Comando propio', template: '' },
]

/** Merges a step's stdout into the context: JSON objects are merged, text goes to ctx.output. */
export function mergeOutput(ctx: Ctx, stdout: string): Ctx {
  const t = stdout.trim()
  if (t.startsWith('{')) {
    try { const o = JSON.parse(t); if (o && typeof o === 'object') return { ...ctx, ...o } } catch { /* fall through */ }
  }
  return t ? { ...ctx, output: t } : ctx
}

/** Like fill(), but values become SQL literals (quoted/escaped), so {{name}} can't break out of the query. */
export function fillSql(sql: string, ctx: Ctx, mysql = false): string {
  // MySQL treats backslash as an escape inside strings; SQLite/Postgres don't
  const esc = (t: string) => (mysql ? t.replace(/\\/g, '\\\\') : t).replace(/'/g, "''")
  return String(sql ?? '').replace(/\{\{\s*([\w.\[\]]+)\s*\}\}/g, (_, p) => {
    const v = getPath(ctx, p)
    if (v == null) return 'NULL'
    if (typeof v === 'number' && Number.isFinite(v)) return String(v)
    if (typeof v === 'boolean') return v ? '1' : '0'
    return `'${esc(typeof v === 'object' ? JSON.stringify(v) : String(v))}'`
  })
}

export const isReadQuery = (sql: string) => /^\s*(select|show|pragma|with|describe|explain)\b/i.test(sql)
