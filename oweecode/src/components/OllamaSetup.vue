<script setup lang="ts">
// Guided setup for the local agent: detect Ollama, show what's about to be
// installed/downloaded and let the user accept before anything runs, install
// it (reusing CliPanel.vue's exact pty_create + pty_write(installCmd)
// pattern), then pull the two recommended models with live progress.
import { ref, onMounted, onBeforeUnmount } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { Terminal as XTerm } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { useEditorStore } from '../composables/useEditorStore'
import { useI18n } from '../composables/useI18n'

const emit = defineEmits<{ (e: 'close'): void }>()
const store = useEditorStore()

interface OllamaStatus { installed: boolean; running: boolean; version: string | null }
interface PullProgress { model: string; status: string; completed: number; total: number }

const { t } = useI18n()
const status = ref<OllamaStatus | null>(null)
const checking = ref(true)
const accepted = ref(false)

const RECOMMENDED_MODELS: { id: string; label: 'oM3' | 'oM7'; size: string; hint: 'oM3h' | 'oM7h' }[] = [
  { id: 'qwen2.5-coder:3b-instruct-q4_K_M', label: 'oM3', size: '~2 GB', hint: 'oM3h' },
  { id: 'qwen2.5-coder:7b-instruct-q4_K_M', label: 'oM7', size: '~4.5 GB', hint: 'oM7h' },
]

const installedModels = ref<string[]>([])
const pulling = ref<Record<string, PullProgress>>({})
const customModel = ref('')

// ollama.com's curl script (install.sh) is Linux-only — it checks for
// systemd/apt/etc. and doesn't run on macOS. Both Windows and macOS need a
// downloaded installer instead (the template branches on `platform`).
const installCmd = 'curl -fsSL https://ollama.com/install.sh | sh'
const platform = navigator.userAgent.includes('Windows') ? 'windows'
  : navigator.userAgent.includes('Mac') ? 'mac'
  : 'linux'

let unlistenProgress: UnlistenFn | null = null

async function refreshStatus() {
  checking.value = true
  try {
    status.value = await invoke<OllamaStatus>('ollama_check_installed')
    if (status.value.running) await refreshModels()
  } finally {
    checking.value = false
  }
}

async function refreshModels() {
  try { installedModels.value = await invoke<string[]>('list_ollama_models') }
  catch { installedModels.value = [] }
}

function hasModel(id: string) { return installedModels.value.includes(id) }

async function pullModel(id: string) {
  pulling.value = { ...pulling.value, [id]: { model: id, status: 'iniciando…', completed: 0, total: 0 } }
  try {
    await invoke('ollama_pull_model', { model: id })
    await refreshModels()
  } catch (e: any) {
    pulling.value = { ...pulling.value, [id]: { model: id, status: `Error: ${e}`, completed: 0, total: 0 } }
    return
  }
  const next = { ...pulling.value }
  delete next[id]
  pulling.value = next
}

function progressPct(p: PullProgress): number {
  return p.total > 0 ? Math.round((p.completed / p.total) * 100) : 0
}

// ── Embedded terminal, reused for both the install script and `ollama
// signin` — the two flows are structurally identical (spawn a shell, type a
// command, watch it run, react when it exits) and never run at the same
// time, so one xterm instance covers both instead of duplicating the setup.
const termEl = ref<HTMLElement | null>(null)
const activeTerminal = ref<'none' | 'install' | 'signin'>('none')
const signedIn = ref(false)
let xterm: XTerm | null = null
let fitAddon: FitAddon | null = null
let unlistenData: UnlistenFn | null = null
let unlistenExit: UnlistenFn | null = null
let currentSessionId = ''
const INSTALL_SESSION = 'ollama-install-session'
const SIGNIN_SESSION = 'ollama-signin-session'

