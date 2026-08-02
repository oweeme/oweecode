<script setup lang="ts">
/**
 * CliPanel — panel embebido con xterm PTY para Claude Code CLI y otros CLIs de IA.
 * Se abre como sidebar view (igual que FileExplorer) y corre el CLI directamente.
 */
import { ref, onMounted, onBeforeUnmount } from 'vue'
import { Terminal as XTerm } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { useEditorStore } from '../composables/useEditorStore'
import { useI18n } from '../composables/useI18n'
import { cliMeta, ensureFreshOpenrouterModel } from '../composables/aiClis'
import '@xterm/xterm/css/xterm.css'

const props = defineProps<{
  cli: string
}>()

const emit = defineEmits<{ 'open-file': [path: string] }>()

const store = useEditorStore()
const { t } = useI18n()
const termEl = ref<HTMLElement | null>(null)
const isRunning = ref(false)
// Stable per cli id — openCliTab() prevents more than one tab per tool, and this
// panel now stays mounted (v-show) instead of remounting on every tab switch.
const sessionId = `clipanel-${props.cli}`

let xterm: XTerm | null = null
let fitAddon: FitAddon | null = null
let unlistenData: UnlistenFn | null = null
let unlistenExit: UnlistenFn | null = null

function hexToRgba(hex: string, alpha: number): string {
  const n = parseInt(hex.replace('#', ''), 16)
  const r = (n >> 16) & 255, g = (n >> 8) & 255, b = n & 255
  return `rgba(${r},${g},${b},${alpha})`
}

const base = cliMeta(props.cli)
const meta = {
  ...base,
  bgColor: hexToRgba(base.color, 0.08),
  borderColor: hexToRgba(base.color, 0.25),
  shortcuts: ['↑↓ historial', 'Tab completar', '/help comandos'],
}

// ── Install prompt (shown when the CLI binary isn't found) ────────────────────
const needsInstall = ref(false)
const installing = ref(false)

// ── Compose bar (text + file/image attachments piped into the CLI's stdin) ────
interface Attachment { path: string; name: string }
const composeText = ref('')
const attachments = ref<Attachment[]>([])

const isWindows = navigator.userAgent.includes('Windows')
const displayInstallCmd = (isWindows && meta.installCmdWindows) ? meta.installCmdWindows : meta.installCmd

// Aider works with almost any LLM backend, so instead of leaving the user to set
// this up by hand, launch it pre-wired to whatever provider/model/API key is
// already configured in the AI Code Assistant panel — including OpenRouter and
// OmniRoute, which is what turns those into a real agentic CLI experience.
async function buildAiderLaunch(): Promise<{ env: [string, string][]; args: string[] }> {
  const provider = localStorage.getItem('ai_provider') ?? ''
  if (provider === 'openrouter') await ensureFreshOpenrouterModel()
  const apiKey = localStorage.getItem(`ai_key_${provider}`) ?? ''
  const model = localStorage.getItem('ai_model') ?? ''
  if (!provider || !apiKey) return { env: [], args: [] }

  const openaiCompatBase: Record<string, string> = {
    groq: 'https://api.groq.com/openai/v1',
    openrouter: 'https://openrouter.ai/api/v1',
    omniroute: 'http://localhost:20128/v1',
    deepseek: 'https://api.deepseek.com',
  }
  if (provider === 'claude') return { env: [['ANTHROPIC_API_KEY', apiKey]], args: [] }
  if (provider === 'gemini') return { env: [['GEMINI_API_KEY', apiKey]], args: [] }
  if (provider === 'openai') return { env: [['OPENAI_API_KEY', apiKey]], args: model ? ['--model', model] : [] }
  if (provider in openaiCompatBase) {
    return {
      env: [['OPENAI_API_KEY', apiKey], ['OPENAI_API_BASE', openaiCompatBase[provider]]],
      args: model ? ['--model', `openai/${model}`] : [],
    }
  }
  if (provider === 'ollama') return { env: [['OLLAMA_API_BASE', 'http://localhost:11434']], args: model ? ['--model', `ollama/${model}`] : [] }
  return { env: [], args: [] }
}

