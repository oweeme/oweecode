<script setup lang="ts">
// One container's action row — shared between the standalone container list
// and each pod card, so a container inside a pod gets the exact same
// start/stop/logs/shell/export/edit/remove actions as a standalone one
// instead of the inert name-only pill it used to be.
import { useI18n } from '../composables/useI18n'

interface ContainerInfo {
  id: string; image: string; name: string; status: string
  ports: string; created: string; running: boolean
}

defineProps<{ container: ContainerInfo; busy: boolean }>()
const emit = defineEmits<{
  toggle: []; restart: []; logs: []; shell: []; exportImg: []; edit: []; remove: []
}>()
const { t } = useI18n()
</script>

<template>
  <div class="ctr-row" :class="{ running: container.running }">
    <span class="ctr-dot" :class="{ on: container.running }" />
    <div class="ctr-info" @click="emit('logs')">
      <div class="ctr-name">{{ container.name }}</div>
      <div class="ctr-meta">{{ container.image }}</div>
      <div class="ctr-meta ctr-status">{{ container.status }}<span v-if="container.ports"> · {{ container.ports }}</span></div>
    </div>
    <div class="ctr-actions">
      <button
        class="ctr-action-btn"
        :class="{ on: container.running }"
        @click.stop="emit('toggle')"
        :disabled="busy"
        :title="container.running ? t('stopContainer') : t('startContainer')"
      >
        <span v-if="busy" class="ctr-spin" />
        <svg v-else-if="container.running" width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><rect x="3" y="3" width="10" height="10" rx="1"/></svg>
        <svg v-else width="11" height="11" viewBox="0 0 16 16" fill="currentColor"><path d="M4 2.5v11l9-5.5-9-5.5z"/></svg>
      </button>
      <button class="ctr-action-btn" @click.stop="emit('restart')" :disabled="busy" :title="t('restartContainer')">
        <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
          <path d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2v1z"/>
          <path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466z"/>
        </svg>
      </button>
      <button class="ctr-action-btn" @click.stop="emit('logs')" :title="t('viewLogs')">
        <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
          <path d="M2 2.5A1.5 1.5 0 013.5 1h9A1.5 1.5 0 0114 2.5v11a1.5 1.5 0 01-1.5 1.5h-9A1.5 1.5 0 012 13.5v-11zM4 4v1h8V4H4zm0 3v1h8V7H4zm0 3v1h5v-1H4z"/>
        </svg>
      </button>
      <button
        class="ctr-action-btn"
        @click.stop="emit('shell')"
        :disabled="!container.running"
        :title="container.running ? t('openShell') : t('shellNeedsRunning')"
      >
        <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
          <path d="M0 3a1 1 0 011-1h14a1 1 0 011 1v10a1 1 0 01-1 1H1a1 1 0 01-1-1V3zm1.5 2.5v7a.5.5 0 00.5.5h12a.5.5 0 00.5-.5V6H1.5v-.5zM1 4h14V3H1v1z"/>
          <path d="M3.354 6.646a.5.5 0 10-.708.708L4.293 9l-1.647 1.646a.5.5 0 00.708.708l2-2a.5.5 0 000-.708l-2-2zM7.5 10a.5.5 0 000 1h3a.5.5 0 000-1h-3z"/>
        </svg>
      </button>
      <button class="ctr-action-btn" @click.stop="emit('exportImg')" :disabled="busy" :title="t('exportContainer')">
        <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
          <path d="M8 1a.5.5 0 01.5.5v9.793l2.146-2.147a.5.5 0 01.708.708l-3 3a.5.5 0 01-.708 0l-3-3a.5.5 0 11.708-.708L7.5 11.293V1.5A.5.5 0 018 1z"/>
          <path d="M3 13.5a.5.5 0 01.5-.5h9a.5.5 0 010 1h-9a.5.5 0 01-.5-.5z"/>
        </svg>
      </button>
      <button class="ctr-action-btn" @click.stop="emit('edit')" :title="t('editContainer')">
        <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
          <path d="M12.854.146a.5.5 0 00-.707 0L10.5 1.793 14.207 5.5l1.647-1.646a.5.5 0 000-.708l-3-3zm.646 6.061L9.793 2.5 3.293 9H3.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.207l6.5-6.5zm-7.468 7.468A.5.5 0 015.5 14H5v-.5a.5.5 0 01-.5-.5H4v-.5a.5.5 0 01-.5-.5H3v-.5a.5.5 0 01-.5-.5H2.5v-.5a.5.5 0 01.646-.473l.853.214 1.832-1.832 3.709 3.709-1.832 1.832.214.853z"/>
        </svg>
      </button>
      <button class="ctr-action-btn ctr-action-del" @click.stop="emit('remove')" :disabled="busy" :title="t('removeContainer')">
        <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor">
          <path d="M5.5 5.5A.5.5 0 016 6v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm2.5 0a.5.5 0 01.5.5v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm3 .5a.5.5 0 00-1 0v6a.5.5 0 001 0V6z"/>
          <path fill-rule="evenodd" d="M14.5 3a1 1 0 01-1 1H13v9a2 2 0 01-2 2H5a2 2 0 01-2-2V4h-.5a1 1 0 010-2h4a1 1 0 011-1h2a1 1 0 011 1h4a1 1 0 011 1zM4.118 4L4 4.059V13a1 1 0 001 1h6a1 1 0 001-1V4.059L11.882 4H4.118zM2.5 3a.5.5 0 000 1h11a.5.5 0 000-1h-11z"/>
        </svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.ctr-row {
  display: flex; align-items: center; gap: 8px;
  padding: 7px 10px 7px 9px; border-left: 3px solid transparent;
  transition: background 0.15s, border-color 0.15s; border-radius: 4px; margin: 1px 4px;
}
.ctr-row:hover { background: var(--bg-hover); }
.ctr-row.running { background: rgba(166,227,161,0.06); border-left-color: var(--green); }
.ctr-row.running:hover { background: rgba(166,227,161,0.12); }

.ctr-dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; background: var(--fg-muted); opacity: 0.4; }
.ctr-dot.on { background: var(--green); opacity: 1; box-shadow: 0 0 5px rgba(166,227,161,0.7); }

.ctr-info { flex: 1; min-width: 0; cursor: pointer; }
.ctr-name { font-size: 12.5px; font-weight: 600; color: var(--fg); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.ctr-meta { font-size: 10.5px; color: var(--fg-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 1px; }
.ctr-status { font-family: var(--font-mono); }

.ctr-actions { display: flex; gap: 2px; flex-shrink: 0; }
.ctr-action-btn {
  width: 22px; height: 22px; background: none; border: none;
  border-radius: 4px; color: var(--fg-muted); cursor: pointer;
  display: flex; align-items: center; justify-content: center; transition: all 0.1s;
}
.ctr-action-btn:hover { background: var(--bg-active); color: var(--fg); }
.ctr-action-btn.on { color: var(--green); }
.ctr-action-btn:disabled { opacity: 0.5; cursor: default; }
.ctr-action-del:hover { color: #f85149 !important; }
.ctr-spin {
  width: 10px; height: 10px; border: 2px solid rgba(208,208,216,0.3);
  border-top-color: var(--accent); border-radius: 50%; animation: spin 0.7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }
</style>
