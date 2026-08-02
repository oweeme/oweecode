<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue'
import { Terminal as XTerm } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import '@xterm/xterm/css/xterm.css'

const props = defineProps<{
  cwd: string
  command?: string   // optional: run this CLI instead of bash (e.g. 'claude', 'gemini')
}>()
const emit = defineEmits<{ 'open-file': [path: string] }>()

const termEl = ref<HTMLElement | null>(null)
let xterm: XTerm | null = null
let fitAddon: FitAddon | null = null

// Unique session ID per terminal instance
const sessionId = `term-${Date.now()}-${Math.random().toString(36).slice(2)}`
let unlistenData: UnlistenFn | null = null
let unlistenExit: UnlistenFn | null = null

// OSC sequence parser for e() → open-in-editor
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
  if (visible) xterm!.write(visible)
}

onMounted(async () => {
  xterm = new XTerm({
    theme: {
      background:          '#0d1117',
      foreground:          '#e6edf3',
      cursor:              '#d0d0d8',
      cursorAccent:        '#0d1117',
      selectionBackground: 'rgba(208,208,216,0.28)',
      black:   '#21262d', red:           '#f85149',
      green:   '#3fb950', yellow:        '#d29922',
      blue:    '#79c0ff', magenta:       '#d2a8ff',
      cyan:    '#56d8e4', white:         '#c9d1d9',
      brightBlack:   '#484f58', brightRed:     '#ff7b72',
      brightGreen:   '#56d364', brightYellow:  '#e3b341',
      brightBlue:    '#79c0ff', brightMagenta: '#d2a8ff',
      brightCyan:    '#56d8e4', brightWhite:   '#f0f6fc',
    },
    fontFamily: '"JetBrains Mono", "Fira Code", "Cascadia Code", Consolas, monospace',
    fontSize: 13,
    lineHeight: 1.5,
    letterSpacing: 0.3,
    cursorBlink: true,
    cursorStyle: 'bar',
    scrollback: 5000,
  })

  fitAddon = new FitAddon()
  xterm.loadAddon(fitAddon)
  xterm.loadAddon(new WebLinksAddon())

  if (termEl.value) {
    xterm.open(termEl.value)
    setTimeout(() => fitAddon?.fit(), 60)
  }

  // All data from xterm (keys, pastes, escape sequences) goes directly to PTY
  xterm.onData((data) => {
    invoke('pty_write', { id: sessionId, data }).catch(() => {})
  })

  // Listen for PTY output
  unlistenData = await listen<string>(`pty:${sessionId}`, (ev) => {
    processOutput(ev.payload)
  })

  unlistenExit = await listen<string>(`pty-exit:${sessionId}`, () => {
    xterm?.writeln('\r\n\x1b[33m[shell terminada — presiona Enter para reiniciar]\x1b[0m')
    xterm?.onData(() => startPty())
  })

  await startPty()

  // Resize: fit xterm and send new dimensions to PTY
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
})

onBeforeUnmount(async () => {
  unlistenData?.()
  unlistenExit?.()
  await invoke('pty_kill', { id: sessionId }).catch(() => {})
  xterm?.dispose()
})

async function startPty() {
  // Only fit if element is actually visible — v-show=false gives zero dimensions
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
  const startCwd = props.cwd || '/'

  try {
    await invoke('pty_create', {
      id: sessionId,
      cwd: startCwd,
      cols,
      rows,
      command: props.command ?? null,
    })

    // Only inject aliases for plain bash sessions
    if (!props.command) {
      setTimeout(() => {
        const init = [
          'export LS_COLORS=\'di=1;34:ln=1;36:ex=1;32:or=1;31:*.vue=1;32:*.ts=1;34:*.tsx=1;34:*.js=1;33:*.json=0;33:*.rs=1;31:*.go=1;36:*.py=1;35:*.sh=1;32:*.css=1;34:*.html=1;31:*.md=0;96:*.sql=1;33:*.yaml=0;33:*.toml=0;33:*.env=0;33:*.log=0;90:*.lock=0;90\'',
          'alias ls=\'ls --color=always --group-directories-first\'',
          'alias ll=\'ls -lah --color=always --group-directories-first\'',
          'alias la=\'ls -lAh --color=always\'',
          'alias lt=\'ls -lahtr --color=always\'',
          'alias grep=\'grep --color=auto\'',
          'e() { local f="$1"; [ -z "$f" ] && echo "Uso: e <archivo>" && return 1; [[ "$f" != /* ]] && f="$(pwd)/$f"; printf "\\033]6634;%s\\007" "$f"; printf "\\033[36m\\xe2\\x86\\x97 Abriendo \\033[1m%s\\033[0m\\033[36m en el editor\\033[0m\\n" "$(basename "$f")"; }',
          'alias nano=e',
          'alias vi=e',
        ].join('\n')
        invoke('pty_write', { id: sessionId, data: init + '\n' }).catch(() => {})
      }, 600)
    }

  } catch (e: any) {
    xterm?.writeln(`\x1b[31mError iniciando PTY: ${e}\x1b[0m`)
    xterm?.writeln('\x1b[2mAsegúrate de que el comando existe en el sistema.\x1b[0m')
  }
}

defineExpose({
  runCommand(cmd: string) {
    invoke('pty_write', { id: sessionId, data: cmd + '\n' }).catch(() => {})
  },
  focus: () => xterm?.focus(),
  fit:   () => fitAddon?.fit(),
})
</script>

<template>
  <div class="terminal-wrap">
    <div ref="termEl" class="term-el" />
  </div>
</template>

<style scoped>
.terminal-wrap {
  width: 100%; height: 100%;
  background: #0d1117;
  display: flex; flex-direction: column;
  overflow: hidden;
  position: absolute; inset: 0;
}
.term-el {
  flex: 1;
  padding: 4px 10px;
  min-height: 0; overflow: hidden;
}
.term-el :deep(.xterm) { height: 100%; }
.term-el :deep(.xterm-viewport)::-webkit-scrollbar { width: 5px; }
.term-el :deep(.xterm-viewport)::-webkit-scrollbar-track { background: transparent; }
.term-el :deep(.xterm-viewport)::-webkit-scrollbar-thumb { background: #30363d; border-radius: 3px; }
.term-el :deep(.xterm-viewport)::-webkit-scrollbar-thumb:hover { background: #484f58; }
</style>
