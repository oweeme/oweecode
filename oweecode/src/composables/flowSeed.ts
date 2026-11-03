// Builds CREATE TABLE + INSERT sample rows from the "Base de datos" blocks.
export type Dialect = 'sqlite' | 'mysql' | 'postgres'

interface Col { name: string; type: string; pk: boolean; unique: boolean; fk?: string }
interface DbNode { data: { kind: string; label: string; columns?: string } }

// One column per line: `nombre text`, `email text unique`, `cliente_id int`, `precio decimal`
export function parseColumns(text: string | undefined): Col[] {
  const cols: Col[] = []
  for (const raw of (text ?? '').split('\n')) {
    const parts = raw.trim().split(/[\s,]+/).filter(Boolean)
    if (!parts.length) continue
    const name = parts[0].replace(/[^\w]/g, '')
    if (!name) continue
    const rest = parts.slice(1).map(x => x.toLowerCase())
    const typeWord = rest.find(x => !['pk', 'unique', 'primary', 'key', 'not', 'null'].includes(x)) ?? 'text'
    const pk = rest.includes('pk') || rest.includes('primary') || name.toLowerCase() === 'id'
    const fk = !pk && /_id$/i.test(name) ? name.replace(/_id$/i, '') : undefined
    cols.push({ name, type: pk || fk ? 'int' : typeWord, pk, unique: rest.includes('unique'), fk })
  }
  return cols
}

function sqlType(c: Col, d: Dialect): string {
  const t = c.type.toLowerCase()
  if (c.pk) return d === 'sqlite' ? 'INTEGER PRIMARY KEY AUTOINCREMENT' : d === 'mysql' ? 'INT AUTO_INCREMENT PRIMARY KEY' : 'SERIAL PRIMARY KEY'
  if (/^(int|integer|number|entero)/.test(t)) return 'INTEGER'
  if (/^(dec|float|double|money|numeric|real)/.test(t)) return d === 'sqlite' ? 'REAL' : 'DECIMAL(12,2)'
  if (/^(bool|bit)/.test(t)) return d === 'postgres' ? 'BOOLEAN' : 'INTEGER'
  if (t === 'datetime' || t === 'timestamp') return d === 'postgres' ? 'TIMESTAMP' : 'DATETIME'
  if (t.startsWith('date')) return 'DATE'
  return d === 'mysql' ? 'VARCHAR(255)' : 'TEXT'
}

// Small deterministic PRNG so the same flow always yields the same sample data.
function rng(seed: number) { let a = seed; return () => { a |= 0; a = a + 0x6D2B79F5 | 0; let t = Math.imul(a ^ a >>> 15, 1 | a); t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t; return ((t ^ t >>> 14) >>> 0) / 4294967296 } }
const NOMBRES = ['Ana', 'Luis', 'María', 'Carlos', 'Lucía', 'Jorge', 'Sofía', 'Diego', 'Valeria', 'Pedro', 'Camila', 'Andrés']
const APELLIDOS = ['Quispe', 'Mamani', 'Rojas', 'Flores', 'Vargas', 'Gutiérrez', 'Pérez', 'Condori', 'Suárez', 'Mendoza']
const CIUDADES = ['La Paz', 'Santa Cruz', 'Cochabamba', 'Sucre', 'Oruro', 'Tarija', 'Potosí', 'El Alto']
const ESTADOS = ['pendiente', 'activo', 'completado', 'cancelado']
const PRODUCTOS = ['Camiseta', 'Taza', 'Cuaderno', 'Mochila', 'Teclado', 'Mouse', 'Auriculares', 'Lámpara']

