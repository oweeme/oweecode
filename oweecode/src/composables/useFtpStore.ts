import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface FtpConn {
  id: string
  name: string
  protocol: 'sftp' | 'ftp' | 'ftps'
  host: string
  port: number
  user: string
  pass: string
  root: string
  authType: 'password' | 'key'
  trustedCert?: string   // FTPS: SHA-256 fingerprint of a certificate the user accepted (pinned)
}

// Module-scope singleton state: survives FtpPanel unmount/remount (sidebar toggle,
// switching sidebar views) so active connections don't appear disconnected and
// force a manual reconnect.
const connections = ref<FtpConn[]>([])
const connectedIds = ref<Set<string>>(new Set())

// Passwords live in the OS keychain (secret_* commands); localStorage only keeps
// the connection details. If the keychain is unavailable (e.g. no Secret Service
// on a bare Linux session) the password falls back to localStorage so it isn't lost.
const secretKey = (id: string) => `ftp:${id}`
const keychainBlocked = ref(false)
let knownIds = new Set<string>()

function persist(strip: boolean) {
  const safe = connections.value.map(c => ({ ...c, pass: strip ? '' : c.pass }))
  localStorage.setItem('ftp_connections', JSON.stringify(safe))
}

async function syncSecrets() {
  const list = connections.value
  try {
    for (const c of list) {
      if (c.pass) await invoke('secret_set', { key: secretKey(c.id), value: c.pass })
      else await invoke('secret_delete', { key: secretKey(c.id) })
    }
    const now = new Set(list.map(c => c.id))
    for (const id of knownIds) if (!now.has(id)) await invoke('secret_delete', { key: secretKey(id) })
    knownIds = now
    keychainBlocked.value = false
    persist(true)
  } catch {
    keychainBlocked.value = true
    persist(false)
  }
}

async function load() {
  try { connections.value = JSON.parse(localStorage.getItem('ftp_connections') ?? '[]') } catch { connections.value = [] }
  knownIds = new Set(connections.value.map(c => c.id))
  const legacy = connections.value.some(c => c.pass)
  if (legacy) { await syncSecrets(); return }   // migrate plaintext passwords into the keychain
  for (const c of connections.value) {
    try { c.pass = (await invoke<string | null>('secret_get', { key: secretKey(c.id) })) ?? '' } catch { break }
  }
}
load()

export function useFtpStore() {
  function save() { persist(false); syncSecrets() }
  return { connections, connectedIds, save, keychainBlocked }
}