const PROVIDER_LABELS: Record<string, string> = {
  claude: 'Claude', openai: 'OpenAI', gemini: 'Gemini', deepseek: 'DeepSeek',
  groq: 'Groq', openrouter: 'OpenRouter', omniroute: 'OmniRoute', ollama: 'Ollama',
}
// Shown in the header so it's obvious Aider isn't its own separate "CLI provider" —
// it's a generic agentic CLI that channels whatever's picked in Configuración AI,
// OpenRouter/OmniRoute included. Refreshed every time it (re)starts, since the
// user may switch providers in the AI panel between runs.
const aiderProviderLabel = ref<string | null>(null)
function refreshAiderProviderLabel() {
  if (meta.id !== 'aider') return
  aiderProviderLabel.value = PROVIDER_LABELS[localStorage.getItem('ai_provider') ?? ''] ?? null
}

async function attachFiles() {
  const selected = await openDialog({ multiple: true })
  const paths = Array.isArray(selected) ? selected : selected ? [selected] : []
  for (const p of paths) {
    if (attachments.value.some(a => a.path === p)) continue
    attachments.value.push({ path: p, name: p.split(/[\\/]/).pop() ?? p })
  }
}

function removeAttachment(path: string) {
  attachments.value = attachments.value.filter(a => a.path !== path)
}

function sendCompose() {
  if (!isRunning.value) return
  const text = composeText.value.trim()
  if (!text && attachments.value.length === 0) return
  let msg = text
  if (attachments.value.length) {
    const list = attachments.value.map(a => `- ${a.path}`).join('\n')
    msg += (msg ? '\n\n' : '') + `${t('attachedFiles')}\n${list}`
  }
  invoke('pty_write', { id: sessionId, data: msg + '\r' }).catch(() => {})
  composeText.value = ''
  attachments.value = []
  xterm?.focus()
}

// OSC parser (same as Terminal.vue — for open-in-editor support)
let oscBuf = ''
let inOsc = false
function processOutput(data: string) {
  let visible = ''
  for (let i = 0; i < data.length; i++) {
    const ch = data[i]
    if (inOsc) {
      if (ch === '\x07') {
        if (oscBuf.startsWith('6634;')) emit('open-file', oscBuf.slice(5))
        oscBuf = ''; inOsc = false
      } else if (ch === '\x1b' && data[i + 1] === '\\') {
        if (oscBuf.startsWith('6634;')) emit('open-file', oscBuf.slice(5))
        oscBuf = ''; inOsc = false; i++
      } else { oscBuf += ch }
    } else if (ch === '\x1b' && data[i + 1] === ']') {
      inOsc = true; oscBuf = ''; i++
    } else { visible += ch }
  }
  if (visible) xterm?.write(visible)
}

onMounted(async () => {
  xterm = new XTerm({
    theme: {
      background:          '#0d1117',
      foreground:          '#e6edf3',
      cursor:              meta.color,
      cursorAccent:        '#0d1117',
      selectionBackground: hexToRgba(meta.color, 0.25),
      black:   '#21262d', red:     '#f85149',
      green:   '#3fb950', yellow:  '#d29922',
      blue:    '#79c0ff', magenta: '#d2a8ff',
      cyan:    '#56d8e4', white:   '#c9d1d9',
      brightBlack:   '#484f58', brightRed:     '#ff7b72',
      brightGreen:   '#56d364', brightYellow:  '#e3b341',
      brightBlue:    '#79c0ff', brightMagenta: '#d2a8ff',
      brightCyan:    '#56d8e4', brightWhite:   '#f0f6fc',
    },
    fontFamily: '"JetBrains Mono", "Fira Code", Consolas, monospace',
    fontSize: 12.5,
    lineHeight: 1.5,
    cursorBlink: true,
    cursorStyle: 'bar',
    scrollback: 10000,
  })

  fitAddon = new FitAddon()
  xterm.loadAddon(fitAddon)
  xterm.loadAddon(new WebLinksAddon())

  if (termEl.value) {
    xterm.open(termEl.value)
    setTimeout(() => { fitAddon?.fit() }, 80)
  }

  xterm.onData((data) => {
    invoke('pty_write', { id: sessionId, data }).catch(() => {})
  })

  unlistenData = await listen<string>(`pty:${sessionId}`, (ev) => {
    processOutput(ev.payload)
  })

  unlistenExit = await listen<string>(`pty-exit:${sessionId}`, () => {
    isRunning.value = false
    xterm?.writeln(`\r\n\x1b[33m[${meta.label} cerrado — presiona Enter para reiniciar]\x1b[0m`)
    xterm?.onData(() => { if (!isRunning.value) startCli() })
  })

  const ro = new ResizeObserver(() => {
    setTimeout(async () => {
      fitAddon?.fit()
      const raw = fitAddon?.proposeDimensions()
      const cols = Math.max(10, Math.round(raw?.cols ?? 0) || 80)
      const rows = Math.max(5,  Math.round(raw?.rows ?? 0) || 24)
      await invoke('pty_resize', { id: sessionId, cols, rows }).catch(() => {})
    }, 30)
  })
  if (termEl.value) ro.observe(termEl.value)
  onBeforeUnmount(() => ro.disconnect())

  await startCli()
})

