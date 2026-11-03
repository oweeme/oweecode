import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface DbConnection {
  id: string
  name: string
  driver: 'mysql' | 'postgres' | 'sqlite'
  host: string
  port: number
  user: string
  password: string
  database: string
  filepath: string
}

// Module-scope singleton state: survives DatabasePanel unmount/remount when the
// sidebar is toggled closed/open or the user switches to another sidebar view,
// so an active connection stays connected instead of forcing a manual reconnect.
const connections = ref<DbConnection[]>(JSON.parse(localStorage.getItem('db_connections') || '[]'))
const secretKey = (id: string) => `db:${id}`
let knownIds = new Set<string>(connections.value.map(c => c.id))
const activeConnId = ref<string | null>(null)
const tables = ref<string[]>([])


// Passwords go to the OS keychain; localStorage keeps only the connection details.
// Without a keychain the password stays in localStorage (as before) so nothing is lost.
function persist(strip: boolean) {
  localStorage.setItem('db_connections', JSON.stringify(connections.value.map(c => ({ ...c, password: strip ? '' : c.password }))))
}
async function syncSecrets() {
  const list = connections.value
  try {
    for (const c of list) {
      if (c.password) await invoke('secret_set', { key: secretKey(c.id), value: c.password })
      else await invoke('secret_delete', { key: secretKey(c.id) })
    }
    const now = new Set(list.map(c => c.id))
    for (const id of knownIds) if (!now.has(id)) await invoke('secret_delete', { key: secretKey(id) })
    knownIds = now
    persist(true)
  } catch { persist(false) }
}
;(async () => {
  if (connections.value.some(c => c.password)) { await syncSecrets(); return }   // migrate plaintext
  for (const c of connections.value) {
    try { c.password = (await invoke<string | null>('secret_get', { key: secretKey(c.id) })) ?? '' } catch { break }
  }
})()

export function useDatabaseStore() {
  function saveConnections() { persist(false); syncSecrets() }
  return { connections, activeConnId, tables, saveConnections }
}
