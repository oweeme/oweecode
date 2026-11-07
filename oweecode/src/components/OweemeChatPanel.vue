<script setup lang="ts">
// Side panel "Oweeme": team chat rooms of oweeme.com inside the IDE.
//  - Connect: server + personal `owc_` token (created in Oweeme → Team chat → Tokens).
//  - Rooms: the one whose repository matches this project's git remote opens by itself.
//  - Messages: refreshed every 5 s; "Attach selection" sends the code selected in the
//    editor with its file and branch as context.
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useEditorStore } from '../composables/useEditorStore'
import { useI18n } from '../composables/useI18n'
import { OWEEME_SERVERS, useOweemeChat } from '../composables/useOweemeChat'

const store = useEditorStore()
const { t } = useI18n()
const chat = useOweemeChat()

const tokenInput = ref('')
const customBase = ref(chat.baseUrl.value)
const text = ref('')
const attach = ref<{ code: string; context: string } | null>(null)
const branch = ref('')
const remote = ref<string | null>(null)
const sending = ref(false)
const showNewRoom = ref(false)
const newRoom = ref({ name: '', repo: '' })
const listEl = ref<HTMLElement | null>(null)

const serverOptions = computed(() => [...OWEEME_SERVERS, { label: t('owCustomServer'), url: '' }])
const isPreset = computed(() => OWEEME_SERVERS.some((s) => s.url === chat.baseUrl.value))
const webBase = computed(() => chat.baseUrl.value.replace(/\/api\/?$/, ''))
const projectName = computed(() => store.state.rootPath?.split(/[\\/]/).filter(Boolean).pop() ?? '')
const selection = computed(() => store.state.selectedText.trim())

/** Path of the active file relative to the project, e.g. "src/main.rs". */
function relativeActivePath(): string {
  const path = store.state.activeTabPath ?? ''
  const root = store.state.rootPath ?? ''
  return root && path.startsWith(root) ? path.slice(root.length).replace(/^[\\/]+/, '') : path.split(/[\\/]/).pop() ?? ''
}

async function readGit() {
  const root = store.state.rootPath
  if (!root) { remote.value = null; branch.value = ''; return }
  try { remote.value = await invoke<string | null>('git_remote_url', { path: root }) } catch { remote.value = null }
  try {
    const branches = await invoke<Array<{ name: string; current: boolean }>>('git_branches', { path: root })
    branch.value = branches.find((b) => b.current)?.name ?? ''
  } catch { branch.value = '' }
}

async function afterConnect() {
  await readGit()
  const matched = await chat.openRoomForRepo(remote.value)
  if (!matched && chat.activeRoomId.value && chat.rooms.value.some((r) => r.uuid === chat.activeRoomId.value)) await chat.openRoom(chat.activeRoomId.value)
  chat.startPolling()
}

async function connect() {
  if (!isPreset.value) chat.setBase(customBase.value)
  if (await chat.connect(tokenInput.value)) {
    tokenInput.value = ''
    await afterConnect()
  }
}

function attachSelection() {
  if (!selection.value) return
  const file = relativeActivePath()
  attach.value = { code: selection.value, context: [file, branch.value].filter(Boolean).join(' · ') }
}

async function send() {
  if (!text.value.trim() || sending.value) return
  sending.value = true
  // codeContext = "file · branch" + the selected code, only when the user attached it.
  const context = attach.value ? `${attach.value.context}\n${attach.value.code}` : ''
  if (await chat.send(text.value, context)) {
    text.value = ''
    attach.value = null
  }
  sending.value = false
}

async function createRoom() {
  if (await chat.createRoom(newRoom.value.name, newRoom.value.repo)) showNewRoom.value = false
}

