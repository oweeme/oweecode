<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useI18n } from '../composables/useI18n'

const appWindow = getCurrentWindow()
const { t } = useI18n()

function minimize() { appWindow.minimize() }
function maximize() { appWindow.toggleMaximize() }
function close() { appWindow.close() }
</script>

<template>
  <div class="titlebar" data-tauri-drag-region>
    <div class="titlebar-logo" data-tauri-drag-region>
      <img src="/oweedev.png" width="18" height="18" style="border-radius:4px;display:block;" />
      <span class="titlebar-name">OweeCode</span>
    </div>
    <div class="titlebar-spacer" data-tauri-drag-region />
    <div class="titlebar-controls">
      <button class="tb-btn tb-minimize" @click="minimize" :title="t('minimizeTitle')">
        <svg width="10" height="10" viewBox="0 0 10 10"><rect y="4.5" width="10" height="1" fill="currentColor"/></svg>
      </button>
      <button class="tb-btn tb-maximize" @click="maximize" :title="t('maximizeTitle')">
        <svg width="10" height="10" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" stroke="currentColor" fill="none"/></svg>
      </button>
      <button class="tb-btn tb-close" @click="close" :title="t('closeTitle')">
        <svg width="10" height="10" viewBox="0 0 10 10">
          <line x1="1" y1="1" x2="9" y2="9" stroke="currentColor" stroke-width="1.3"/>
          <line x1="9" y1="1" x2="1" y2="9" stroke="currentColor" stroke-width="1.3"/>
        </svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.titlebar {
  height: 30px;
  background: var(--bg-darkest);
  border-bottom: 1px solid var(--border);
  display: flex;
  align-items: center;
  flex-shrink: 0;
  user-select: none;
  -webkit-user-select: none;
}

.titlebar-logo {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 0 12px;
}

.titlebar-name {
  font-size: 12px;
  font-weight: 700;
  color: var(--fg-bright);
  letter-spacing: 0.4px;
  font-family: 'Segoe UI', system-ui, sans-serif;
}

.titlebar-spacer {
  flex: 1;
}

.titlebar-controls {
  display: flex;
  align-items: stretch;
  height: 100%;
}

.tb-btn {
  width: 42px;
  height: 100%;
  background: none;
  border: none;
  color: var(--fg-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background 0.12s, color 0.12s;
}

.tb-btn:hover { background: var(--bg-hover); color: var(--fg); }
.tb-close:hover { background: var(--red) !important; color: #fff !important; }
</style>
