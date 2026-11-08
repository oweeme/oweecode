<script setup lang="ts">
// Side panel "Oweeme": the team's rooms and the project's tasks, inside the IDE.
//
//  Chat   — messages of the room linked to this repo; share the selection, the
//           whole file, your flow or your preview; open what others share.
//  Tasks  — board of the room's project: open a task, comment, move it between
//           columns. The current task is detected from the git branch
//           (bella-7-paypal → BELLA-7), and after a commit that mentions it the
//           panel offers to comment the commit and move it to Review.
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useEditorStore } from '../composables/useEditorStore'
import { useI18n } from '../composables/useI18n'
import { OWEEME_SERVERS, useOweemeChat, type OweemeAttachment } from '../composables/useOweemeChat'
import { taskKeyFromText, useOweemeTasks } from '../composables/useOweemeTasks'
import { useOweemeShare, type PreviewShare } from '../composables/useOweemeShare'
import { compressForChat } from '../composables/imageCompress'
import { renderFlowPng } from '../composables/flowSnapshot'
import { flowState } from '../composables/useFlowState'

const store = useEditorStore()
const { t } = useI18n()
const chat = useOweemeChat()
const tasks = useOweemeTasks()
const share = useOweemeShare()

const view = ref<'chat' | 'tasks'>('chat')
const tokenInput = ref('')
const customBase = ref(chat.baseUrl.value)
const text = ref('')
const attach = ref<OweemeAttachment | null>(null)
const showShareMenu = ref(false)
const branch = ref('')
const remote = ref<string | null>(null)
const sending = ref(false)
const showNewRoom = ref(false)
const newRoom = ref({ name: '', repo: '' })
const listEl = ref<HTMLElement | null>(null)
const commentText = ref('')
const newTaskTitle = ref('')
const notice = ref('')
const pendingCommit = ref<{ key: string; message: string } | null>(null)
const imageInput = ref<HTMLInputElement | null>(null)
const lightbox = ref<string | null>(null)
const compressing = ref(false)
// An old token (chat only) gets 403 on tasks: explain how to fix it instead of the raw error.
const needsNewToken = computed(() => /not allowed for this endpoint/i.test(chat.error.value))

const serverOptions = computed(() => [...OWEEME_SERVERS, { label: t('owCustomServer'), url: '' }])
const isPreset = computed(() => OWEEME_SERVERS.some((s) => s.url === chat.baseUrl.value))
const webBase = computed(() => chat.baseUrl.value.replace(/\/api\/?$/, ''))
const projectName = computed(() => store.state.rootPath?.split(/[\\/]/).filter(Boolean).pop() ?? '')
const branchTaskKey = computed(() => taskKeyFromText(branch.value, tasks.projectKey.value))
const branchTask = computed(() => tasks.findByKey(branchTaskKey.value))