onBeforeUnmount(async () => {
  unlistenData?.()
  unlistenExit?.()
  await invoke('pty_kill', { id: sessionId }).catch(() => {})
  xterm?.dispose()
})

async function startCli() {
  if (isRunning.value) return
  const el = termEl.value
  const isVisible = el != null && el.offsetWidth > 0 && el.offsetHeight > 0
  let cols = 80, rows = 24
  if (isVisible) {
    fitAddon?.fit()
    await new Promise(r => setTimeout(r, 40))
    const raw = fitAddon?.proposeDimensions()
    cols = Math.max(10, Math.round(raw?.cols ?? 80) || 80)
    rows = Math.max(5,  Math.round(raw?.rows ?? 24) || 24)
  }

  const cwd = store.state.rootPath || (await invoke<string>('get_home_dir').catch(() => '/'))

  refreshAiderProviderLabel()
  const launch = meta.id === 'aider' ? await buildAiderLaunch() : { env: [], args: [] }

  try {
    await invoke('pty_create', {
      id: sessionId,
      cwd,
      cols,
      rows,
      command: meta.command,
      extraEnv: launch.env,
      extraArgs: launch.args,
    })
    isRunning.value = true
    needsInstall.value = false
    installing.value = false
  } catch (e: any) {
    xterm?.writeln(`\r\n\x1b[31mError al iniciar ${meta.label}: ${e}\x1b[0m`)
    needsInstall.value = true
  }
}

async function restartCli() {
  await invoke('pty_kill', { id: sessionId }).catch(() => {})
  isRunning.value = false
  needsInstall.value = false
  installing.value = false
  xterm?.clear()
  await startCli()
}

async function installNow() {
  needsInstall.value = false
  installing.value = true
  const cwd = store.state.rootPath || (await invoke<string>('get_home_dir').catch(() => '/'))
  try {
    await invoke('pty_create', { id: sessionId, cwd, cols: 80, rows: 24 })
    isRunning.value = true
    xterm?.clear()
    xterm?.writeln(`\x1b[36m$ ${displayInstallCmd}\x1b[0m`)
    await invoke('pty_write', { id: sessionId, data: displayInstallCmd + '\r' })
  } catch (e: any) {
    xterm?.writeln(`\r\n\x1b[31mError al instalar: ${e}\x1b[0m`)
    installing.value = false
  }
}

async function finishInstall() {
  await invoke('pty_kill', { id: sessionId }).catch(() => {})
  isRunning.value = false
  installing.value = false
  xterm?.clear()
  await startCli()
}

function cancelInstall() { needsInstall.value = false }

// Send active file context as a message to the CLI
async function sendFileContext() {
  const tab = store.activeTab()
  if (!tab || tab.type !== 'code') return
  const sel = store.state.selectedText.trim()
  const text = sel
    ? `Mira este código seleccionado de ${tab.name}:\n\`\`\`${tab.language}\n${sel}\n\`\`\``
    : `Mira este archivo ${tab.name}:\n\`\`\`${tab.language}\n${tab.content.slice(0, 2000)}\n\`\`\``
  // Type it into the terminal (the CLI reads stdin)
  invoke('pty_write', { id: sessionId, data: text }).catch(() => {})
  xterm?.focus()
}
</script>

