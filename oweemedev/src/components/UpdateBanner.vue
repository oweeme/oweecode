<script setup lang="ts">
// Slim always-mounted banner (see App.vue) — invisible until an update is
// found. Not shown at all for .deb installs' target platform limitation:
// the updater plugin can only self-update AppImage/NSIS/MSI/macOS builds,
// never a system-package-manager install — a .deb user just never gets an
// Update object back from `check()`, so this banner naturally never appears
// for them; there's nothing to special-case here.
import { onMounted } from 'vue'
import { useAppUpdater } from '../composables/useAppUpdater'
import { useI18n } from '../composables/useI18n'

const { state, checkForUpdates, installUpdate, dismiss } = useAppUpdater()
const { t } = useI18n()

onMounted(() => {
  checkForUpdates()
})
</script>

<template>
  <div v-if="state.available && !state.dismissed" class="upd-banner">
    <span class="upd-icon">⬆</span>
    <span class="upd-text">
      <template v-if="state.installing">{{ t('updateInstalling') }}</template>
      <template v-else-if="state.downloading">{{ t('updateDownloading') }} {{ state.progress }}%</template>
      <template v-else>{{ t('updateAvailable') }} v{{ state.version }}</template>
    </span>
    <div v-if="state.downloading" class="upd-progress-bar"><div class="upd-progress-fill" :style="{ width: state.progress + '%' }" /></div>
    <span v-if="state.error" class="upd-error">{{ state.error }}</span>
    <div class="upd-actions">
      <button v-if="!state.downloading && !state.installing" class="upd-btn upd-btn--primary" @click="installUpdate">{{ t('updateInstallNow') }}</button>
      <button v-if="!state.downloading && !state.installing" class="upd-btn" @click="dismiss">{{ t('updateLater') }}</button>
    </div>
  </div>
</template>

<style scoped>
.upd-banner {
  position: fixed; bottom: 12px; right: 12px; z-index: 9000;
  display: flex; align-items: center; gap: 10px;
  background: var(--bg-dark); border: 1px solid var(--accent); border-radius: 8px;
  padding: 8px 12px; box-shadow: 0 8px 24px rgba(0,0,0,0.4);
  font-family: var(--font-ui); font-size: 12px; color: var(--fg);
  max-width: 420px;
}
.upd-icon { color: var(--accent); font-size: 14px; }
.upd-text { flex-shrink: 0; }
.upd-progress-bar { width: 60px; height: 5px; background: var(--bg-darker); border-radius: 3px; overflow: hidden; flex-shrink: 0; }
.upd-progress-fill { height: 100%; background: var(--accent); transition: width 0.2s; }
.upd-error { color: #f85149; font-size: 10.5px; }
.upd-actions { display: flex; gap: 6px; margin-left: auto; }
.upd-btn {
  background: none; border: 1px solid var(--border); border-radius: 5px;
  color: var(--fg-muted); font-size: 10.5px; font-weight: 600; padding: 4px 10px; cursor: pointer;
}
.upd-btn:hover { background: var(--bg-hover); color: var(--fg); }
.upd-btn--primary { background: var(--accent); color: var(--accent-fg); border: none; }
.upd-btn--primary:hover { opacity: 0.85; }
</style>