function flash(msg: string) {
  notice.value = msg
  setTimeout(() => { if (notice.value === msg) notice.value = '' }, 3500)
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

// ── sharing ────────────────────────────────────────────────────────────────
/** Screenshot or image → compressed below 300 KB → attached to the next message. */
async function attachImage(file: Blob | null | undefined, name = 'captura') {
  if (!file || !file.type.startsWith('image/')) return
  compressing.value = true
  try {
    const img = await compressForChat(file)
    attach.value = { kind: 'image', name: `${name} · ${img.width}×${img.height} · ${Math.round(img.bytes / 1024)} KB`, content: img.dataUrl }
    if (!text.value.trim()) text.value = t('owDefaultImage')
  } catch {
    flash(t('owImageTooLarge'))
  } finally {
    compressing.value = false
  }
}
/** The Flows tab is sent as a picture of the whole flow (like a screenshot), compressed below 300 KB. */
async function attachFlowSnapshot() {
  if (!flowState.nodes.some((n: any) => !n.hidden)) { flash(t('owNoFlow')); return }
  const title = flowState.mode === 'map' ? t('owProjectMap') : flowState.flowName
  try {
    const png = await renderFlowPng(flowState.nodes, flowState.edges, title)
    await attachImage(png, `🔀 ${title}`)
    if (attach.value?.kind === 'image' && text.value === t('owDefaultImage')) text.value = t('owDefaultFlow')
  } catch {
    flash(t('owImageTooLarge'))
  }
}
function onPaste(e: ClipboardEvent) {
  const item = [...(e.clipboardData?.items ?? [])].find((i) => i.type.startsWith('image/'))
  if (!item) return
  e.preventDefault()
  void attachImage(item.getAsFile())
}
function onDrop(e: DragEvent) {
  const file = [...(e.dataTransfer?.files ?? [])].find((f) => f.type.startsWith('image/'))
  if (file) void attachImage(file, file.name.replace(/\.[^.]+$/, ''))
}

async function pick(kind: OweemeAttachment['kind']) {
  showShareMenu.value = false
  if (kind === 'image') { imageInput.value?.click(); return }
  if (kind === 'flow') { await attachFlowSnapshot(); return }
  const a = kind === 'code' ? share.selection() : kind === 'file' ? share.currentFile() : await share.preview()
  if (!a) { flash(t(({ code: 'owNoSelection', file: 'owNoFile', flow: 'owNoFlow', preview: 'owNoPreview' } as const)[kind])); return }
  attach.value = a
  if (!text.value.trim()) text.value = t(({ code: 'owDefaultCode', file: 'owDefaultFile', flow: 'owDefaultFlow', preview: 'owDefaultPreview' } as const)[kind])
}

async function send() {
  if (!text.value.trim() || sending.value) return
  sending.value = true
  const context = attach.value && (attach.value.kind === 'code' || attach.value.kind === 'file') ? [attach.value.name, branch.value].filter(Boolean).join(' · ') : ''
  const hadAttachment = !!attach.value
  const sent = await chat.send(text.value, context, attach.value)
  if (sent) {
    // An app whose Rust core is older than the panel drops the attachment silently:
    // say so instead of leaving the user wondering where the image went.
    if (hadAttachment && !sent.attachment) chat.error.value = t('owRestartNeeded')
    text.value = ''
    attach.value = null
  }
  sending.value = false
}

/** Splits text into plain parts and links, so URLs in messages open in the system browser. */
const URL_RE = /(https?:\/\/[^\s<>"')\]]+)/g
function parts(text: string): Array<{ t: string; url?: string }> {
  return text.split(URL_RE).filter(Boolean).map((p) => (/^https?:\/\//.test(p) ? { t: p, url: p.replace(/[.,;:!?]+$/, '') } : { t: p }))
}
function openExternal(url: string) { void openUrl(url).catch((e) => (chat.error.value = String(e))) }

const previewOf = (a: OweemeAttachment): PreviewShare | null => { try { return JSON.parse(a.content) } catch { return null } }
const flowSize = (a: OweemeAttachment) => { try { const f = JSON.parse(a.content); return `${f.nodes?.length ?? 0} · ${f.edges?.length ?? 0}` } catch { return '' } }
const lines = (s: string) => s.split('\n').length
async function copy(s: string) { await navigator.clipboard.writeText(s); flash(t('owCopied')) }
async function openShared(a: OweemeAttachment) { if (!(await share.openInProject(a))) flash(t('owNotInProject')) }
function importFlow(a: OweemeAttachment) {
  if (!window.confirm(t('owImportFlowConfirm', { name: a.name }))) return
  share.importFlow(a)
}

// ── rooms ──────────────────────────────────────────────────────────────────
async function createRoom() {
  if (await chat.createRoom(newRoom.value.name, newRoom.value.repo)) showNewRoom.value = false
}
function openNewRoom() {
  newRoom.value = { name: projectName.value, repo: remote.value ? 'https://' + remote.value.replace(/^git@([^:]+):/, '$1/').replace(/^[a-z]+:\/\//, '').replace(/\.git$/, '') : '' }
  showNewRoom.value = true
}

// ── tasks ──────────────────────────────────────────────────────────────────
async function commentTask() {
  if (!tasks.openTask.value) return
  if (await tasks.comment(tasks.openTask.value.uuid, commentText.value)) commentText.value = ''
}

/** After a commit that mentions a task key: comment it on the task and move it to Review. */
async function linkCommit() {
  const c = pendingCommit.value
  pendingCommit.value = null
  const task = c && tasks.findByKey(c.key)
  if (!c || !task) return
  await tasks.comment(task.uuid, `${t('owCommitComment')} ${branch.value ? `(${branch.value})` : ''}\n${c.message}`)
  const review = tasks.reviewList()
  if (review && review.uuid !== task.list.uuid) await tasks.move(task.uuid, review.uuid)
  flash(t('owCommitLinked', { key: c.key }))
}

async function onCommitted(e: Event) {
  const { message } = (e as CustomEvent<{ message: string }>).detail
  await readGit()
  const key = taskKeyFromText(message, tasks.projectKey.value) ?? branchTaskKey.value
  if (!key) return
  if (!tasks.lists.value.length) await tasks.loadBoards()
  if (tasks.findByKey(key)) pendingCommit.value = { key, message }
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
watch([() => chat.activeRoom.value?.projectUuid, view], async ([, v]) => {
  if (v === 'tasks') await tasks.loadBoards()
})

onMounted(async () => {
  window.addEventListener('oweecode-git-committed', onCommitted)
  await chat.init()
  if (chat.connected.value) await afterConnect()
})
onUnmounted(() => {
  chat.stopPolling()
  window.removeEventListener('oweecode-git-committed', onCommitted)
})
</script>

<template>
  <div class="ow-panel">
    <div class="ow-header">
      <span class="ow-title">Oweeme</span>
      <span v-if="chat.connected.value" class="ow-header-actions">
        <button class="ow-icon-btn" :title="t('owRefresh')" @click="view === 'chat' ? chat.refreshRooms() : tasks.loadKanban()">⟳</button>
        <button class="ow-icon-btn" :title="t('owNewRoom')" @click="openNewRoom">＋</button>
        <button class="ow-icon-btn" :title="t('owDisconnect')" @click="chat.disconnect()">⏻</button>
      </span>
    </div>

    <div v-if="needsNewToken" class="ow-banner">
      <span>{{ t('owNeedSdlc') }}</span>
      <div class="ow-row">
        <button class="ow-btn" @click="openUrl(webBase + '/dashboard/professional/team')">{{ t('owOpenTokens') }}</button>
        <button class="ow-btn ow-btn-main" @click="chat.disconnect()">{{ t('owChangeToken') }}</button>
      </div>
    </div>
    <div v-else-if="chat.error.value" class="ow-error">{{ chat.error.value }}</div>
    <div v-if="notice" class="ow-notice">{{ notice }}</div>

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

      <div class="ow-tabs">
        <button :class="{ on: view === 'chat' }" @click="view = 'chat'">{{ t('owTabChat') }}</button>
        <button :class="{ on: view === 'tasks' }" @click="view = 'tasks'">
          {{ t('owTabTasks') }}<span v-if="branchTaskKey" class="ow-pill">{{ branchTaskKey }}</span>
        </button>
      </div>

      <!-- ═════════════ CHAT ═════════════ -->
      <template v-if="view === 'chat'">
        <div ref="listEl" class="ow-messages">
          <div v-if="!chat.activeRoom.value" class="ow-empty">{{ t('owPickRoomHint') }}</div>
          <div v-for="m in chat.messages.value" :key="m.uuid" class="ow-msg" :class="{ mine: m.isMine }">
            <div class="ow-meta">{{ m.senderName }} · {{ time(m.createdAt) }}</div>
            <div class="ow-text"><template v-for="(p, i) in parts(m.message)" :key="i"><a v-if="p.url" href="#" class="ow-url" :title="p.url" @click.prevent="openExternal(p.url)">{{ p.t }}</a><template v-else>{{ p.t }}</template></template></div>
            <pre v-if="m.codeContext && !m.attachment" class="ow-code">{{ m.codeContext }}</pre>

            <!-- attachments -->
            <div v-if="m.attachment" class="ow-att">
              <template v-if="m.attachment.kind === 'code' || m.attachment.kind === 'file'">
                <div class="ow-att-head">{{ m.attachment.kind === 'file' ? '📄' : '✂' }} {{ m.attachment.name }} · {{ lines(m.attachment.content) }} {{ t('owLines') }}</div>
                <pre class="ow-code">{{ m.attachment.content }}</pre>
                <div class="ow-att-actions">
                  <button class="ow-link" @click="copy(m.attachment.content)">{{ t('owCopy') }}</button>
                  <button class="ow-link" @click="openShared(m.attachment)">{{ t('owOpenInProject') }}</button>
                </div>
              </template>
              <div v-else-if="m.attachment.kind === 'image' && (m.attachment.expired || !m.attachment.content)" class="ow-dim ow-small">🖼 {{ t('owImageExpired', { n: m.attachment.retentionDays ?? 30 }) }}</div>
              <template v-else-if="m.attachment.kind === 'image'">
                <img :src="m.attachment.content" :alt="m.attachment.name" class="ow-img" @click="lightbox = m.attachment.content" />
                <div class="ow-dim ow-small">{{ m.attachment.name }} · {{ t('owImageRetention', { n: m.attachment.retentionDays ?? 30 }) }}</div>
              </template>
              <template v-else-if="m.attachment.kind === 'flow'">
                <div class="ow-att-head">🔀 {{ t('owFlow') }}: {{ m.attachment.name }} <span class="ow-dim">({{ flowSize(m.attachment) }})</span></div>
                <div class="ow-att-actions"><button class="ow-link" @click="importFlow(m.attachment)">{{ t('owImportFlow') }}</button></div>
              </template>
              <template v-else-if="m.attachment.kind === 'preview' && previewOf(m.attachment)">
                <div class="ow-att-head">🌐 {{ t('owPreview') }} <span v-if="previewOf(m.attachment)!.device" class="ow-dim">· {{ previewOf(m.attachment)!.device }}</span></div>
                <a href="#" class="ow-mono ow-url ow-ellipsis" @click.prevent="openExternal(previewOf(m.attachment)!.lanUrl ?? previewOf(m.attachment)!.url)">{{ previewOf(m.attachment)!.lanUrl ?? previewOf(m.attachment)!.url }}</a>
                <div class="ow-att-actions">
                  <button class="ow-link" @click="share.openPreview(m.attachment)">{{ t('owOpenPreview') }}</button>
                  <button class="ow-link" @click="copy(previewOf(m.attachment)!.lanUrl ?? previewOf(m.attachment)!.url)">{{ t('owCopy') }}</button>
                </div>
                <div v-if="!previewOf(m.attachment)!.lanUrl && /\/\/(localhost|127\.0\.0\.1|0\.0\.0\.0)/.test(previewOf(m.attachment)!.url)" class="ow-dim ow-small">{{ t('owPreviewLocalOnly') }}</div>
              </template>
            </div>
          </div>
        </div>

        <div v-if="chat.activeRoom.value" class="ow-composer" @dragover.prevent @drop.prevent="onDrop">
          <div v-if="compressing" class="ow-attach"><span>🖼 {{ t('owCompressing') }}</span></div>
          <div v-if="attach" class="ow-attach">
            <img v-if="attach.kind === 'image'" :src="attach.content" class="ow-thumb" alt="" />
            <span class="ow-ellipsis">{{ ({ code: '✂', file: '📄', flow: '🔀', preview: '🌐', image: '🖼' })[attach.kind] }} {{ attach.name }}</span>
            <button class="ow-icon-btn" @click="attach = null">✕</button>
          </div>
          <textarea v-model="text" class="ow-input ow-textarea" rows="2" :placeholder="t('owMessagePh')" @keydown.enter.exact.prevent="send" @paste="onPaste" />
          <div class="ow-row" style="position: relative">
            <button class="ow-btn" @click.stop="showShareMenu = !showShareMenu">＋ {{ t('owShare') }}</button>
            <div v-if="showShareMenu" class="ow-menu" @click.stop>
              <button @click="pick('code')">✂ {{ t('owShareSelection') }}</button>
              <button @click="pick('file')">📄 {{ t('owShareFile') }}</button>
              <button @click="pick('flow')">🔀 {{ t('owShareFlow') }}</button>
              <button @click="pick('preview')">🌐 {{ t('owSharePreview') }}</button>
              <button @click="pick('image')">🖼 {{ t('owShareImage') }}</button>
              <div class="ow-menu-hint">{{ t('owImageHint') }}</div>
            </div>
            <input ref="imageInput" type="file" accept="image/*" hidden @change="attachImage(($event.target as HTMLInputElement).files?.[0]); ($event.target as HTMLInputElement).value = ''" />
            <span class="ow-spacer" />
            <button class="ow-btn ow-btn-main" :disabled="!text.trim() || sending" @click="send">{{ t('owSend') }}</button>
          </div>
        </div>
      </template>

      <!-- ═════════════ TASKS ═════════════ -->
      <template v-else>
        <div v-if="pendingCommit" class="ow-banner">
          <span>{{ t('owCommitAsk', { key: pendingCommit.key }) }}</span>
          <div class="ow-row">
            <button class="ow-btn ow-btn-main" @click="linkCommit">{{ t('owYes') }}</button>
            <button class="ow-btn" @click="pendingCommit = null">{{ t('owNo') }}</button>
          </div>
        </div>

        <div v-if="!chat.activeRoom.value?.projectUuid" class="ow-empty">{{ t('owNoProject') }}</div>

        <template v-else-if="tasks.openTask.value">
          <div class="ow-task">
            <button class="ow-link" @click="tasks.openTaskByUuid(null)">← {{ t('owBack') }}</button>
            <div class="ow-mono ow-dim">{{ tasks.openTask.value.key }} · {{ tasks.openTask.value.list.name }}</div>
            <h3 class="ow-task-title">{{ tasks.openTask.value.title }}</h3>
            <p v-if="tasks.openTask.value.description" class="ow-text">{{ tasks.openTask.value.description }}</p>
            <label class="ow-label">{{ t('owMoveTo') }}</label>
            <div class="ow-chips">
              <button v-for="l in tasks.lists.value" :key="l.uuid" class="ow-chip" :class="{ on: l.uuid === tasks.openTask.value.list.uuid }" @click="tasks.move(tasks.openTask.value!.uuid, l.uuid)">{{ l.name }}</button>
            </div>
            <label class="ow-label">{{ t('owComments') }}</label>
            <div v-for="c in tasks.comments.value" :key="c.uuid" class="ow-msg" :class="{ mine: c.isMe }">
              <div class="ow-meta">{{ c.authorName }} · {{ time(c.createdAt) }}</div>
              <div class="ow-text"><template v-for="(p, i) in parts(c.body)" :key="i"><a v-if="p.url" href="#" class="ow-url" :title="p.url" @click.prevent="openExternal(p.url)">{{ p.t }}</a><template v-else>{{ p.t }}</template></template></div>
            </div>
            <textarea v-model="commentText" class="ow-input ow-textarea" rows="2" :placeholder="t('owCommentPh')" @keydown.enter.exact.prevent="commentTask" />
            <button class="ow-btn ow-btn-main" :disabled="!commentText.trim()" @click="commentTask">{{ t('owComment') }}</button>
          </div>
        </template>

        <div v-else class="ow-board">
          <select v-if="tasks.boards.value.length > 1" class="ow-input" :value="tasks.boardId.value ?? ''" @change="tasks.selectBoard(($event.target as HTMLSelectElement).value)">
            <option v-for="b in tasks.boards.value" :key="b.uuid" :value="b.uuid">{{ b.name }}</option>
          </select>
          <button v-if="branchTask" class="ow-current" @click="tasks.openTaskByUuid(branchTask.uuid)">
            ▶ {{ t('owWorkingOn') }} <b>{{ branchTask.key }}</b> — {{ branchTask.title }}
          </button>
          <div v-for="l in tasks.lists.value" :key="l.uuid" class="ow-col">
            <div class="ow-col-head">{{ l.name }} <span class="ow-dim">{{ l.tasks.length }}</span></div>
            <button v-for="tk in l.tasks" :key="tk.uuid" class="ow-card" :class="'p-' + tk.priority" @click="tasks.openTaskByUuid(tk.uuid)">
              <span class="ow-mono ow-dim">{{ tk.key }}</span> {{ tk.title }}
              <span v-if="tk.commentCount" class="ow-dim"> · 💬 {{ tk.commentCount }}</span>
            </button>
          </div>
          <div v-if="tasks.lists.value.length" class="ow-row">
            <input v-model="newTaskTitle" class="ow-input" :placeholder="t('owNewTask')" @keydown.enter="tasks.createTask(tasks.lists.value[0].uuid, newTaskTitle).then(() => (newTaskTitle = ''))" />
          </div>
          <div v-if="tasks.loading.value && !tasks.lists.value.length" class="ow-empty">…</div>
        </div>
      </template>
    </template>
    <div v-if="lightbox" class="ow-lightbox" @click="lightbox = null"><img :src="lightbox" alt="" /></div>
  </div>
</template>

<style scoped>
.ow-panel { display: flex; flex-direction: column; height: 100%; font-family: var(--font-ui); font-size: 12px; color: var(--fg); }
.ow-header { display: flex; align-items: center; justify-content: space-between; padding: 0 12px; height: 36px; flex-shrink: 0; border-bottom: 1px solid var(--border); }
.ow-title { font-size: 10px; font-weight: 700; letter-spacing: 1px; color: var(--fg-muted); text-transform: uppercase; }
.ow-header-actions { display: flex; gap: 2px; }
.ow-icon-btn { background: none; border: none; color: var(--fg-muted); cursor: pointer; padding: 2px 6px; border-radius: 4px; font-size: 13px; }
.ow-icon-btn:hover { color: var(--fg); background: var(--bg-hover); }
.ow-error { margin: 8px 12px 0; padding: 8px 10px; background: rgba(248,81,73,0.12); border: 1px solid rgba(248,81,73,0.3); border-radius: 6px; font-size: 11px; color: #f85149; line-height: 1.5; }
.ow-notice { margin: 8px 12px 0; padding: 6px 10px; background: var(--bg-active); border-radius: 6px; font-size: 11px; }
.ow-connect, .ow-newroom { display: flex; flex-direction: column; gap: 6px; padding: 12px; }
.ow-newroom { border-bottom: 1px solid var(--border); }
.ow-lead { margin: 0 0 6px; color: var(--fg-dim); line-height: 1.5; }
.ow-label { font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: .5px; color: var(--fg-muted); margin-top: 6px; display: block; }
.ow-input { width: 100%; box-sizing: border-box; background: var(--bg-dark); border: 1px solid var(--border); color: var(--fg); border-radius: 5px; padding: 6px 8px; font: inherit; }
.ow-input:focus { outline: none; border-color: var(--accent); }
.ow-mono { font-family: var(--font-mono); }
.ow-dim { color: var(--fg-muted); }
.ow-small { font-size: 10px; margin-top: 4px; }
.ow-ellipsis { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
.ow-textarea { resize: vertical; min-height: 40px; }
.ow-room-select { margin: 8px 12px 0; width: calc(100% - 24px); }
.ow-tabs { display: flex; gap: 2px; margin: 8px 12px 0; border-bottom: 1px solid var(--border); }
.ow-tabs button { background: none; border: none; border-bottom: 2px solid transparent; color: var(--fg-muted); padding: 6px 10px; cursor: pointer; font: inherit; font-weight: 600; margin-bottom: -1px; }
.ow-tabs button.on { color: var(--fg-bright); border-bottom-color: var(--accent); }
.ow-pill { margin-left: 6px; font-size: 10px; background: var(--bg-active); border-radius: 999px; padding: 1px 6px; font-family: var(--font-mono); }
.ow-btn { background: var(--bg-mid); border: 1px solid var(--border); color: var(--fg); border-radius: 5px; padding: 5px 10px; cursor: pointer; font: inherit; }
.ow-btn:hover:not(:disabled) { border-color: var(--accent); }
.ow-btn:disabled { opacity: .45; cursor: default; }
.ow-btn-main { background: var(--accent); color: var(--accent-fg); border-color: transparent; font-weight: 600; }
.ow-link { background: none; border: none; color: var(--accent); cursor: pointer; padding: 0; font: inherit; align-self: flex-start; }
.ow-link:hover { text-decoration: underline; }
.ow-url { color: var(--accent); text-decoration: underline; word-break: break-all; }
.ow-hint { margin: 6px 0 0; color: var(--fg-muted); font-size: 11px; line-height: 1.5; }
.ow-hint a { color: var(--accent); }
.ow-messages, .ow-board, .ow-task { flex: 1; overflow-y: auto; padding: 10px 12px; display: flex; flex-direction: column; gap: 8px; }
.ow-empty { color: var(--fg-muted); text-align: center; margin: 24px 12px; line-height: 1.5; }
.ow-msg { max-width: 92%; align-self: flex-start; background: var(--bg-mid); border-radius: 8px; padding: 6px 9px; }
.ow-msg.mine { align-self: flex-end; background: var(--bg-active); }
.ow-meta { font-size: 10px; color: var(--fg-muted); margin-bottom: 2px; }
.ow-text { white-space: pre-wrap; word-break: break-word; line-height: 1.45; margin: 0; }
.ow-code { margin: 6px 0 0; padding: 6px 8px; background: var(--bg-darkest); border-radius: 5px; font-family: var(--font-mono); font-size: 11px; white-space: pre; overflow: auto; max-height: 220px; }
.ow-att { margin-top: 6px; border-top: 1px solid var(--border); padding-top: 6px; display: flex; flex-direction: column; gap: 4px; }
.ow-att-head { font-weight: 600; font-size: 11px; }
.ow-att-actions { display: flex; gap: 12px; font-size: 11px; }
.ow-composer { border-top: 1px solid var(--border); padding: 8px 12px; display: flex; flex-direction: column; gap: 6px; }
.ow-attach { display: flex; align-items: center; justify-content: space-between; gap: 6px; font-size: 11px; color: var(--fg-dim); background: var(--bg-mid); border-radius: 5px; padding: 3px 4px 3px 8px; }
.ow-menu { position: absolute; bottom: 34px; left: 0; z-index: 5; background: var(--bg-panel); border: 1px solid var(--border); border-radius: 6px; padding: 4px; display: flex; flex-direction: column; min-width: 190px; box-shadow: 0 8px 24px rgba(0,0,0,.4); }
.ow-menu button { background: none; border: none; color: var(--fg); text-align: left; padding: 6px 8px; border-radius: 4px; cursor: pointer; font: inherit; }
.ow-menu button:hover { background: var(--bg-hover); }
.ow-row { display: flex; gap: 6px; align-items: center; }
.ow-img { max-width: 100%; max-height: 220px; border-radius: 6px; cursor: zoom-in; display: block; border: 1px solid var(--border); }
.ow-thumb { width: 28px; height: 28px; object-fit: cover; border-radius: 4px; flex: none; }
.ow-menu-hint { font-size: 10px; color: var(--fg-muted); padding: 4px 8px 2px; line-height: 1.4; border-top: 1px solid var(--border); margin-top: 2px; }
.ow-lightbox { position: fixed; inset: 0; z-index: 1000; background: rgba(0,0,0,.8); display: grid; place-items: center; cursor: zoom-out; }
.ow-lightbox img { max-width: 92vw; max-height: 92vh; border-radius: 8px; box-shadow: 0 20px 60px rgba(0,0,0,.6); }
.ow-spacer { flex: 1; }
.ow-banner { margin: 8px 12px 0; padding: 8px 10px; border: 1px solid var(--accent); border-radius: 6px; display: flex; flex-direction: column; gap: 6px; line-height: 1.4; }
.ow-current { text-align: left; background: var(--bg-active); border: 1px solid var(--accent); color: var(--fg); border-radius: 6px; padding: 6px 8px; cursor: pointer; font: inherit; }
.ow-col { display: flex; flex-direction: column; gap: 4px; }
.ow-col-head { font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: .5px; color: var(--fg-muted); margin-top: 4px; }
.ow-card { text-align: left; background: var(--bg-mid); border: 1px solid var(--border); border-left-width: 3px; color: var(--fg); border-radius: 5px; padding: 6px 8px; cursor: pointer; font: inherit; line-height: 1.4; }
.ow-card:hover { border-color: var(--accent); }
.ow-card.p-urgent { border-left-color: #f85149; }
.ow-card.p-high { border-left-color: #d29922; }
.ow-task-title { margin: 2px 0; font-size: 14px; color: var(--fg-bright); }
.ow-chips { display: flex; flex-wrap: wrap; gap: 4px; }
.ow-chip { background: var(--bg-mid); border: 1px solid var(--border); color: var(--fg); border-radius: 999px; padding: 3px 10px; cursor: pointer; font: inherit; font-size: 11px; }
.ow-chip.on { background: var(--accent); color: var(--accent-fg); border-color: transparent; }
</style>
