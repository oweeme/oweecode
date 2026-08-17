<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '../composables/useI18n'

const props = defineProps<{ containerId: string; containerName: string }>()
const { t } = useI18n()

const logText = ref('')
const loading = ref(false)
const error = ref('')
const tail = ref(300)

async function load() {
  loading.value = true
  error.value = ''
  try {
    logText.value = await invoke<string>('container_logs', { id: props.containerId, tail: tail.value })
  } catch (e: any) { error.value = String(e) }
  finally { loading.value = false }
}

watch(() => props.containerId, load)
onMounted(load)
</script>

<template>
  <div class="cl-view">
    <div class="cl-header">
      <span class="cl-name">{{ containerName }}</span>
      <span class="cl-id">{{ containerId.slice(0, 12) }}</span>
      <div style="flex:1" />
      <select v-model.number="tail" class="cl-tail-select" @change="load">
        <option :value="100">100</option>
        <option :value="300">300</option>
        <option :value="1000">1000</option>
        <option :value="5000">5000</option>
      </select>
      <button class="cl-refresh" @click="load" :title="t('reload')">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
          <path d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2v1z"/>
          <path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466z"/>
        </svg>
      </button>
    </div>

    <div v-if="loading" class="cl-loading">{{ t('loading') }}</div>
    <div v-else-if="error" class="cl-error">{{ error }}</div>
    <div v-else-if="!logText.trim()" class="cl-empty">{{ t('noLogsYet') }}</div>
    <pre v-else class="cl-body">{{ logText }}</pre>
  </div>
</template>

<style scoped>
.cl-view { display: flex; flex-direction: column; height: 100%; background: var(--bg-darkest); font-family: var(--font-ui); overflow: hidden; }
.cl-header {
  display: flex; align-items: center; gap: 8px; padding: 8px 12px;
  background: var(--bg-dark); border-bottom: 1px solid var(--border); flex-shrink: 0;
}
.cl-name { font-family: var(--font-mono); font-size: 12.5px; color: var(--fg-bright); font-weight: 600; }
.cl-id { font-family: var(--font-mono); font-size: 10.5px; color: var(--fg-muted); }
.cl-tail-select {
  background: var(--bg-mid); border: 1px solid var(--border); border-radius: 5px;
  color: var(--fg); font-size: 11px; padding: 3px 6px; outline: none; font-family: var(--font-ui);
}
.cl-refresh {
  width: 24px; height: 24px; background: none; border: 1px solid var(--border);
  border-radius: 5px; color: var(--fg-muted); cursor: pointer;
  display: flex; align-items: center; justify-content: center; flex-shrink: 0;
}
.cl-refresh:hover { background: var(--bg-hover); color: var(--fg); }

.cl-loading, .cl-empty { padding: 20px; font-size: 12px; color: var(--fg-muted); }
.cl-error { padding: 20px; font-size: 12px; color: #f85149; white-space: pre-wrap; }

.cl-body {
  flex: 1; overflow: auto; font-family: var(--font-mono); font-size: 12px; line-height: 1.5;
  padding: 10px 14px; margin: 0; white-space: pre-wrap; word-break: break-word; color: var(--fg-dim);
}
</style>