<template>
  <div class="cli-panel">
    <!-- Header -->
    <div class="cli-header" :style="{ borderBottomColor: meta.borderColor }">
      <div class="cli-header-left">
        <span class="cli-icon" :style="{ color: meta.color }">{{ meta.icon }}</span>
        <span class="cli-title">{{ meta.label }}</span>
        <span v-if="aiderProviderLabel" class="cli-provider-badge">⇄ {{ aiderProviderLabel }}</span>
        <span class="cli-status" :class="isRunning ? 'cli-status--on' : 'cli-status--off'">
          {{ isRunning ? t('active') : t('inactive') }}
        </span>
      </div>
      <div class="cli-header-right">
        <!-- Send file context button -->
        <button
          class="cli-hdr-btn"
          @click="sendFileContext"
          :disabled="!store.activeTab() || store.activeTab()?.type !== 'code'"
          :title="store.state.selectedText ? t('sendSelectionTitle') : t('sendFileTitle')"
        >
          <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor">
            <path d="M4 0h5.293A1 1 0 0 1 10 .293L13.707 4a1 1 0 0 1 .293.707V14a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V2a2 2 0 0 1 2-2zm5.5 1.5v2a1 1 0 0 0 1 1h2L9.5 1.5z"/>
          </svg>
          {{ store.state.selectedText ? t('sendSelection') : t('sendFile') }}
        </button>
        <!-- Restart button -->
        <button class="cli-hdr-btn cli-hdr-btn--icon" @click="restartCli" :title="t('restart')">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor">
            <path fill-rule="evenodd" d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2v1z"/>
            <path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466z"/>
          </svg>
        </button>
      </div>
    </div>

    <!-- Tip bar -->
    <div class="cli-tip" :style="{ background: meta.bgColor, borderBottomColor: meta.borderColor }">
      <span class="cli-tip-icon" :style="{ color: meta.color }">💡</span>
      <span>{{ meta.tip }}</span>
      <span v-for="s in meta.shortcuts" :key="s" class="cli-shortcut">{{ s }}</span>
    </div>

    <!-- Context indicator -->
    <div v-if="store.activeTab()?.type === 'code'" class="cli-ctx-bar">
      <svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor" style="flex-shrink:0;opacity:.6">
        <path d="M4 0h5.293A1 1 0 0 1 10 .293L13.707 4a1 1 0 0 1 .293.707V14a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V2a2 2 0 0 1 2-2zm5.5 1.5v2a1 1 0 0 0 1 1h2L9.5 1.5z"/>
      </svg>
      <span>
        {{ store.state.selectedText ? `${t('selectionIn')} ${store.activeTab()?.name}` : store.activeTab()?.name }}
        — {{ t('clickSendContext') }}
      </span>
    </div>

    <!-- Install prompt -->
    <div v-if="needsInstall" class="cli-install-card">
      <div class="cli-install-title">{{ meta.label }} {{ t('notInstalled') }}</div>
      <div class="cli-install-desc">{{ t('requiresPackageDesc') }}</div>
      <code class="cli-install-cmd">{{ displayInstallCmd }}</code>
      <div class="cli-install-actions">
        <button class="cli-install-cancel" @click="cancelInstall">{{ t('cancel') }}</button>
        <button class="cli-install-btn" :style="{ background: meta.color }" @click="installNow">{{ t('installNow') }}</button>
      </div>
    </div>

    <!-- Installing banner -->
    <div v-if="installing" class="cli-installing-bar">
      <span>{{ t('install') }} {{ meta.label }} {{ t('installingBanner') }}</span>
      <button class="cli-continue-btn" :style="{ background: meta.color }" @click="finishInstall">
        ▶ {{ t('startCliBtn') }} {{ meta.label }}
      </button>
    </div>

    <!-- xterm terminal -->
    <div ref="termEl" class="cli-term" />

    <!-- Compose bar: text + file/image attachments piped to the CLI's stdin -->
    <div class="cli-compose">
      <div v-if="attachments.length" class="cli-attachments">
        <span v-for="a in attachments" :key="a.path" class="cli-attach-chip">
          📎 {{ a.name }}
          <button @click="removeAttachment(a.path)">×</button>
        </span>
      </div>
      <div class="cli-compose-row">
        <button class="cli-attach-btn" @click="attachFiles" :title="t('attachTitle')" :disabled="!isRunning">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
            <path d="M4 0h5.293A1 1 0 0 1 10 .293L13.707 4a1 1 0 0 1 .293.707V14a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V2a2 2 0 0 1 2-2zm5.5 1.5v2a1 1 0 0 0 1 1h2L9.5 1.5z"/>
          </svg>
        </button>
        <textarea
          v-model="composeText"
          class="cli-compose-input"
          rows="1"
          :placeholder="t('composePlaceholder')"
          :disabled="!isRunning"
          @keydown.enter.exact.prevent="sendCompose"
        />
        <button
          class="cli-send-btn"
          :style="{ background: meta.color }"
          :disabled="!isRunning || (!composeText.trim() && attachments.length === 0)"
          @click="sendCompose"
          :title="t('sendTitle')"
        >
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
            <path d="M15.964.686a.5.5 0 0 0-.65-.65L.767 5.855H.766l-.452.18a.5.5 0 0 0-.082.887l.41.26.001.002 4.995 3.178 3.178 4.995.002.002.26.41a.5.5 0 0 0 .886-.083l6-15Zm-1.833 1.89L6.637 10.07l-.215-.338a.5.5 0 0 0-.152-.152l-.338-.215 7.494-7.494 1.178-.471-.473 1.178Z"/>
          </svg>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.cli-panel {
  display: flex; flex-direction: column; height: 100%;
  background: #0d1117; font-family: var(--font-ui); overflow: hidden;
}

