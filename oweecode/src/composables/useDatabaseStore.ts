import { ref } from 'vue'

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
const activeConnId = ref<string | null>(null)
const tables = ref<string[]>([])

export function useDatabaseStore() {
  function saveConnections() {
    localStorage.setItem('db_connections', JSON.stringify(connections.value))
  }
  return { connections, activeConnId, tables, saveConnections }
}
