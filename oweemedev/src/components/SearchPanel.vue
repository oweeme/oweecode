<script setup lang="ts">
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useEditorStore } from '../composables/useEditorStore'
import { useI18n } from '../composables/useI18n'

const { t } = useI18n()
const store = useEditorStore()
const query = ref('')
const results = ref<{ file: string; line: number; text: string }[]>([])
const isSearching = ref(false)
const searched = ref(false)
const caseSensitive = ref(false)
const useRegex = ref(false)
const MAX = 200

async function search() {
  const q = query.value.trim()
  if (!q || !store.state.rootPath) return
  isSearching.value = true
  searched.value = false
  results.value = []
  try {
    results.value = await invoke<{ file: string; line: number; text: string }[]>('search_in_files', {
      rootPath: store.state.rootPath,
      query: q,
      caseSensitive: caseSensitive.value,
      useRegex: useRegex.value,
      maxResults: MAX,
    })
  } catch (e) {
    console.error(e)
  }
  isSearching.value = false
  searched.value = true
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'Enter') search()
}

function openResult(r: { file: string; line: number }) {
  store.openFile(r.file, r.line)
}

// Group by file
const grouped = computed(() => {
  const map = new Map<string, typeof results.value>()
  for (const r of results.value) {
    if (!map.has(r.file)) map.set(r.file, [])
    map.get(r.file)!.push(r)
  }
  return Array.from(map.entries()).map(([file, hits]) => ({ file, hits }))
})

function shortPath(full: string) {
  const root = store.state.rootPath
  return full.startsWith(root) ? full.slice(root.length + 1) : full
}

function highlight(text: string): string {
  if (!query.value) return escHtml(text)
  try {
    const flags = caseSensitive.value ? 'g' : 'gi'
    const pat = useRegex.value ? query.value : query.value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
    return escHtml(text).replace(new RegExp(pat, flags), m => `<mark>${m}</mark>`)
  } catch { return escHtml(text) }
}
function escHtml(s: string) {
  return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;')
}
</script>

<template>
  <div class="search-panel">
    <div class="search-header">
      <span class="search-title">SEARCH</span>
      <span class="search-scope">{{ store.state.rootPath.split('/').pop() || 'project' }}</span>
    </div>

    <div class="search-box">
      <input
        v-model="query"
        class="search-input"
        :placeholder="t('searchInFiles')"
        @keydown="onKey"
        autofocus
      />
      <button class="search-run" @click="search" :disabled="isSearching" :title="t('searchTitle')">
        <svg v-if="!isSearching" width="13" height="13" viewBox="0 0 16 16" fill="currentColor">
          <path d="M11.742 10.344a6.5 6.5 0 10-1.397 1.398h-.001c.03.04.062.078.098.115l3.85 3.85a1 1 0 001.415-1.414l-3.85-3.85a1.007 1.007 0 00-.115-.099zm-5.242 1.156a5.5 5.5 0 110-11 5.5 5.5 0 010 11z"/>
        </svg>
        <span v-else class="mini-spinner" />
      </button>
    </div>

    <!-- Options -->
    <div class="search-opts">
      <label class="opt-toggle" :class="{ active: caseSensitive }" @click="caseSensitive = !caseSensitive" :title="t('caseSensitive')">Aa</label>
      <label class="opt-toggle" :class="{ active: useRegex }" @click="useRegex = !useRegex" :title="t('useRegex')">.*</label>
    </div>

    <!-- No root -->
    <div v-if="!store.state.rootPath" class="search-hint">{{ t('openFolderFirst') }}</div>

    <!-- Loading -->
    <div v-else-if="isSearching" class="search-loading">
      <span class="mini-spinner" /> {{ t('searching') }}
    </div>

    <!-- Results -->
    <div v-else-if="searched" class="search-results">
      <div v-if="results.length === 0" class="search-empty">{{ t('noResultsFor') }} "{{ query }}"</div>
      <div v-else class="results-summary">
        {{ results.length }}{{ results.length >= MAX ? '+' : '' }} {{ t('resultsIn') }} {{ grouped.length }} {{ t('filesLabel2') }}
      </div>

      <div v-for="group in grouped" :key="group.file" class="result-group">
        <div class="result-file" :title="group.file">
          <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor" style="flex-shrink:0;opacity:.7">
            <path d="M2 1.75C2 .784 2.784 0 3.75 0h6.586c.464 0 .909.184 1.237.513l2.914 2.914c.329.328.513.773.513 1.237v9.586A1.75 1.75 0 0113.25 16h-9.5A1.75 1.75 0 012 14.25V1.75z"/>
          </svg>
          {{ shortPath(group.file) }}
        </div>
        <div
          v-for="hit in group.hits"
          :key="hit.line"
          class="result-hit"
          @click="openResult(hit)"
        >
          <span class="hit-line">{{ hit.line }}</span>
          <span class="hit-text" v-html="highlight(hit.text.trim())" />
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.search-panel {
  display: flex; flex-direction: column;
  height: 100%; overflow: hidden;
  font-family: var(--font-ui); color: var(--fg);
}
.search-header {
  padding: 8px 14px 4px;
  display: flex; align-items: center; gap: 8px;
  flex-shrink: 0;
}
.search-title { font-size: 10px; font-weight: 700; letter-spacing: 1.5px; color: var(--fg-muted); }
.search-scope { font-size: 10px; color: var(--fg-muted); background: var(--bg-hover); border-radius: 4px; padding: 1px 5px; }