.cli-header {
  display: flex; align-items: center; justify-content: space-between;
  padding: 7px 10px; border-bottom: 1px solid;
  flex-shrink: 0; background: #161b22;
}
.cli-header-left { display: flex; align-items: center; gap: 7px; }
.cli-icon { font-size: 15px; }
.cli-title { font-size: 13px; font-weight: 700; color: #e6edf3; }
.cli-status {
  font-size: 9.5px; font-weight: 600; border-radius: 8px;
  padding: 1px 7px; border: 1px solid;
}
.cli-status--on  { color: #3fb950; border-color: rgba(63,185,80,.4); background: rgba(63,185,80,.1); }
.cli-status--off { color: #6e7681; border-color: rgba(110,118,129,.4); background: rgba(110,118,129,.1); }
.cli-provider-badge {
  font-size: 9.5px; font-weight: 600; border-radius: 8px; padding: 1px 7px;
  border: 1px solid rgba(139,92,246,.4); background: rgba(139,92,246,.1); color: #a78bfa;
}

.cli-header-right { display: flex; gap: 4px; align-items: center; }
.cli-hdr-btn {
  display: flex; align-items: center; gap: 5px;
  background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.1);
  border-radius: 5px; color: #8b949e; font-size: 11px;
  padding: 4px 8px; cursor: pointer; transition: all 0.12s; white-space: nowrap;
}
.cli-hdr-btn:hover:not(:disabled) { background: rgba(255,255,255,0.12); color: #e6edf3; }
.cli-hdr-btn:disabled { opacity: 0.4; cursor: default; }
.cli-hdr-btn--icon { padding: 4px 6px; }

.cli-tip {
  display: flex; align-items: center; gap: 6px; flex-wrap: wrap;
  padding: 5px 10px; border-bottom: 1px solid;
  font-size: 10.5px; color: #8b949e; flex-shrink: 0;
}
.cli-tip-icon { font-size: 11px; }
.cli-shortcut {
  background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.1);
  border-radius: 4px; padding: 1px 6px; font-size: 10px; color: #6e7681;
  font-family: var(--font-mono);
}

.cli-ctx-bar {
  display: flex; align-items: center; gap: 5px;
  padding: 4px 10px; font-size: 10px; color: #6e7681;
  background: rgba(255,255,255,0.03); border-bottom: 1px solid rgba(255,255,255,0.06);
  flex-shrink: 0;
}

.cli-term {
  flex: 1; min-height: 0; padding: 6px 0 0 0;
  /* xterm.js needs explicit sizing */
  overflow: hidden;
}

/* xterm overrides */
.cli-term :deep(.xterm) { height: 100%; }
.cli-term :deep(.xterm-viewport) { overflow-y: auto !important; }

/* Install prompt */
.cli-install-card {
  margin: 10px; padding: 12px 14px; border-radius: 8px; flex-shrink: 0;
  background: rgba(255,255,255,0.04); border: 1px solid rgba(255,255,255,0.1);
}
.cli-install-title { font-size: 13px; font-weight: 700; color: #e6edf3; margin-bottom: 4px; }
.cli-install-desc { font-size: 11.5px; color: #8b949e; margin-bottom: 8px; }
.cli-install-cmd {
  display: block; background: #010409; border: 1px solid rgba(255,255,255,0.1);
  border-radius: 5px; padding: 6px 9px; font-family: var(--font-mono);
  font-size: 11.5px; color: #56d8e4; margin-bottom: 10px; overflow-x: auto; white-space: nowrap;
}
.cli-install-actions { display: flex; gap: 8px; }
.cli-install-cancel {
  flex: 1; background: none; border: 1px solid rgba(255,255,255,0.15);
  border-radius: 6px; color: #8b949e; font-size: 12px; padding: 6px; cursor: pointer;
}
.cli-install-cancel:hover { color: #e6edf3; border-color: rgba(255,255,255,0.3); }
.cli-install-btn {
  flex: 2; border: none; border-radius: 6px; color: #fff;
  font-size: 12.5px; font-weight: 600; padding: 6px; cursor: pointer; transition: opacity 0.12s;
}
.cli-install-btn:hover { opacity: 0.85; }

/* Installing banner */
.cli-installing-bar {
  display: flex; align-items: center; justify-content: space-between; gap: 10px;
  padding: 6px 10px; background: rgba(255,255,255,0.05); border-bottom: 1px solid rgba(255,255,255,0.1);
  font-size: 11px; color: #8b949e; flex-shrink: 0;
}
.cli-continue-btn {
  border: none; border-radius: 5px; color: #fff; font-size: 11px; font-weight: 600;
  padding: 4px 10px; cursor: pointer; white-space: nowrap; transition: opacity 0.12s;
}
.cli-continue-btn:hover { opacity: 0.85; }

/* Compose bar */
.cli-compose {
  flex-shrink: 0; border-top: 1px solid rgba(255,255,255,0.08);
  background: #161b22; padding: 7px 8px;
}
.cli-attachments { display: flex; flex-wrap: wrap; gap: 5px; margin-bottom: 6px; }
.cli-attach-chip {
  display: flex; align-items: center; gap: 5px;
  background: rgba(255,255,255,0.07); border: 1px solid rgba(255,255,255,0.12);
  border-radius: 12px; padding: 2px 8px; font-size: 10.5px; color: #c9d1d9;
}
.cli-attach-chip button {
  background: none; border: none; color: #8b949e; cursor: pointer; font-size: 12px; padding: 0;
}
.cli-attach-chip button:hover { color: #f85149; }
.cli-compose-row { display: flex; align-items: flex-end; gap: 6px; }
.cli-attach-btn {
  flex-shrink: 0; width: 28px; height: 28px; background: rgba(255,255,255,0.06);
  border: 1px solid rgba(255,255,255,0.1); border-radius: 6px; color: #8b949e;
  cursor: pointer; display: flex; align-items: center; justify-content: center; transition: all 0.12s;
}
.cli-attach-btn:hover:not(:disabled) { background: rgba(255,255,255,0.12); color: #e6edf3; }
.cli-attach-btn:disabled { opacity: 0.35; cursor: default; }
.cli-compose-input {
  flex: 1; min-height: 28px; max-height: 100px; background: rgba(255,255,255,0.05);
  border: 1px solid rgba(255,255,255,0.1); border-radius: 6px; color: #e6edf3;
  font-family: var(--font-ui); font-size: 12.5px; padding: 6px 9px; outline: none;
  resize: none; line-height: 1.4;
}
.cli-compose-input:focus { border-color: rgba(255,255,255,0.3); }
.cli-compose-input:disabled { opacity: 0.4; }
.cli-compose-input::placeholder { color: #6e7681; }
.cli-send-btn {
  flex-shrink: 0; width: 28px; height: 28px; border: none; border-radius: 6px;
  color: #fff; cursor: pointer; display: flex; align-items: center; justify-content: center;
  transition: opacity 0.12s;
}
.cli-send-btn:hover:not(:disabled) { opacity: 0.85; }
.cli-send-btn:disabled { opacity: 0.3; cursor: default; }
</style>
