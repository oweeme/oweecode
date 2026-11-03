// Transfer queue (FileZilla style): files wait in a list, run one after another, can be paused, cancelled and retried.
// The actual upload/download is injected, so the logic is testable without a server.
import { reactive } from 'vue'

export type QStatus = 'queued' | 'active' | 'done' | 'error' | 'canceled'
export interface QItem {
  id: number
  dir: 'up' | 'down'
  name: string          // shown name (relative path)
  local: string
  remote: string
  size: number
  status: QStatus
  sent: number
  error: string
  tries: number
  ms: number            // how long it took (done) 
}
export type Runner = (item: QItem) => Promise<void>

export const MAX_AUTO_RETRIES = 1

export function createTransferQueue(run: Runner, onIdle?: (summary: { done: number; failed: number }) => void) {
  let seq = 0
  const state = reactive({
    items: [] as QItem[],
    paused: false,
    running: false,
    current: null as QItem | null,
    speed: 0,           // bytes per second of the file in progress
  })
  let lastSample = { t: 0, sent: 0 }
  let batchDone = 0, batchFailed = 0

  function enqueue(files: { dir: 'up' | 'down'; name: string; local: string; remote: string; size: number }[]) {
    for (const f of files) state.items.push({ id: ++seq, ...f, status: 'queued', sent: 0, error: '', tries: 0, ms: 0 })
    void pump()
  }

  /** called with the bytes sent so far of the file in progress */
  function progress(sent: number) {
    const c = state.current; if (!c) return
    c.sent = sent
    const now = Date.now()
    if (now - lastSample.t >= 400) {
      const dt = (now - lastSample.t) / 1000
      if (lastSample.t && dt > 0 && sent >= lastSample.sent) state.speed = Math.round((sent - lastSample.sent) / dt)
      lastSample = { t: now, sent }
    }
  }

  async function pump() {
    if (state.running) return
    state.running = true
    try {
      while (!state.paused) {
        const item = state.items.find(i => i.status === 'queued')
        if (!item) break
        item.status = 'active'; item.sent = 0; item.error = ''
        state.current = item; state.speed = 0; lastSample = { t: 0, sent: 0 }
        const t0 = Date.now()
        try {
          await run(item)
          item.status = 'done'; item.sent = item.size; item.ms = Date.now() - t0; batchDone++
        } catch (e) {
          item.error = e instanceof Error ? e.message : String(e)
          if (item.tries < MAX_AUTO_RETRIES) {          // one automatic retry, at the end of the line
            item.tries++; item.status = 'queued'
            state.items.splice(state.items.indexOf(item), 1); state.items.push(item)
          } else { item.status = 'error'; batchFailed++ }
        }
      }
    } finally {
      state.current = null; state.speed = 0; state.running = false
    }
    if (!state.paused && !state.items.some(i => i.status === 'queued')) {
      const summary = { done: batchDone, failed: batchFailed }
      batchDone = 0; batchFailed = 0
      onIdle?.(summary)
    }
  }

  const pause = () => { state.paused = true }
  function resume() { state.paused = false; void pump() }
  function cancelAll() { for (const i of state.items) if (i.status === 'queued') i.status = 'canceled'; state.paused = false }
  function retryFailed() {
    for (const i of state.items) if (i.status === 'error' || i.status === 'canceled') { i.status = 'queued'; i.tries = 0; i.error = ''; i.sent = 0 }
    void pump()
  }
  function remove(id: number) {
    const i = state.items.find(x => x.id === id)
    if (i && i.status !== 'active') state.items.splice(state.items.indexOf(i), 1)
  }
  function clearFinished() { state.items = state.items.filter(i => i.status === 'queued' || i.status === 'active' || i.status === 'error') }

  function counts() {
    const c = { queued: 0, active: 0, done: 0, error: 0, canceled: 0 }
    for (const i of state.items) c[i.status]++
    return c
  }
  /** bytes of everything still pending or running, and what has already gone through */
  function bytes() {
    let total = 0, sent = 0
    for (const i of state.items) { if (i.status === 'canceled') continue; total += i.size; sent += i.status === 'done' ? i.size : i.sent }
    return { total, sent }
  }

  return { state, enqueue, progress, pause, resume, cancelAll, retryFailed, remove, clearFinished, counts, bytes }
}
