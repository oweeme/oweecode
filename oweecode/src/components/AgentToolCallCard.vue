<script setup lang="ts">
// Inline approval card shown in the agent's message list whenever a
// "dangerous" tool (writes a file, runs a shell command, stages/commits) is
// about to run — the agent loop in useAgentStore.ts pauses until Approve or
// Reject is clicked, it never runs a mutating tool on its own.
import { computed } from 'vue'
import type { PendingToolCall } from '../composables/useAgentStore'

const props = defineProps<{ call: PendingToolCall }>()
const emit = defineEmits<{ (e: 'approve'): void; (e: 'reject'): void }>()

const summary = computed(() => {
  const a = props.call.arguments ?? {}
  switch (props.call.name) {
    case 'fs_write_file': return a.path ?? ''
    case 'git_stage': return Array.isArray(a.files) ? a.files.join(', ') : ''
    case 'git_commit': return a.message ?? ''
    case 'shell_run_command': return a.command ?? ''
    case 'container_create': return `${a.name ?? ''} — ${a.image ?? ''}`
    case 'container_start': case 'container_stop': case 'container_remove': return a.id ?? ''
    case 'container_image_pull': return a.image ?? ''
    case 'container_network_ensure': return a.name ?? ''
    case 'container_export': return `${a.id ?? ''} → ${a.outPath ?? ''}`
    default: return ''
  }
})

const detail = computed(() => {
  if (props.call.name === 'fs_write_file' && typeof props.call.arguments?.content === 'string') {
    const content = props.call.arguments.content as string
    return content.length > 600 ? content.slice(0, 600) + '\n… (truncado)' : content
  }
  if (props.call.name === 'container_create') {
    return JSON.stringify(props.call.arguments, null, 2)
  }
  return ''
})
</script>

<template>
  <div class="tool-call-card">
    <div class="tool-call-head">
      <span class="tool-call-icon">⚠</span>
      <span class="tool-call-label">{{ call.label }}</span>
    </div>
    <div v-if="summary" class="tool-call-summary">{{ summary }}</div>
    <pre v-if="detail" class="tool-call-detail">{{ detail }}</pre>
    <div class="tool-call-actions">
      <button class="tool-call-btn tool-call-btn--reject" @click="emit('reject')">Rechazar</button>
      <button class="tool-call-btn tool-call-btn--approve" @click="emit('approve')">Aprobar</button>
    </div>
  </div>
</template>

<style scoped>
.tool-call-card {
  margin: 6px 12px; padding: 10px 12px; border-radius: 8px;
  background: rgba(245, 158, 11, 0.08); border: 1px solid rgba(245, 158, 11, 0.35);
}
.tool-call-head { display: flex; align-items: center; gap: 6px; margin-bottom: 4px; }
.tool-call-icon { color: #f5a623; font-size: 12px; }
.tool-call-label { font-size: 12px; font-weight: 700; color: var(--fg-bright); }
.tool-call-summary { font-family: var(--font-mono); font-size: 11px; color: var(--fg-muted); margin-bottom: 6px; word-break: break-word; }
.tool-call-detail {
  font-family: var(--font-mono); font-size: 10.5px; color: var(--fg-muted);
  background: var(--bg-darker); border-radius: 6px; padding: 6px 8px;
  max-height: 180px; overflow-y: auto; white-space: pre-wrap; word-break: break-word;
  margin: 0 0 8px;
}
.tool-call-actions { display: flex; gap: 6px; justify-content: flex-end; }
.tool-call-btn { border: none; border-radius: 6px; padding: 5px 12px; font-size: 11px; font-weight: 600; cursor: pointer; }
.tool-call-btn--reject { background: var(--bg-hover); color: var(--fg-muted); }
.tool-call-btn--reject:hover { color: var(--fg); }
.tool-call-btn--approve { background: #f5a623; color: #1a1200; }
.tool-call-btn--approve:hover { opacity: 0.9; }
</style>
