import { ref } from 'vue'

export interface RedisConn {
  id: string
  name: string
  host: string
  port: number
  password: string
  db: number
}

// Module-scope singleton state: survives RedisPanel unmount/remount (sidebar
// toggle, switching sidebar views) so active connections don't appear
// disconnected and force a manual reconnect.
const connections = ref<RedisConn[]>([])
const connectedIds = ref<Set<string>>(new Set())

function load() {
  try { connections.value = JSON.parse(localStorage.getItem('redis_connections') ?? '[]') } catch { connections.value = [] }
}
load()

export function useRedisStore() {
  function save() { localStorage.setItem('redis_connections', JSON.stringify(connections.value)) }
  return { connections, connectedIds, save }
}