.search-box {
  display: flex; gap: 4px;
  padding: 4px 8px; flex-shrink: 0;
}
.search-input {
  flex: 1; background: var(--bg-mid);
  border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg); font-size: 12px; padding: 5px 8px;
  outline: none; font-family: var(--font-ui);
  min-width: 0;
}
.search-input:focus { border-color: var(--accent); }
.search-run {
  width: 30px; height: 30px;
  background: var(--accent); border: none; border-radius: 6px;
  color: var(--accent-fg); cursor: pointer; flex-shrink: 0;
  display: flex; align-items: center; justify-content: center;
  transition: opacity 0.12s;
}
.search-run:disabled { opacity: 0.5; }
.search-run:hover:not(:disabled) { opacity: 0.85; }

.search-opts {
  display: flex; gap: 4px; padding: 0 8px 6px;
  flex-shrink: 0;
}
.opt-toggle {
  font-size: 11px; font-weight: 700;
  background: var(--bg-mid); border: 1px solid var(--border);
  border-radius: 4px; padding: 2px 7px; cursor: pointer;
  color: var(--fg-muted); transition: all 0.12s; user-select: none;
  font-family: var(--font-mono);
}
.opt-toggle.active { color: var(--accent); border-color: var(--accent); background: rgba(82,139,255,0.1); }

.search-hint, .search-empty, .search-loading {
  font-size: 12px; color: var(--fg-muted);
  padding: 16px 14px; text-align: center;
  display: flex; align-items: center; justify-content: center; gap: 6px;
}
.results-summary {
  font-size: 10.5px; color: var(--fg-muted);
  padding: 4px 12px 6px;
  border-bottom: 1px solid var(--border);
}

.search-results {
  flex: 1; overflow-y: auto;
}
.search-results::-webkit-scrollbar { width: 4px; }
.search-results::-webkit-scrollbar-thumb { background: var(--bg-active); border-radius: 2px; }

.result-group { margin-bottom: 2px; }
.result-file {
  display: flex; align-items: center; gap: 5px;
  padding: 5px 10px;
  font-size: 11.5px; font-weight: 600; color: var(--fg-dim);
  background: var(--bg-panel);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  position: sticky; top: 0; z-index: 1;
}
.result-hit {
  display: flex; align-items: baseline; gap: 8px;
  padding: 3px 10px 3px 22px;
  cursor: pointer; font-family: var(--font-mono);
  font-size: 11.5px; transition: background 0.08s;
}
.result-hit:hover { background: var(--bg-hover); }
.hit-line {
  color: var(--fg-muted); font-size: 10.5px;
  min-width: 24px; text-align: right; flex-shrink: 0;
}
.hit-text {
  color: var(--fg); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1;
}
.hit-text :deep(mark) {
  background: rgba(255,198,0,0.3); color: var(--yellow);
  border-radius: 2px; padding: 0 1px;
}

.mini-spinner {
  display: inline-block; width: 11px; height: 11px;
  border: 2px solid rgba(255,255,255,0.3);
  border-top-color: #fff; border-radius: 50%;
  animation: spin 0.7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }
</style>
