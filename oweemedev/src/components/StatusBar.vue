<script setup lang="ts">
import { computed } from 'vue'
import { useEditorStore } from '../composables/useEditorStore'
import { useI18n } from '../composables/useI18n'

const store = useEditorStore()
const { t } = useI18n()

const activeTab = computed(() => store.activeTab())
const langLabel = computed(() => {
  if (!activeTab.value) return ''
  const lang = activeTab.value.language
  const labels: Record<string, string> = {
    typescript: 'TypeScript', javascript: 'JavaScript',
    vue: 'Vue', php: 'PHP', go: 'Go', html: 'HTML',
    css: 'CSS', json: 'JSON', markdown: 'Markdown',
    rust: 'Rust', python: 'Python', sql: 'SQL', cpp: 'C++',
    shell: 'Shell', yaml: 'YAML', toml: 'TOML', xml: 'XML',
    text: 'Plain Text', image: 'Image',
  }
  return labels[lang] ?? lang
})

const tabType = computed(() => activeTab.value?.type ?? '')
const hasSelection = computed(() => store.state.selectedText.trim().length > 0)
const selLines = computed(() => store.state.selectedText.split('\n').length)
const selChars = computed(() => store.state.selectedText.length)

// AI provider from localStorage (reactive read)
const aiProvider = computed(() => localStorage.getItem('ai_provider') ?? 'claude')
const aiModel = computed(() => {
  const m = localStorage.getItem('ai_model') ?? ''
  return m.split('-').slice(0,3).join('-') // shorten long model names
})
const aiProviderIcon: Record<string, string> = {
  claude: '◆', deepseek: '◈', gemini: '◈', openai: '⬡', ollama: '⬡'
}

const lspStatusIcon = computed(() => {
  switch (store.state.lspStatus) {
    case 'starting': return '◌'
    case 'ready':     return '●'
    case 'error':     return '✕'
    default:          return ''
  }
})
const lspStatusTitle = computed(() => {
  switch (store.state.lspStatus) {
    case 'starting': return t('lspStatusStarting')
    case 'ready':     return t('lspStatusReady')
    case 'error':     return t('lspStatusError')
    default:          return ''
  }
})
</script>

<template>
  <div class="statusbar">
    <!-- Left: brand + git/project info -->
    <div class="statusbar-left">
      <span class="statusbar-item statusbar-item--brand">
        <img src="/oweedev.png" width="11" height="11" style="border-radius:2px" />
        OweemeIDE
      </span>

      <!-- Tab type badge -->
      <span v-if="tabType === 'database'" class="statusbar-item statusbar-item--db">
        🗄 DB
      </span>
      <span v-else-if="tabType === 'ftp' || tabType === 'ftp-file'" class="statusbar-item statusbar-item--ftp">
        📡 FTP/SFTP
      </span>
      <span v-else-if="tabType === 'api'" class="statusbar-item statusbar-item--api">
        ⚡ API Client
      </span>

      <!-- Git placeholder (future: branch info) -->
      <span v-if="store.state.rootPath" class="statusbar-item statusbar-item--path" :title="store.state.rootPath">
        <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
          <path fill-rule="evenodd" d="M11.75 2.5a.75.75 0 100 1.5.75.75 0 000-1.5zm-2.25.75a2.25 2.25 0 113 2.122V6A2.5 2.5 0 0110 8.5H6a1 1 0 00-1 1v1.128a2.251 2.251 0 11-1.5 0V5.372a2.25 2.25 0 111.5 0v1.836A2.493 2.493 0 016 7h4a1 1 0 001-1v-.628A2.25 2.25 0 019.5 3.25zM4.25 12a.75.75 0 100 1.5.75.75 0 000-1.5zM3.5 3.25a.75.75 0 111.5 0 .75.75 0 01-1.5 0z"/>
        </svg>
        {{ store.state.rootPath.split('/').pop() }}
      </span>
    </div>

    <!-- Right: cursor, encoding, lang, AI, modified -->
    <div class="statusbar-right">
      <!-- Selection info -->
      <span v-if="hasSelection" class="statusbar-item statusbar-item--sel">
        {{ selLines > 1 ? `${selLines} ${t('linesWord2')}` : `${selChars} ${t('charsWord')}` }} {{ t('selectedSuffix') }}
      </span>

      <!-- Cursor position -->
      <span v-if="activeTab && activeTab.type === 'code'" class="statusbar-item">
        Ln {{ store.state.cursorLine }}, Col {{ store.state.cursorCol }}
      </span>

      <span v-if="activeTab && activeTab.type === 'code'" class="statusbar-item">UTF-8</span>

      <!-- Language -->
      <span v-if="langLabel && activeTab?.type === 'code'" class="statusbar-item statusbar-item--lang">
        {{ langLabel }}
      </span>

      <!-- LSP status: makes it explicit whether IntelliSense is actually running for
           this file, instead of the user having to guess from whether completions show up. -->
      <span
        v-if="activeTab?.type === 'code' && store.state.lspStatus !== 'idle'"
        class="statusbar-item statusbar-item--lsp"
        :class="`statusbar-item--lsp-${store.state.lspStatus}`"
        :title="lspStatusTitle"
      >
        {{ lspStatusIcon }} LSP
      </span>

      <!-- Modified indicator -->
      <span v-if="activeTab?.modified" class="statusbar-item statusbar-item--modified">
        ● {{ t('modifiedLabel') }}
      </span>

      <!-- AI indicator -->
      <span class="statusbar-item statusbar-item--ai" :title="t('aiActiveTitle')">
        {{ aiProviderIcon[aiProvider] ?? '◆' }} {{ aiModel || aiProvider }}
      </span>
    </div>
  </div>
</template>

<style scoped>
.statusbar {
  height: var(--statusbar-h);
  background: var(--bg-darkest);
  border-top: 1px solid var(--border);
  display: flex; align-items: center; justify-content: space-between;
  padding: 0 6px; flex-shrink: 0;
  font-family: var(--font-ui); font-size: 11px;
  color: var(--fg-dim); user-select: none;
}
.statusbar-left, .statusbar-right { display: flex; align-items: center; gap: 1px; }

.statusbar-item {
  padding: 0 6px; height: 22px;
  display: flex; align-items: center; gap: 4px;
  border-radius: 3px; cursor: default; white-space: nowrap;
  transition: background 0.1s;
}
.statusbar-item:hover { background: var(--bg-hover); }

.statusbar-item--brand { font-weight: 700; gap: 5px; font-size: 11.5px; color: var(--fg); }
.statusbar-item--lang { font-weight: 500; }
.statusbar-item--modified { color: var(--yellow); }
.statusbar-item--sel { color: var(--fg-muted); font-size: 10.5px; }
.statusbar-item--path { color: var(--fg-muted); font-size: 10.5px; max-width: 160px; overflow: hidden; text-overflow: ellipsis; }
.statusbar-item--ai { color: var(--fg-dim); font-size: 10.5px; font-weight: 600; }
.statusbar-item--db { color: var(--yellow); }
.statusbar-item--ftp { color: var(--green); }
.statusbar-item--api { color: var(--accent); }
.statusbar-item--lsp { font-weight: 600; font-size: 10.5px; }
.statusbar-item--lsp-starting { color: var(--yellow); }
.statusbar-item--lsp-ready { color: var(--green); }
.statusbar-item--lsp-error { color: var(--red); }
</style>
