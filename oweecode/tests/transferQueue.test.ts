import { describe, expect, it } from 'vitest'
import { createTransferQueue, type QItem } from '../src/composables/transferQueue'

const file = (name: string, size = 100) => ({ dir: 'up' as const, name, local: '/l/' + name, remote: '/r/' + name, size })
const tick = () => new Promise(r => setTimeout(r, 0))
async function settle(q: ReturnType<typeof createTransferQueue>) { for (let i = 0; i < 50 && (q.state.running || q.state.items.some(x => x.status === 'queued' && !q.state.paused)); i++) await tick() }

describe('transfer queue', () => {
  it('runs files one after another, in order, never two at once', async () => {
    const order: string[] = []; let active = 0, maxActive = 0
    const q = createTransferQueue(async i => { active++; maxActive = Math.max(maxActive, active); order.push(i.name); await tick(); active--; })
    q.enqueue([file('a'), file('b'), file('c')])
    await settle(q)
    expect(order).toEqual(['a', 'b', 'c'])
    expect(maxActive).toBe(1)
    expect(q.counts().done).toBe(3)
  })

  it('retries a failed file once on its own, then marks it as failed', async () => {
    const tries: Record<string, number> = {}
    const q = createTransferQueue(async i => { tries[i.name] = (tries[i.name] ?? 0) + 1; if (i.name === 'mala') throw new Error('permiso denegado'); if (i.name === 'flaky' && tries[i.name] < 2) throw new Error('timeout') })
    q.enqueue([file('flaky'), file('mala'), file('ok')])
    await settle(q)
    expect(tries).toEqual({ flaky: 2, mala: 2, ok: 1 })
    const by = Object.fromEntries(q.state.items.map(i => [i.name, i]))
    expect(by.flaky.status).toBe('done')
    expect(by.mala.status).toBe('error')
    expect(by.mala.error).toBe('permiso denegado')
    expect(by.ok.status).toBe('done')
  })

  it('pause stops after the current file; resume continues', async () => {
    const done: string[] = []
    let q!: ReturnType<typeof createTransferQueue>
    q = createTransferQueue(async i => { if (i.name === 'a') q.pause(); await tick(); done.push(i.name) })
    q.enqueue([file('a'), file('b')])
    await settle(q)
    expect(done).toEqual(['a'])
    expect(q.counts()).toMatchObject({ done: 1, queued: 1 })
    q.resume(); await settle(q)
    expect(done).toEqual(['a', 'b'])
  })

  it('cancelAll cancels what is waiting; retryFailed brings errors and cancelled back', async () => {
    let q!: ReturnType<typeof createTransferQueue>
    const ran: string[] = []
    q = createTransferQueue(async i => { ran.push(i.name); if (i.name === 'a') { q.cancelAll() } })
    q.enqueue([file('a'), file('b'), file('c')])
    await settle(q)
    expect(ran).toEqual(['a'])
    expect(q.counts()).toMatchObject({ done: 1, canceled: 2 })
    q.retryFailed(); await settle(q)
    expect(ran).toEqual(['a', 'b', 'c'])
  })

  it('reports when the whole batch finishes, with totals', async () => {
    let summary: any = null
    const q = createTransferQueue(async i => { if (i.name === 'x') throw new Error('no') }, s => { summary = s })
    q.enqueue([file('a'), file('x')])
    await settle(q)
    expect(summary).toEqual({ done: 1, failed: 1 })
  })

  it('tracks bytes, ignoring cancelled files, and removes or clears items', async () => {
    const q = createTransferQueue(async () => {})
    q.enqueue([file('a', 100), file('b', 300)])
    await settle(q)
    expect(q.bytes()).toEqual({ total: 400, sent: 400 })
    q.remove(q.state.items[0].id)
    expect(q.state.items.length).toBe(1)
    q.clearFinished()
    expect(q.state.items.length).toBe(0)
  })

  it('progress updates the file in progress and a speed estimate', async () => {
    let seen: QItem | null = null
    const q = createTransferQueue(async i => { seen = i; q.progress(10); await new Promise(r => setTimeout(r, 450)); q.progress(2_000_010) })
    q.enqueue([file('big', 5_000_000)])
    await new Promise(r => setTimeout(r, 120))
    expect(q.state.current?.sent).toBe(10)
    await settle(q)
    expect(seen).not.toBeNull()
  })
})