async function runInTerminal(sessionId: string, command: string, onExit: () => void | Promise<void>) {
  unlistenData?.(); unlistenExit?.(); xterm?.dispose()
  currentSessionId = sessionId
  await nextFrame()
  if (!termEl.value) return

  xterm = new XTerm({
    theme: { background: '#0d1117', foreground: '#e6edf3', cursor: '#a6e3a1' },
    fontFamily: '"JetBrains Mono", "Fira Code", Consolas, monospace',
    fontSize: 12, lineHeight: 1.5, cursorBlink: true, scrollback: 5000,
  })
  fitAddon = new FitAddon()
  xterm.loadAddon(fitAddon)
  xterm.open(termEl.value)
  setTimeout(() => fitAddon?.fit(), 60)

  unlistenData = await listen<string>(`pty:${sessionId}`, (ev) => xterm?.write(ev.payload))
  unlistenExit = await listen<string>(`pty-exit:${sessionId}`, async () => {
    xterm?.writeln('\r\n\x1b[32m[Listo]\x1b[0m')
    await new Promise(r => setTimeout(r, 600))
    await onExit()
  })

  try {
    const cwd = store.state.rootPath || (await invoke<string>('get_home_dir').catch(() => '/'))
    await invoke('pty_create', { id: sessionId, cwd, cols: 100, rows: 24 })
    xterm.writeln(`\x1b[36m$ ${command}\x1b[0m`)
    await invoke('pty_write', { id: sessionId, data: command + '\r' })
  } catch (e: any) {
    xterm.writeln(`\r\n\x1b[31mError: ${e}\x1b[0m`)
  }
}

async function startInstall() {
  accepted.value = true
  activeTerminal.value = 'install'
  await runInTerminal(INSTALL_SESSION, installCmd, refreshStatus)
}

// `ollama signin` prints a one-time URL/code to open in the browser and
// blocks until that finishes — there's no `ollama whoami`-style command to
// query auth state afterwards, so this only tracks "ran signin successfully
// at least once this session" rather than a real signed-in/out status.
async function startSignin() {
  activeTerminal.value = 'signin'
  await runInTerminal(SIGNIN_SESSION, 'ollama signin', async () => { signedIn.value = true })
}

function nextFrame() { return new Promise(r => setTimeout(r, 0)) }

onMounted(async () => {
  unlistenProgress = await listen<PullProgress>('ollama-pull-progress', (ev) => {
    if (pulling.value[ev.payload.model]) pulling.value = { ...pulling.value, [ev.payload.model]: ev.payload }
  })
  await refreshStatus()
})

onBeforeUnmount(async () => {
  unlistenProgress?.()
  unlistenData?.()
  unlistenExit?.()
  if (activeTerminal.value !== 'none') await invoke('pty_kill', { id: currentSessionId }).catch(() => {})
  xterm?.dispose()
})
</script>

