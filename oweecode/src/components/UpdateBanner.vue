<script setup lang="ts">
// Slim always-mounted banner (see App.vue) — invisible until an update is
// found. `check()` is a pure version-string comparison against latest.json,
// so it reports an update regardless of how the app was installed — but the
// updater plugin can only self-replace an AppImage (or NSIS/MSI/macOS
// builds); a .deb/.rpm install lives in a system directory a regular
// process can't overwrite, so `installUpdate()` fails there. useAppUpdater
// detects that (Linux + install failure) and sets `manualUpdateUrl` instead
// of a raw error, so this banner can point straight at the release page.
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
    <span v-if="state.manualUpdateUrl" class="upd-manual-text">{{ t('updateManualNeeded') }}</span>
    <span v-else-if="state.error" class="upd-error">{{ state.error }}</span>
    <div class="upd-actions">
      <template v-if="state.manualUpdateUrl">
        <a class="upd-btn upd-btn--primary" :href="state.manualUpdateUrl" target="_blank">{{ t('updateManualLink') }}</a>
        <button class="upd-btn" @click="dismiss">{{ t('updateLater') }}</button>
      </template>
      <template v-else-if="!state.downloading && !state.installing">
        <button class="upd-btn upd-btn--primary" @click="installUpdate">{{ t('updateInstallNow') }}</button>
        <button class="upd-btn" @click="dismiss">{{ t('updateLater') }}</button>
      </template>
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
.upd-manual-text { color: var(--fg-muted); font-size: 10.5px; }
.upd-actions { display: flex; gap: 6px; margin-left: auto; }
.upd-btn {
  background: none; border: 1px solid var(--border); border-radius: 5px;
  color: var(--fg-muted); font-size: 10.5px; font-weight: 600; padding: 4px 10px; cursor: pointer;
  display: inline-flex; align-items: center; text-decoration: none;
}
.upd-btn:hover { background: var(--bg-hover); color: var(--fg); }
.upd-btn--primary { background: var(--accent); color: var(--accent-fg); border: none; }
.upd-btn--primary:hover { opacity: 0.85; }
</style>
