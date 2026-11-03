import { describe, expect, it } from 'vitest'
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { extraEn, extraEs } from '../src/composables/i18nExtra'
import { extraDe } from '../src/composables/i18nExtra.de'
import { extraFr } from '../src/composables/i18nExtra.fr'
import { extraKo } from '../src/composables/i18nExtra.ko'
import { extraZh } from '../src/composables/i18nExtra.zh'
import { extraJa } from '../src/composables/i18nExtra.ja'
import { extraRu } from '../src/composables/i18nExtra.ru'
import { useI18n } from '../src/composables/useI18n'

function walk(dir: string, out: string[] = []): string[] {
  for (const n of readdirSync(dir)) { const p = join(dir, n); statSync(p).isDirectory() ? walk(p, out) : /\.(vue|ts)$/.test(n) && out.push(p) }
  return out
}
const src = walk('src').filter(f => !f.endsWith('i18nExtra.ts')).map(f => readFileSync(f, 'utf8')).join('\n')
const mainKeys = new Set([...readFileSync('src/composables/useI18n.ts', 'utf8').matchAll(/\b([A-Za-z_][A-Za-z0-9_]*):\s*['"`]/g)].map(m => m[1]))
const placeholders = (s: string) => [...s.matchAll(/(?<!\{)\{([a-z]+)\}(?!\})/gi)].map(m => m[1]).sort().join(',')

describe('i18n', () => {
  it('Spanish has exactly the English keys and the same {placeholders}', () => {
    expect(Object.keys(extraEs).sort()).toEqual(Object.keys(extraEn).sort())
    for (const k of Object.keys(extraEn) as (keyof typeof extraEn)[]) expect(placeholders(extraEs[k]), k).toBe(placeholders(extraEn[k]))
  })
  it('every language translates (almost) all keys and keeps the {placeholders}', () => {
    const langs: Record<string, Partial<Record<string, string>>> = { de: extraDe, fr: extraFr, ko: extraKo, zh: extraZh, ja: extraJa, ru: extraRu }
    for (const [code, table] of Object.entries(langs)) {
      const keys = Object.keys(table)
      expect(keys.filter(k => !(k in extraEn)), `${code}: unknown keys`).toEqual([])
      expect(keys.length, `${code}: coverage`).toBeGreaterThan(Object.keys(extraEn).length * 0.9)
      for (const k of keys) expect(placeholders(table[k]!), `${code}.${k}`).toBe(placeholders((extraEn as Record<string, string>)[k]))
    }
  })
  it('every t(\'key\') used in the app exists', () => {
    const used = new Set([...src.matchAll(/\bt\(\s*'([A-Za-z0-9_]+)'/g)].map(m => m[1]))
    const missing = [...used].filter(k => !k.endsWith('_') && !(k in extraEn) && !mainKeys.has(k))
    expect(missing).toEqual([])
  })
  it('translates, interpolates and falls back to English', () => {
    const { t, setLocale } = useI18n()
    setLocale('es'); expect(t('bServers')).toBe('Servidores'); expect(t('fQueued', { n: 3 })).toBe('3 archivo(s) añadidos a la cola')
    setLocale('en'); expect(t('bServers')).toBe('Servers'); expect(t('fQueued', { n: 3 })).toBe('3 file(s) added to the queue')
    setLocale('de'); expect(t('bServers')).toBe('Server'); expect(t('fQueued', { n: 2 })).toContain('2')
    setLocale('ja'); expect(t('bServers')).toBe('サーバー')
    setLocale('en')
  })
})
