<script setup lang="ts">
// Guided Podman install, mirroring OllamaSetup.vue's exact shape: detect →
// show what's about to run → accept → install via the same embedded-
// terminal (pty_create + pty_write(installCmd)) pattern CliPanel.vue uses →
// recheck. Podman has no single cross-platform installer, so the command
// (and whether one exists at all) is picked from what podman_check_installed
// found on this machine — a package manager on Linux, Homebrew on macOS,
// winget on Windows — falling back to a link to the official docs when none
// of those are available.
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
const { t } = useI18n()

interface PodmanStatus { installed: boolean; version: string | null; linuxPkgManager: string | null; hasBrew: boolean }

const status = ref<PodmanStatus | null>(null)
const checking = ref(true)

const platform = navigator.userAgent.includes('Windows') ? 'windows'
  : navigator.userAgent.includes('Mac') ? 'mac'
  : 'linux'

const PKG_INSTALL_CMD: Record<string, string> = {
  'apt-get': 'sudo apt-get update && sudo apt-get install -y podman',
  'dnf': 'sudo dnf install -y podman',
  'pacman': 'sudo pacman -S --noconfirm podman',
  'zypper': 'sudo zypper install -y podman',
}

const installCmd = ref('')
function computeInstallCmd() {
  if (!status.value) { installCmd.value = ''; return }
  if (platform === 'linux' && status.value.linuxPkgManager) {
    installCmd.value = PKG_INSTALL_CMD[status.value.linuxPkgManager] ?? ''
  } else if (platform === 'mac' && status.value.hasBrew) {
    installCmd.value = 'brew install podman'
  } else if (platform === 'windows') {
    installCmd.value = 'winget install -e --id RedHat.Podman'
  } else {
    installCmd.value = ''
  }
}

async function refreshStatus() {
  checking.value = true
  try {
    status.value = await invoke<PodmanStatus>('podman_check_installed')
    computeInstallCmd()
  } finally {
    checking.value = false
  }
}

// ── Embedded terminal — same pattern as OllamaSetup.vue's runInTerminal.
const termEl = ref<HTMLElement | null>(null)
const installing = ref(false)
let xterm: XTerm | null = null
let fitAddon: FitAddon | null = null
let unlistenData: UnlistenFn | null = null
let unlistenExit: UnlistenFn | null = null
const SESSION_ID = 'podman-install-session'

function nextFrame() { return new Promise(r => setTimeout(r, 0)) }

async function startInstall() {
  installing.value = true
  unlistenData?.(); unlistenExit?.(); xterm?.dispose()
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

  unlistenData = await listen<string>(`pty:${SESSION_ID}`, (ev) => xterm?.write(ev.payload))
  unlistenExit = await listen<string>(`pty-exit:${SESSION_ID}`, async () => {
    xterm?.writeln('\r\n\x1b[32m[Listo]\x1b[0m')
    await new Promise(r => setTimeout(r, 600))
    await refreshStatus()
  })

  try {
    const cwd = store.state.rootPath || (await invoke<string>('get_home_dir').catch(() => '/'))
    await invoke('pty_create', { id: SESSION_ID, cwd, cols: 100, rows: 24 })
    xterm.writeln(`\x1b[36m$ ${installCmd.value}\x1b[0m`)
    await invoke('pty_write', { id: SESSION_ID, data: installCmd.value + '\r' })
  } catch (e: any) {
    xterm.writeln(`\r\n\x1b[31mError: ${e}\x1b[0m`)
  }
}

onMounted(refreshStatus)

onBeforeUnmount(async () => {
  unlistenData?.()
  unlistenExit?.()
  if (installing.value) await invoke('pty_kill', { id: SESSION_ID }).catch(() => {})
  xterm?.dispose()
})
</script>

