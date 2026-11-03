import { describe, expect, it } from 'vitest'
import { cssHandledInPage, hasHotReload, localPort, shouldReload } from '../src/composables/previewReload'

const ports = [
  { port: 9000, process: 'node', cmdline: 'node /p/node_modules/.bin/quasar dev' },
  { port: 5173, process: 'node', cmdline: 'node /p/node_modules/.bin/vite' },
  { port: 8000, process: 'php', cmdline: 'php -S 127.0.0.1:8000 -t site' },
  { port: 8080, process: 'oweecode', cmdline: '' },
  { port: 8100, process: 'python3', cmdline: 'python3 manage.py runserver' },
]

describe('live reload decision', () => {
  it('auto: leaves servers that already hot-reload alone, reloads the others', () => {
    expect(shouldReload('auto', 'http://localhost:9000', ports)).toBe(false)
    expect(shouldReload('auto', 'http://localhost:5173/page', ports)).toBe(false)
    expect(shouldReload('auto', 'http://localhost:8000', ports)).toBe(true)      // PHP
    expect(shouldReload('auto', 'http://127.0.0.1:8080', ports)).toBe(true)      // OweeCode static server
    expect(shouldReload('auto', 'http://localhost:8100', ports)).toBe(true)      // Django
  })
  it('always / off', () => {
    expect(shouldReload('always', 'http://localhost:9000', ports)).toBe(true)
    expect(shouldReload('off', 'http://localhost:8000', ports)).toBe(false)
  })
  it('never touches external pages', () => {
    expect(shouldReload('always', 'https://example.com', ports)).toBe(false)
    expect(shouldReload('auto', 'http://192.168.1.5:3000', ports)).toBe(false)
  })
  it('an unknown local port is reloaded in auto mode', () => expect(shouldReload('auto', 'http://localhost:4444', ports)).toBe(true))
  it('helpers', () => {
    expect(localPort('http://localhost')).toBe(80)
    expect(localPort('https://localhost')).toBe(443)
    expect(localPort('no es url')).toBeNull()
    expect(hasHotReload('/usr/bin/node /x/node_modules/.bin/next dev')).toBe(true)
    expect(hasHotReload('node server.js')).toBe(false)
  })
})

describe('cssHandledInPage', () => {
  it('only for .css on a static-folder port', () => {
    expect(cssHandledInPage('a/site.css', 'http://localhost:8080/', [8080])).toBe(true)
    expect(cssHandledInPage('a/site.css', 'http://localhost:9000/', [8080])).toBe(false)
    expect(cssHandledInPage('a/page.php', 'http://localhost:8080/', [8080])).toBe(false)
  })
})
