import { describe, expect, it } from 'vitest'
import { errorText, RULES } from '../src/composables/errorText'
import { blockCategory, blockText } from '../src/composables/blockText'
import { gameLabel, nativeText } from '../src/composables/nativeText'

describe('errorText', () => {
  it('keeps Spanish and unknown messages, translates the known ones', () => {
    const m = 'No se encontró Podman ni Docker instalado en el sistema'
    expect(errorText(m, 'es')).toBe(m)
    expect(errorText(m, 'en')).toBe('Neither Podman nor Docker is installed on the system')
    expect(errorText(m, 'ja')).toContain('Podman')
    expect(errorText('some english error', 'de')).toBe('some english error')
  })
  it('fills the captured parts', () => {
    expect(errorText('Carpeta inválida: /x', 'fr')).toBe('Dossier invalide : /x')
    expect(errorText('git push falló', 'en')).toBe('git push failed')
    expect(errorText('Comando cancelado: superó el límite de 30s', 'ru')).toBe('Команда отменена: превышен лимит в 30 с')
    expect(errorText('SELECT 1\n\nEn declaración #3: boom', 'zh')).toBe('SELECT 1\n\n在语句 #3：boom')
  })
  it('every rule has the pattern + 7 translations with the same {n} markers', () => {
    expect(RULES.length).toBeGreaterThan(30)
    for (const rule of RULES) {
      expect(rule.length, String(rule[0])).toBe(8)
      const marks = (s: string) => [...s.matchAll(/\{(\d)\}/g)].map(m => m[1]).sort().join('')
      for (let i = 2; i < 8; i++) expect(marks(rule[i] as string), `${rule[0]} #${i}`).toBe(marks(rule[1] as string))
    }
  })
})

describe('designer texts', () => {
  it('translate block names and categories', () => {
    expect(blockText('Tarjeta', 'es')).toBe('Tarjeta')
    expect(blockText('Tarjeta', 'en')).toBe('Card')
    expect(blockText('Tarjeta', 'ko')).toBe('카드')
    expect(blockCategory('DaisyUI · Acciones', 'de')).toBe('DaisyUI · Aktionen')
    expect(blockText('algo nuevo', 'de')).toBe('algo nuevo')
  })
  it('translate native widget and game labels', () => {
    expect(nativeText('Etiqueta', 'fr')).toBe('Étiquette')
    expect(gameLabel('Juego 2D — pygame', 'ja')).toBe('2D ゲーム — pygame')
  })
})