<template>
  <Teleport to="body">
    <div class="pm-overlay" @click.self="emit('close')">
      <div class="pm-modal">
        <div class="pm-modal-head">
          <span class="pm-modal-title">⚙ {{ t('installPodmanTitle') }}</span>
          <button class="pm-modal-close" @click="emit('close')">✕</button>
        </div>

        <div v-if="checking" class="pm-step">
          <div class="mini-spin" /> {{ t('checkingPodman') }}
        </div>

        <!-- Terms + install -->
        <template v-else-if="!status?.installed && !installing">
          <div class="pm-step">
            <p>{{ t('podmanIntro') }}</p>
            <template v-if="installCmd">
              <div class="pm-cmd-preview">{{ installCmd }}</div>
              <button class="pm-btn pm-btn--primary" @click="startInstall">{{ t('acceptAndInstall') }}</button>
            </template>
            <template v-else>
              <p class="pm-hint">{{ t('podmanNoAutoInstall') }}</p>
              <a class="pm-btn pm-btn--primary" href="https://podman.io/docs/installation" target="_blank">{{ t('podmanDownloadLink') }} →</a>
              <button class="pm-btn" @click="refreshStatus">{{ t('alreadyInstalledRetry') }}</button>
            </template>
          </div>
        </template>

        <!-- Installing (embedded terminal) -->
        <template v-else-if="installing && !status?.installed">
          <div class="pm-step">
            <p>{{ t('installingPodman') }}</p>
            <div ref="termEl" class="pm-term"></div>
          </div>
        </template>

        <!-- Installed -->
        <template v-else-if="status?.installed">
          <div class="pm-step">
            <p class="pm-ready">✓ Podman {{ status.version }} {{ t('installedLabel') }}</p>
            <p v-if="platform !== 'linux'" class="pm-hint">{{ t('podmanMachineHint') }}</p>
            <div v-if="platform !== 'linux'" class="pm-cmd-preview">podman machine init && podman machine start</div>
            <button class="pm-btn pm-btn--primary" @click="emit('close')">{{ t('close') }}</button>
          </div>
        </template>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.pm-overlay {
  position: fixed; inset: 0; background: rgba(0,0,0,0.55);
  display: flex; align-items: center; justify-content: center; z-index: 200;
}
.pm-modal {
  width: min(480px, 92vw); max-height: 80vh; overflow-y: auto;
  background: var(--bg-dark); border: 1px solid var(--border); border-radius: 10px;
  padding: 14px 16px 16px;
}
.pm-modal-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 10px; }
.pm-modal-title { font-size: 13px; font-weight: 700; color: var(--fg-bright); }
.pm-modal-close { background: none; border: none; color: var(--fg-muted); cursor: pointer; font-size: 13px; }
.pm-step { font-size: 12px; color: var(--fg); display: flex; flex-direction: column; gap: 8px; }
.pm-cmd-preview {
  font-family: var(--font-mono); font-size: 10.5px; color: #a6e3a1;
  background: var(--bg-darker); border-radius: 6px; padding: 6px 8px; word-break: break-all;
}
.pm-hint { font-size: 11px; color: var(--fg-muted); }
.pm-term { height: 220px; background: #0d1117; border-radius: 6px; padding: 4px; }
.pm-btn {
  align-self: flex-start; background: var(--bg-hover); color: var(--fg);
  border: 1px solid var(--border); border-radius: 6px; padding: 6px 14px;
  font-size: 11.5px; font-weight: 600; cursor: pointer;
}
.pm-btn--primary { background: #a6e3a1; color: #0d1117; border: none; }
.pm-btn--primary:hover { opacity: 0.9; }
.pm-ready { font-size: 12.5px; color: #3fb950; font-weight: 600; }
.mini-spin {
  width: 12px; height: 12px; border: 2px solid var(--border); border-top-color: #a6e3a1;
  border-radius: 50%; animation: spin 0.7s linear infinite; display: inline-block; vertical-align: middle;
}
@keyframes spin { to { transform: rotate(360deg); } }
</style>