<template>
  <Teleport to="body">
    <div class="ollama-overlay" @click.self="emit('close')">
      <div class="ollama-modal">
        <div class="ollama-modal-head">
          <span class="ollama-modal-title">{{ t('oTitle') }}</span>
          <button class="ollama-modal-close" @click="emit('close')">✕</button>
        </div>

        <div v-if="checking" class="ollama-step">
          <div class="mini-spin" /> {{ t('oChecking') }}
        </div>

        <!-- Terms + install -->
        <template v-else-if="!status?.installed && activeTerminal !== 'install'">
          <div class="ollama-step">
            <p v-html="t('oIntro')" />
            <ul class="ollama-terms">
              <li>{{ t('oTerm1') }}</li>
              <li>{{ t('oTerm2') }}</li>
              <li>{{ t('oTerm3') }}</li>
              <li>{{ t('oTerm4') }}</li>
            </ul>
            <template v-if="platform === 'linux'">
              <div class="ollama-cmd-preview">{{ installCmd }}</div>
              <button class="ollama-btn ollama-btn--primary" @click="startInstall">{{ t('oAccept') }}</button>
            </template>
            <template v-else-if="platform === 'mac'">
              <p class="ollama-hint" v-html="t('oMacHint')" />
              <a class="ollama-btn ollama-btn--primary" href="https://ollama.com/download/mac" target="_blank">{{ t('oMacBtn') }}</a>
              <button class="ollama-btn" @click="refreshStatus">{{ t('oDone') }}</button>
            </template>
            <template v-else>
              <p class="ollama-hint">{{ t('oWinHint') }}</p>
              <a class="ollama-btn ollama-btn--primary" href="https://ollama.com/download/windows" target="_blank">{{ t('oWinBtn') }}</a>
              <button class="ollama-btn" @click="refreshStatus">{{ t('oDone') }}</button>
            </template>
          </div>
        </template>

        <!-- Installing (embedded terminal) -->
        <template v-else-if="activeTerminal === 'install' && !status?.installed">
          <div class="ollama-step">
            <p>{{ t('oInstalling') }}</p>
            <div ref="termEl" class="ollama-term"></div>
          </div>
        </template>

        <!-- Installed but service not reachable yet -->
        <template v-else-if="status?.installed && !status?.running">
          <div class="ollama-step">
            <p>{{ t('oNoService', { v: status.version ?? '' }) }}</p>
            <p class="ollama-hint" v-html="t('oServeHint')" />
            <button class="ollama-btn" @click="refreshStatus">{{ t('oRetry') }}</button>
          </div>
        </template>

        <!-- Signing in to ollama.com (embedded terminal) -->
        <template v-else-if="activeTerminal === 'signin'">
          <div class="ollama-step">
            <p>{{ t('oSigning') }}</p>
            <div ref="termEl" class="ollama-term"></div>
            <button class="ollama-btn" @click="activeTerminal = 'none'">{{ t('oBack') }}</button>
          </div>
        </template>

        <!-- Running: account + model pull cards -->
        <template v-else-if="status?.running">
          <div class="ollama-step">
            <p class="ollama-hint">{{ t('oRunning', { v: status.version ?? '' }) }}</p>

            <div class="ollama-cloud-box">
              <div class="ollama-cloud-head">{{ t('oCloud') }}</div>
              <p class="ollama-model-hint" v-html="t('oCloudNote')"></p>
              <div v-if="signedIn" class="ollama-model-ready">{{ t('oSignedIn') }}</div>
              <button v-else class="ollama-btn" @click="startSignin">{{ t('oSignIn') }}</button>
            </div>

            <p class="ollama-hint">{{ t('oRecommended') }}</p>
            <div v-for="m in RECOMMENDED_MODELS" :key="m.id" class="ollama-model-card">
              <div class="ollama-model-head">
                <span class="ollama-model-label">{{ t(m.label) }}</span>
                <span class="ollama-model-size">{{ m.size }}</span>
              </div>
              <div class="ollama-model-id">{{ m.id }}</div>
              <p class="ollama-model-hint">{{ t(m.hint) }}</p>

              <div v-if="hasModel(m.id)" class="ollama-model-ready">{{ t('oAvailable') }}</div>
              <div v-else-if="pulling[m.id]" class="ollama-model-progress">
                <div class="ollama-progress-bar"><div class="ollama-progress-fill" :style="{ width: progressPct(pulling[m.id]) + '%' }" /></div>
                <span class="ollama-progress-label">{{ pulling[m.id].status }} {{ pulling[m.id].total ? progressPct(pulling[m.id]) + '%' : '' }}</span>
              </div>
              <button v-else class="ollama-btn" @click="pullModel(m.id)">{{ t('oDownload') }}</button>
            </div>

            <div class="ollama-custom-box">
              <div class="ollama-cloud-head">{{ t('oOther') }}</div>
              <p class="ollama-model-hint" v-html="t('oOtherNote')"></p>
              <div v-if="pulling[customModel]" class="ollama-model-progress">
                <div class="ollama-progress-bar"><div class="ollama-progress-fill" :style="{ width: progressPct(pulling[customModel]) + '%' }" /></div>
                <span class="ollama-progress-label">{{ pulling[customModel].status }} {{ pulling[customModel].total ? progressPct(pulling[customModel]) + '%' : '' }}</span>
              </div>
              <div v-else class="ollama-custom-row">
                <input v-model="customModel" class="ollama-custom-input" :placeholder="t('oCustomPh')" @keydown.enter="customModel.trim() && pullModel(customModel.trim())" />
                <button class="ollama-btn" :disabled="!customModel.trim()" @click="pullModel(customModel.trim())">{{ t('oDownload') }}</button>
              </div>
              <div v-if="hasModel(customModel.trim())" class="ollama-model-ready">{{ t('oAlready') }}</div>
            </div>

            <div v-if="installedModels.length" class="ollama-installed-list">
              <div class="ollama-cloud-head">{{ t('oInstalled') }}</div>
              <div v-for="m in installedModels" :key="m" class="ollama-installed-item">{{ m }}</div>
            </div>
          </div>
        </template>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.ollama-overlay {
  position: fixed; inset: 0; background: rgba(0,0,0,0.55);
  display: flex; align-items: center; justify-content: center; z-index: 200;
}
.ollama-modal {
  width: min(480px, 92vw); max-height: 80vh; overflow-y: auto;
  background: var(--bg-dark); border: 1px solid var(--border); border-radius: 10px;
  padding: 14px 16px 16px;
}
.ollama-modal-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 10px; }
.ollama-modal-title { font-size: 13px; font-weight: 700; color: var(--fg-bright); }
.ollama-modal-close { background: none; border: none; color: var(--fg-muted); cursor: pointer; font-size: 13px; }
.ollama-step { font-size: 12px; color: var(--fg); display: flex; flex-direction: column; gap: 8px; }
.ollama-terms { margin: 0; padding-left: 18px; color: var(--fg-muted); font-size: 11.5px; line-height: 1.6; }
.ollama-cmd-preview {
  font-family: var(--font-mono); font-size: 10.5px; color: #a6e3a1;
  background: var(--bg-darker); border-radius: 6px; padding: 6px 8px; word-break: break-all;
}
.ollama-hint { font-size: 11px; color: var(--fg-muted); }
.ollama-term { height: 220px; background: #0d1117; border-radius: 6px; padding: 4px; }
.ollama-btn {
  align-self: flex-start; background: var(--bg-hover); color: var(--fg);
  border: 1px solid var(--border); border-radius: 6px; padding: 6px 14px;
  font-size: 11.5px; font-weight: 600; cursor: pointer;
}
.ollama-btn--primary { background: #a6e3a1; color: #0d1117; border: none; }
.ollama-btn--primary:hover { opacity: 0.9; }
.ollama-cloud-box {
  border: 1px solid rgba(137,180,250,0.3); background: rgba(137,180,250,0.06);
  border-radius: 8px; padding: 10px 12px; display: flex; flex-direction: column; gap: 6px;
}
.ollama-cloud-head { font-size: 12px; font-weight: 700; color: #89b4fa; }
.ollama-custom-box {
  border: 1px dashed var(--border); border-radius: 8px; padding: 10px 12px;
  display: flex; flex-direction: column; gap: 6px;
}
.ollama-custom-box a { color: #89b4fa; }
.ollama-custom-row { display: flex; gap: 6px; }
.ollama-custom-input {
  flex: 1; background: var(--bg-darker); border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg); font-size: 11.5px; font-family: var(--font-mono); padding: 6px 8px; outline: none;
}
.ollama-custom-input:focus { border-color: #a6e3a1; }
.ollama-installed-list { display: flex; flex-direction: column; gap: 4px; }
.ollama-installed-item { font-family: var(--font-mono); font-size: 10.5px; color: var(--fg-muted); }
.ollama-model-card { border: 1px solid var(--border); border-radius: 8px; padding: 10px 12px; display: flex; flex-direction: column; gap: 4px; }
.ollama-model-head { display: flex; justify-content: space-between; align-items: baseline; }
.ollama-model-label { font-size: 12px; font-weight: 700; color: var(--fg-bright); }
.ollama-model-size { font-size: 10.5px; color: var(--fg-muted); }
.ollama-model-id { font-family: var(--font-mono); font-size: 10px; color: var(--fg-muted); }
.ollama-model-hint { font-size: 10.5px; color: var(--fg-muted); margin: 0; }
.ollama-model-ready { font-size: 11px; color: #3fb950; font-weight: 600; }
.ollama-model-progress { display: flex; flex-direction: column; gap: 3px; }
.ollama-progress-bar { height: 5px; background: var(--bg-darker); border-radius: 3px; overflow: hidden; }
.ollama-progress-fill { height: 100%; background: #a6e3a1; transition: width 0.2s; }
.ollama-progress-label { font-size: 10px; color: var(--fg-muted); }
.mini-spin {
  width: 12px; height: 12px; border: 2px solid var(--border); border-top-color: #a6e3a1;
  border-radius: 50%; animation: spin 0.7s linear infinite; display: inline-block; vertical-align: middle;
}
@keyframes spin { to { transform: rotate(360deg); } }
</style>
