// State of the "Oweeme" side panel: connection, rooms, the open conversation
// and polling. All network calls go through Rust commands (src-tauri/src/oweeme.rs),
// which read the `owc_` token from the OS keychain — the token never lives here.
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface OweemeRoom {
  uuid: string
  name: string
  repositoryUrl: string | null
  workspaceName: string | null
  projectName: string | null
  projectUuid: string | null
  projectKey: string | null
  myRole: string
  memberCount: number
  messageCount: number
}

/** Something shared with a message. See useOweemeShare.ts for how each kind is built and opened. */
export interface OweemeAttachment {
  kind: 'code' | 'file' | 'flow' | 'preview' | 'image'
  name: string
  content: string
  /** Images only: Oweeme deletes chat images after `retentionDays` (30), then `expired` is true. */
  expired?: boolean
  retentionDays?: number
}

export interface OweemeMessage {
  uuid: string
  senderName: string
  message: string
  codeContext: string | null
  attachment: OweemeAttachment | null
  isMine: boolean
  createdAt: string
}

export const OWEEME_SERVERS = [
  { label: 'oweeme.com', url: 'https://oweeme.com/api' },
  { label: 'oweeme_pod (local)', url: 'http://localhost:8090/api' },
]

const BASE_KEY = 'oweeme_base_url'
const ROOM_KEY = 'oweeme_last_room'

// Module-level state: shared by every component that uses the composable.
const baseUrl = ref(localStorage.getItem(BASE_KEY) || OWEEME_SERVERS[0].url)
const connected = ref(false)
const checked = ref(false)
const rooms = ref<OweemeRoom[]>([])
const activeRoomId = ref<string | null>(localStorage.getItem(ROOM_KEY))
const messages = ref<OweemeMessage[]>([])
const error = ref('')
const loading = ref(false)
let timer: ReturnType<typeof setInterval> | null = null

/** "git@github.com:Org/Repo.git" and "https://github.com/org/repo" → "github.com/org/repo" */
export function normalizeRepo(url: string | null | undefined): string {
  if (!url) return ''
  return url
    .trim()
    .toLowerCase()
    .replace(/^git@([^:]+):/, '$1/')
    .replace(/^[a-z+]+:\/\//, '')
    .replace(/^[^@/]+@/, '')
    .replace(/\/+$/, '')
    .replace(/\.git$/, '')
}

export function useOweemeChat() {
  const activeRoom = computed(() => rooms.value.find((r) => r.uuid === activeRoomId.value) ?? null)

  async function run<T>(fn: () => Promise<T>): Promise<T | undefined> {
    error.value = ''
    try {
      return await fn()
    } catch (e) {
      error.value = String(e)
      return undefined
    }
  }

  async function init() {
    if (checked.value) return
    checked.value = true
    connected.value = (await run(() => invoke<boolean>('oweeme_has_token'))) ?? false
    if (connected.value) await refreshRooms()
  }

  function setBase(url: string) {
    baseUrl.value = url.trim()
    localStorage.setItem(BASE_KEY, baseUrl.value)
  }

  async function connect(token: string) {
    loading.value = true
    const list = await run(() => invoke<OweemeRoom[]>('oweeme_connect', { base: baseUrl.value, token }))
    loading.value = false
    if (list) {
      rooms.value = list
      connected.value = true
    }
    return !!list
  }

  async function disconnect() {
    stopPolling()
    await run(() => invoke('oweeme_disconnect'))
    connected.value = false
    rooms.value = []
    messages.value = []
  }

  async function refreshRooms() {
    const list = await run(() => invoke<OweemeRoom[]>('oweeme_rooms', { base: baseUrl.value }))
    if (list) rooms.value = list
  }

  async function loadMessages() {
    if (!activeRoomId.value) return
    const list = await run(() => invoke<OweemeMessage[]>('oweeme_messages', { base: baseUrl.value, room: activeRoomId.value }))
    if (list) messages.value = list
  }

  async function openRoom(uuid: string | null) {
    activeRoomId.value = uuid
    messages.value = []
    if (uuid) localStorage.setItem(ROOM_KEY, uuid)
    else localStorage.removeItem(ROOM_KEY)
    await loadMessages()
  }

  /** Opens the room whose repository is the git remote of the open project, if any. */
  async function openRoomForRepo(remote: string | null) {
    const target = normalizeRepo(remote)
    if (!target) return false
    const match = rooms.value.find((r) => normalizeRepo(r.repositoryUrl) === target)
    if (match && match.uuid !== activeRoomId.value) await openRoom(match.uuid)
    return !!match
  }

  async function send(message: string, codeContext: string, attachment: OweemeAttachment | null = null) {
    if (!activeRoomId.value || !message.trim()) return false
    const sent = await run(() =>
      invoke<OweemeMessage>('oweeme_send', { base: baseUrl.value, room: activeRoomId.value, message: message.trim(), codeContext, attachment }),
    )
    if (sent) messages.value = [...messages.value, sent]
    return sent
  }

  async function createRoom(name: string, repositoryUrl: string) {
    const room = await run(() => invoke<OweemeRoom>('oweeme_create_room', { base: baseUrl.value, name: name.trim(), repositoryUrl: repositoryUrl.trim() }))
    if (room) {
      rooms.value = [room, ...rooms.value]
      await openRoom(room.uuid)
    }
    return !!room
  }

  /** Refreshes the open conversation every few seconds while the panel is visible. */
  function startPolling(ms = 5000) {
    stopPolling()
    timer = setInterval(() => {
      if (connected.value && activeRoomId.value && document.visibilityState === 'visible') void loadMessages()
    }, ms)
  }

  function stopPolling() {
    if (timer) clearInterval(timer)
    timer = null
  }

  return {
    baseUrl, connected, rooms, activeRoom, activeRoomId, messages, error, loading,
    init, setBase, connect, disconnect, refreshRooms, openRoom, openRoomForRepo, send, createRoom, startPolling, stopPolling,
  }
}
