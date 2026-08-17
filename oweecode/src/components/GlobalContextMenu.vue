<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useI18n } from '../composables/useI18n'

const { t } = useI18n()
const menu = ref<{ x: number; y: number } | null>(null)

function show(e: MouseEvent) {
  const target = e.target as Element
  // Skip if FileExplorer handles it (has data-explorer attr on ancestor)
  if (target.closest('[data-explorer]')) return
  // Skip if it's a terminal area
  if (target.closest('.xterm')) return
  e.preventDefault()
  // Position within viewport
  const x = Math.min(e.clientX, window.innerWidth - 130)
  const y = Math.min(e.clientY, window.innerHeight - 110)
  menu.value = { x, y }
}

function close() { menu.value = null }

function cut() {
  document.execCommand('cut')
  close()
}
function copy() {
  document.execCommand('copy')
  close()
}
async function paste() {
  try {
    const text = await navigator.clipboard.readText()
    document.execCommand('insertText', false, text)
  } catch {
    document.execCommand('paste')
  }
  close()
}
function selectAll() {
  document.execCommand('selectAll')
  close()
}

onMounted(() => document.addEventListener('contextmenu', show))
onUnmounted(() => document.removeEventListener('contextmenu', show))
</script>

<template>
  <Teleport to="body">
    <div v-if="menu" class="gctx-overlay" @click="close" @contextmenu.prevent>
      <div class="gctx-menu" :style="{ left: menu.x + 'px', top: menu.y + 'px' }">
        <button class="gctx-item" @click="cut">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M3.5 3.5a1.5 1.5 0 100 3 1.5 1.5 0 000-3zM2 5a2 2 0 114 0 2 2 0 01-4 0zm7.5 4a1.5 1.5 0 100 3 1.5 1.5 0 000-3zM10 11a2 2 0 11-2.236-1.981l-2.063-2.295-.673.808A2 2 0 015 9a2 2 0 11-1.006-1.726L6.97 4.383A2 2 0 015 2.5a2 2 0 114 0 2 2 0 01-1.97 1.996l-2.063 2.295A2.001 2.001 0 0110 9a2 2 0 010 2z"/></svg>
          {{ t('cut') }}
          <span class="gctx-key">Ctrl+X</span>
        </button>
        <button class="gctx-item" @click="copy">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M4 1.5H3a2 2 0 00-2 2V14a2 2 0 002 2h10a2 2 0 002-2V3.5a2 2 0 00-2-2h-1v1h1a1 1 0 011 1V14a1 1 0 01-1 1H3a1 1 0 01-1-1V3.5a1 1 0 011-1h1v-1z"/><path d="M9.5 1a.5.5 0 01.5.5v1a.5.5 0 01-.5.5h-3a.5.5 0 01-.5-.5v-1a.5.5 0 01.5-.5h3zm-3-1A1.5 1.5 0 005 1.5v1A1.5 1.5 0 006.5 4h3A1.5 1.5 0 0011 2.5v-1A1.5 1.5 0 009.5 0h-3z"/></svg>
          {{ t('copy') }}
          <span class="gctx-key">Ctrl+C</span>
        </button>
        <button class="gctx-item" @click="paste">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M5.5 0A1.5 1.5 0 004 1.5v1A1.5 1.5 0 005.5 4h5A1.5 1.5 0 0012 2.5v-1A1.5 1.5 0 0010.5 0h-5z"/><path d="M3 2.5a.5.5 0 01.5-.5H4a.5.5 0 010 1h-.5v11h9V3h-.5a.5.5 0 010-1h.5a1 1 0 011 1v11a1 1 0 01-1 1H3a1 1 0 01-1-1V3a1 1 0 011-1z"/></svg>
          {{ t('paste') }}
          <span class="gctx-key">Ctrl+V</span>
        </button>
        <div class="gctx-sep" />
        <button class="gctx-item" @click="selectAll">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M2 0h12a2 2 0 012 2v12a2 2 0 01-2 2H2a2 2 0 01-2-2V2a2 2 0 012-2zm.854 4.146l3 3a.5.5 0 010 .708l-3 3a.5.5 0 01-.708-.708L4.793 7.5 2.146 4.854a.5.5 0 11.708-.708zM7.5 11a.5.5 0 000 1h5a.5.5 0 000-1h-5z"/></svg>
          {{ t('selectAll') }}
          <span class="gctx-key">Ctrl+A</span>
        </button>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.gctx-overlay { position: fixed; inset: 0; z-index: 88888; }
.gctx-menu {
  position: fixed; z-index: 88889;
  background: var(--bg-panel); border: 1px solid var(--border);
  border-radius: 8px; padding: 4px; min-width: 160px;
  box-shadow: 0 8px 32px rgba(0,0,0,0.6);
  font-family: var(--font-ui);
}
.gctx-item {
  display: flex; align-items: center; gap: 8px;
  width: 100%; background: none; border: none;
  color: var(--fg); font-size: 12.5px;
  padding: 6px 10px; border-radius: 5px; cursor: pointer;
  transition: background 0.1s; text-align: left;
}
.gctx-item:hover { background: var(--bg-hover); }
.gctx-key { margin-left: auto; font-size: 10.5px; color: var(--fg-muted); }
.gctx-sep { height: 1px; background: var(--border); margin: 3px 0; }
</style>
