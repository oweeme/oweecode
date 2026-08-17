<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useEditorStore } from '../composables/useEditorStore'
import { useI18n } from '../composables/useI18n'

const props = defineProps<{ filePath: string; staged: boolean }>()
const store = useEditorStore()
const { t } = useI18n()

interface DiffLine { kind: 'add' | 'del' | 'hunk' | 'ctx' | 'meta'; text: string }

const lines = ref<DiffLine[]>([])
const loading = ref(false)
const error = ref('')

function parseDiff(raw: string): DiffLine[] {
  return raw.split('\n').map(l => {
    if (l.startsWith('@@')) return { kind: 'hunk', text: l }
    if (l.startsWith('+++') || l.startsWith('---') || l.startsWith('diff ') || l.startsWith('index ')) return { kind: 'meta', text: l }
    if (l.startsWith('+')) return { kind: 'add', text: l }
    if (l.startsWith('-')) return { kind: 'del', text: l }
    return { kind: 'ctx', text: l }
  })
}

async function load() {
  loading.value = true
  error.value = ''
  try {
    const raw = await invoke<string>('git_diff', { path: store.state.rootPath, file: props.filePath, staged: props.staged })
    lines.value = parseDiff(raw)
  } catch (e: any) { error.value = String(e) }
  finally { loading.value = false }
}

watch(() => [props.filePath, props.staged], load)
onMounted(load)
</script>

<template>
  <div class="gd-view">
    <div class="gd-header">
      <span class="gd-file">{{ filePath }}</span>
      <span class="gd-badge" :class="staged ? 'staged' : 'unstaged'">{{ staged ? t('staged') : t('unstagedLabel') }}</span>
      <div style="flex:1" />
      <button class="gd-refresh" @click="load" :title="`${t('reload')} diff`">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
          <path d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2v1z"/>
          <path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466z"/>
        </svg>
      </button>
    </div>

    <div v-if="loading" class="gd-loading">{{ t('loading') }}</div>
    <div v-else-if="error" class="gd-error">{{ error }}</div>
    <div v-else-if="lines.length <= 1" class="gd-empty">{{ t('noChangesToShow') }}</div>
    <div v-else class="gd-body">
      <div
        v-for="(l, i) in lines" :key="i"
        class="gd-line" :class="`gd-${l.kind}`"
      >{{ l.text || ' ' }}</div>
    </div>
  </div>
</template>

<style scoped>
.gd-view { display: flex; flex-direction: column; height: 100%; background: var(--bg-darkest); font-family: var(--font-ui); overflow: hidden; }
.gd-header {
  display: flex; align-items: center; gap: 8px; padding: 8px 12px;
  background: var(--bg-dark); border-bottom: 1px solid var(--border); flex-shrink: 0;
}
.gd-file { font-family: var(--font-mono); font-size: 12.5px; color: var(--fg-bright); }
.gd-badge { font-size: 9.5px; font-weight: 700; border-radius: 8px; padding: 1px 8px; text-transform: uppercase; letter-spacing: 0.3px; }
.gd-badge.staged { color: var(--green); background: rgba(166,227,161,0.12); border: 1px solid rgba(166,227,161,0.3); }
.gd-badge.unstaged { color: var(--yellow); background: rgba(249,226,175,0.12); border: 1px solid rgba(249,226,175,0.3); }
.gd-refresh {
  width: 24px; height: 24px; background: none; border: 1px solid var(--border);
  border-radius: 5px; color: var(--fg-muted); cursor: pointer;
  display: flex; align-items: center; justify-content: center;
}
.gd-refresh:hover { background: var(--bg-hover); color: var(--fg); }

.gd-loading, .gd-empty { padding: 20px; font-size: 12px; color: var(--fg-muted); }
.gd-error { padding: 20px; font-size: 12px; color: #f85149; white-space: pre-wrap; }

.gd-body { flex: 1; overflow: auto; font-family: var(--font-mono); font-size: 12px; line-height: 1.5; padding: 6px 0; }
.gd-line { padding: 0 14px; white-space: pre; }
.gd-ctx  { color: var(--fg-dim); }
.gd-meta { color: var(--fg-muted); }
.gd-hunk { color: var(--accent); background: rgba(208,208,216,0.06); font-weight: 600; }
.gd-add  { color: var(--green); background: rgba(166,227,161,0.08); }
.gd-del  { color: var(--red); background: rgba(243,139,168,0.08); }
</style>
