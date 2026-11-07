import { describe, expect, it, vi } from 'vitest'

// The composable imports Tauri's invoke; the pure helper under test does not need it.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

const { normalizeRepo } = await import('../src/composables/useOweemeChat')

describe('normalizeRepo (links the open project with its Oweeme room)', () => {
  it('treats SSH, HTTPS and .git variants of the same repo as equal', () => {
    const want = 'github.com/oweeme/oweecode'
    expect(normalizeRepo('git@github.com:oweeme/oweecode.git')).toBe(want)
    expect(normalizeRepo('https://github.com/oweeme/oweecode')).toBe(want)
    expect(normalizeRepo('https://github.com/Oweeme/OweeCode.git/')).toBe(want)
    expect(normalizeRepo('ssh://git@github.com/oweeme/oweecode.git')).toBe(want)
    expect(normalizeRepo('https://user@gitlab.com/team/app.git')).toBe('gitlab.com/team/app')
  })

  it('returns empty for missing remotes', () => {
    expect(normalizeRepo(null)).toBe('')
    expect(normalizeRepo('  ')).toBe('')
  })
})