function sample(c: Col, i: number, n: number, r: () => number, table: string, d: Dialect): string {
  const pick = <T,>(a: T[]) => a[Math.floor(r() * a.length)]
  const q = (v: string) => `'${v.replace(/'/g, "''")}'`
  const k = c.name.toLowerCase(), t = c.type.toLowerCase()
  if (c.fk) return String(1 + Math.floor(r() * n))
  if (/^(bool|bit)/.test(t) || /^(activo|active|enabled|habilitado|is_|es_)/.test(k)) return d === 'postgres' ? (r() > 0.3 ? 'TRUE' : 'FALSE') : (r() > 0.3 ? '1' : '0')
  if (/email|correo/.test(k)) return q(`${pick(NOMBRES).toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '')}${i + 1}@ejemplo.com`)
  if (/tel|phone|celular|cel/.test(k)) return q(`7${Math.floor(1000000 + r() * 8999999)}`)
  if (/apellido|last/.test(k)) return q(pick(APELLIDOS))
  if (/^(nombre|name|first)/.test(k) || k === 'usuario' || k === 'user') return q(/producto|item|articulo/i.test(table) ? pick(PRODUCTOS) : `${pick(NOMBRES)} ${pick(APELLIDOS)}`)
  if (/ciudad|city/.test(k)) return q(pick(CIUDADES))
  if (/direcc|address/.test(k)) return q(`Av. ${pick(APELLIDOS)} #${Math.floor(r() * 900 + 10)}`)
  if (/estado|status/.test(k)) return q(pick(ESTADOS))
  if (/desc|nota|obs|comment/.test(k)) return q(`Registro de ejemplo ${i + 1}`)
  if (t.startsWith('date') || t === 'timestamp' || /fecha|date|created|creado/.test(k)) {
    const iso = new Date(2026, 0, 1 + Math.floor(r() * 300), Math.floor(r() * 24), Math.floor(r() * 60)).toISOString()
    return q(/^date$/.test(t) ? iso.slice(0, 10) : iso.slice(0, 19).replace('T', ' '))
  }
  if (/^(dec|float|double|money|numeric|real)/.test(t) || /precio|price|monto|total|costo|importe/.test(k)) return (5 + r() * 495).toFixed(2)
  if (/^(int|integer|number|entero)/.test(t) || /cantidad|stock|qty|edad|age/.test(k)) return String(1 + Math.floor(r() * 50))
  return q(`${c.name} ${i + 1}`)
}

export function buildSql(nodes: DbNode[], dialect: Dialect, rows: number, flowName: string): { sql: string; tables: number } {
  const dbNodes = nodes.filter(n => n.data.kind === 'db' && parseColumns(n.data.columns).length)
  const tn = (n: DbNode) => String(n.data.label).trim().toLowerCase().replace(/[^\w]+/g, '_').replace(/^_+|_+$/g, '') || 'tabla'
  // tables referenced by foreign keys must be created first
  const sorted = [...dbNodes].sort((a, b) => {
    const fa = parseColumns(a.data.columns).some(c => c.fk && tn(b).startsWith(c.fk.toLowerCase()))
    const fb = parseColumns(b.data.columns).some(c => c.fk && tn(a).startsWith(c.fk.toLowerCase()))
    return fa === fb ? 0 : fa ? 1 : -1
  })
  const out: string[] = [`-- Generado por OweeCode (${dialect}) — flujo "${flowName}"`, '']
  const n = Math.max(1, Math.min(500, rows || 10))
  const r = rng(42)
  for (const node of sorted) {
    const table = tn(node), cols = parseColumns(node.data.columns)
    if (!cols.some(c => c.pk)) cols.unshift({ name: 'id', type: 'int', pk: true, unique: false })
    out.push(`CREATE TABLE IF NOT EXISTS ${table} (`)
    out.push(cols.map(c => `  ${c.name} ${sqlType(c, dialect)}${c.unique ? ' UNIQUE' : ''}`).join(',\n'))
    out.push(');', '')
  }
  for (const node of sorted) {
    const table = tn(node), cols = parseColumns(node.data.columns).filter(c => !c.pk)
    if (!cols.length) continue
    out.push(`-- Datos de ejemplo: ${table}`)
    for (let i = 0; i < n; i++) {
      out.push(`INSERT INTO ${table} (${cols.map(c => c.name).join(', ')}) VALUES (${cols.map(c => sample(c, i, n, r, table, dialect)).join(', ')});`)
    }
    out.push('')
  }
  return { sql: out.join('\n'), tables: sorted.length }
}