function openNewRoom() {
  newRoom.value = { name: projectName.value, repo: remote.value ? 'https://' + remote.value.replace(/^git@([^:]+):/, '$1/').replace(/^[a-z]+:\/\//, '').replace(/\.git$/, '') : '' }
  showNewRoom.value = true
}

function time(iso: string) {
  const d = new Date(iso)
  return isNaN(d.getTime()) ? '' : d.toLocaleString(undefined, { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })
}

watch(() => chat.messages.value.length, async () => {
  await nextTick()
  listEl.value?.scrollTo({ top: listEl.value.scrollHeight })
})
watch(() => store.state.rootPath, async () => {
  if (!chat.connected.value) return
  await readGit()
  await chat.openRoomForRepo(remote.value)
})

onMounted(async () => {
  await chat.init()
  if (chat.connected.value) await afterConnect()
})
onUnmounted(() => chat.stopPolling())
</script>

<template>
  <div class="ow-panel">
    <div class="ow-header">
      <span class="ow-title">Oweeme</span>
      <span v-if="chat.connected.value" class="ow-header-actions">
        <button class="ow-icon-btn" :title="t('owRefresh')" @click="chat.refreshRooms()">⟳</button>
        <button class="ow-icon-btn" :title="t('owNewRoom')" @click="openNewRoom">＋</button>
        <button class="ow-icon-btn" :title="t('owDisconnect')" @click="chat.disconnect()">⏻</button>
      </span>
    </div>

    <div v-if="chat.error.value" class="ow-error">{{ chat.error.value }}</div>

    <!-- Not connected: server + token -->
    <div v-if="!chat.connected.value" class="ow-connect">
      <p class="ow-lead">{{ t('owConnectLead') }}</p>
      <label class="ow-label">{{ t('owServer') }}</label>
      <select class="ow-input" :value="isPreset ? chat.baseUrl.value : ''" @change="(e) => { const v = (e.target as HTMLSelectElement).value; if (v) chat.setBase(v); else chat.setBase(customBase) }">
        <option v-for="s in serverOptions" :key="s.label" :value="s.url">{{ s.label }}</option>
      </select>
      <input v-if="!isPreset" v-model="customBase" class="ow-input" placeholder="https://mi-servidor/api" @change="chat.setBase(customBase)" />
      <label class="ow-label">{{ t('owToken') }}</label>
      <input v-model="tokenInput" class="ow-input ow-mono" type="password" placeholder="owc_…" @keydown.enter="connect" />
      <button class="ow-btn ow-btn-main" :disabled="!tokenInput.trim() || chat.loading.value" @click="connect">
        {{ chat.loading.value ? t('owConnecting') : t('owConnect') }}
      </button>
      <p class="ow-hint">
        {{ t('owTokenHint') }}
        <a href="#" @click.prevent="openUrl(webBase + '/dashboard/professional/team')">{{ t('owOpenTokens') }}</a>
      </p>
    </div>

    <!-- Connected -->
    <template v-else>
      <div v-if="showNewRoom" class="ow-newroom">
        <input v-model="newRoom.name" class="ow-input" :placeholder="t('owRoomName')" />
        <input v-model="newRoom.repo" class="ow-input" :placeholder="t('owRoomRepo')" />
        <div class="ow-row">
          <button class="ow-btn ow-btn-main" :disabled="!newRoom.name.trim()" @click="createRoom">{{ t('owCreate') }}</button>
          <button class="ow-btn" @click="showNewRoom = false">{{ t('owCancel') }}</button>
        </div>
      </div>

      <select class="ow-input ow-room-select" :value="chat.activeRoomId.value ?? ''" @change="chat.openRoom(($event.target as HTMLSelectElement).value || null)">
        <option value="">{{ chat.rooms.value.length ? t('owPickRoom') : t('owNoRooms') }}</option>
        <option v-for="r in chat.rooms.value" :key="r.uuid" :value="r.uuid"># {{ r.name }}{{ r.projectName ? ' — ' + r.projectName : '' }}</option>
      </select>
      <div v-if="chat.activeRoom.value?.repositoryUrl" class="ow-repo" :title="chat.activeRoom.value.repositoryUrl">
        {{ chat.activeRoom.value.repositoryUrl.replace(/^https?:\/\//, '') }}
      </div>

      <div ref="listEl" class="ow-messages">
        <div v-if="!chat.activeRoom.value" class="ow-empty">{{ t('owPickRoomHint') }}</div>
        <div v-for="m in chat.messages.value" :key="m.uuid" class="ow-msg" :class="{ mine: m.isMine }">
          <div class="ow-meta">{{ m.senderName }} · {{ time(m.createdAt) }}</div>
          <div class="ow-text">{{ m.message }}</div>
          <pre v-if="m.codeContext" class="ow-code">{{ m.codeContext }}</pre>
        </div>
      </div>

      <div v-if="chat.activeRoom.value" class="ow-composer">
        <div v-if="attach" class="ow-attach">
          <span>📎 {{ attach.context || t('owSelection') }} · {{ attach.code.split('\n').length }} {{ t('owLines') }}</span>
          <button class="ow-icon-btn" @click="attach = null">✕</button>
        </div>
        <textarea v-model="text" class="ow-input ow-textarea" rows="2" :placeholder="t('owMessagePh')" @keydown.enter.exact.prevent="send" />
        <div class="ow-row">
          <button class="ow-btn" :disabled="!selection" :title="selection ? '' : t('owSelectFirst')" @click="attachSelection">📎 {{ t('owAttach') }}</button>
          <span class="ow-spacer" />
          <button class="ow-btn ow-btn-main" :disabled="!text.trim() || sending" @click="send">{{ t('owSend') }}</button>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.ow-panel { display: flex; flex-direction: column; height: 100%; font-family: var(--font-ui); font-size: 12px; color: var(--fg); }
.ow-header { display: flex; align-items: center; justify-content: space-between; padding: 0 12px; height: 36px; flex-shrink: 0; border-bottom: 1px solid var(--border); }
.ow-title { font-size: 10px; font-weight: 700; letter-spacing: 1px; color: var(--fg-muted); text-transform: uppercase; }
.ow-header-actions { display: flex; gap: 2px; }
.ow-icon-btn { background: none; border: none; color: var(--fg-muted); cursor: pointer; padding: 2px 6px; border-radius: 4px; font-size: 13px; }
.ow-icon-btn:hover { color: var(--fg); background: var(--bg-hover); }
.ow-error { margin: 8px 12px; padding: 8px 10px; background: rgba(248,81,73,0.12); border: 1px solid rgba(248,81,73,0.3); border-radius: 6px; font-size: 11px; color: #f85149; line-height: 1.5; }
.ow-connect, .ow-newroom { display: flex; flex-direction: column; gap: 6px; padding: 12px; }
.ow-newroom { border-bottom: 1px solid var(--border); }
.ow-lead { margin: 0 0 6px; color: var(--fg-dim); line-height: 1.5; }
.ow-label { font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: .5px; color: var(--fg-muted); margin-top: 4px; }
.ow-input { width: 100%; box-sizing: border-box; background: var(--bg-dark); border: 1px solid var(--border); color: var(--fg); border-radius: 5px; padding: 6px 8px; font: inherit; }
.ow-input:focus { outline: none; border-color: var(--accent); }
.ow-mono { font-family: var(--font-mono); }
.ow-textarea { resize: vertical; min-height: 40px; }
.ow-room-select { margin: 8px 12px 0; width: calc(100% - 24px); }
.ow-repo { margin: 4px 12px 0; font-size: 10px; color: var(--fg-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.ow-btn { background: var(--bg-mid); border: 1px solid var(--border); color: var(--fg); border-radius: 5px; padding: 5px 10px; cursor: pointer; font: inherit; }
.ow-btn:hover:not(:disabled) { border-color: var(--accent); }
.ow-btn:disabled { opacity: .45; cursor: default; }
.ow-btn-main { background: var(--accent); color: var(--accent-fg); border-color: transparent; font-weight: 600; }
.ow-hint { margin: 6px 0 0; color: var(--fg-muted); font-size: 11px; line-height: 1.5; }
.ow-hint a { color: var(--accent); }
.ow-messages { flex: 1; overflow-y: auto; padding: 10px 12px; display: flex; flex-direction: column; gap: 8px; }
.ow-empty { color: var(--fg-muted); text-align: center; margin-top: 24px; line-height: 1.5; }
.ow-msg { max-width: 92%; align-self: flex-start; background: var(--bg-mid); border-radius: 8px; padding: 6px 9px; }
.ow-msg.mine { align-self: flex-end; background: var(--bg-active); }
.ow-meta { font-size: 10px; color: var(--fg-muted); margin-bottom: 2px; }
.ow-text { white-space: pre-wrap; word-break: break-word; line-height: 1.45; }
.ow-code { margin: 6px 0 0; padding: 6px 8px; background: var(--bg-darkest); border-radius: 5px; font-family: var(--font-mono); font-size: 11px; white-space: pre-wrap; word-break: break-all; max-height: 180px; overflow: auto; }
.ow-composer { border-top: 1px solid var(--border); padding: 8px 12px; display: flex; flex-direction: column; gap: 6px; }
.ow-attach { display: flex; align-items: center; justify-content: space-between; gap: 6px; font-size: 11px; color: var(--fg-dim); background: var(--bg-mid); border-radius: 5px; padding: 3px 4px 3px 8px; }
.ow-row { display: flex; gap: 6px; align-items: center; }
.ow-spacer { flex: 1; }
</style>
