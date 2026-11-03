import { describe, expect, it } from 'vitest'
import { rustText } from '../src/composables/rustText'

describe('rustText', () => {
  it('keeps Spanish for Spanish and translates known labels otherwise', () => {
    expect(rustText('Compilar APK', 'es')).toBe('Compilar APK')
    expect(rustText('Compilar APK', 'en')).toBe('Build APK')
    expect(rustText('Compilar APK', 'de')).toBe('APK bauen')
    expect(rustText('Compilar APK', 'ja')).toBe('APK をビルド')
    expect(rustText('Compilar APK', 'it')).toBe('Build APK')
  })
  it('handles labels with a variable part and leaves unknown text alone', () => {
    expect(rustText('Servir public/ (HTML estático)', 'en')).toBe('Serve public/ (static HTML)')
    expect(rustText('Servir la carpeta del proyecto (HTML estático)', 'en')).toBe('Serve the project folder (static HTML)')
    expect(rustText('Puertos publicados: 80, 443', 'en')).toBe('Published ports: 80, 443')
    expect(rustText('npm run dev', 'en')).toBe('npm run dev')
    expect(rustText('Servir public/ (HTML estático)', 'ru')).toBe('Раздать public/ (статический HTML)')
    expect(rustText('Servir la carpeta del proyecto (HTML estático)', 'zh')).toBe('托管项目文件夹（静态 HTML）')
  })
})
