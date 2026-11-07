import { describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
const { taskKeyFromText } = await import('../src/composables/useOweemeTasks')
const { toLanUrl } = await import('../src/composables/useOweemeShare')

describe('taskKeyFromText (branch / commit → task of the project)', () => {
  it('finds the key in branch names and commit messages', () => {
    expect(taskKeyFromText('bella-7-paypal', 'BELLA')).toBe('BELLA-7')
    expect(taskKeyFromText('feature/BELLA-12', 'BELLA')).toBe('BELLA-12')
    expect(taskKeyFromText('BELLA-3: integrar PayPal', 'BELLA')).toBe('BELLA-3')
    expect(taskKeyFromText('fix (bella-10) seo', 'bella')).toBe('BELLA-10')
  })
  it('ignores other projects, partial numbers and missing data', () => {
    expect(taskKeyFromText('umbrella-7', 'BELLA')).toBeNull()
    expect(taskKeyFromText('main', 'BELLA')).toBeNull()
    expect(taskKeyFromText('bella-7', null)).toBeNull()
    expect(taskKeyFromText(null, 'BELLA')).toBeNull()
  })
})

describe('toLanUrl (share a local preview with the team on the same network)', () => {
  it('swaps local hosts for the LAN IP', () => {
    expect(toLanUrl('http://localhost:5173/menu', '192.168.1.20')).toBe('http://192.168.1.20:5173/menu')
    expect(toLanUrl('http://127.0.0.1:9000', '192.168.1.20')).toBe('http://192.168.1.20:9000')
  })
  it('leaves public addresses alone and needs an IP', () => {
    expect(toLanUrl('https://oweeme.com', '192.168.1.20')).toBeNull()
    expect(toLanUrl('http://localhost:5173', null)).toBeNull()
  })
})
