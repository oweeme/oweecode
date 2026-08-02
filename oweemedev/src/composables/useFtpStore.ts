import { ref } from 'vue'

export interface FtpConn {
  id: string
  name: string
  protocol: 'sftp' | 'ftp'
  host: string
  port: number
  user: string
  pass: string
  root: string
  authType: 'password' | 'key'
}

// Module-scope singleton state: survives FtpPanel unmount/remount (sidebar toggle,
// switching sidebar views) so active connections don't appear disconnected and
// force a manual reconnect.
const connections = ref<FtpConn[]>([])
const connectedIds = ref<Set<string>>(new Set())

function load() {
  try { connections.value = JSON.parse(localStorage.getItem('ftp_connections') ?? '[]') } catch { connections.value = [] }
}
load()

export function useFtpStore() {
  function save() { localStorage.setItem('ftp_connections', JSON.stringify(connections.value)) }
  return { connections, connectedIds, save }
}
