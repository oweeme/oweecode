<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from '../composables/useI18n'
import { SHORTCUT_GROUPS } from '../composables/shortcuts'

defineEmits<{ close: [] }>()
const { t } = useI18n()

// Rendered straight from the shared registry (src/composables/shortcuts.ts) —
// that file is the single source of truth new shortcuts get added to, so
// this modal can't drift out of sync with what's actually implemented.
const GROUPS = computed(() => SHORTCUT_GROUPS.map(g => ({
  title: t(g.titleKey as any),
  items: g.items.map(item => ({ keys: item.keys, desc: t(item.descKey as any) })),
})))
</script>

<template>
  <div class="sc-overlay" @click.self="$emit('close')">
    <div class="sc-modal">
      <div class="sc-header">
        <span class="sc-title">{{ t('scKeyboardShortcuts') }}</span>
        <button class="sc-close" @click="$emit('close')">×</button>
      </div>

      <div class="sc-body">
        <div v-for="g in GROUPS" :key="g.title" class="sc-group">
          <div class="sc-group-title">{{ g.title.toUpperCase() }}</div>
          <div class="sc-row" v-for="item in g.items" :key="item.desc">
            <span class="sc-desc">{{ item.desc }}</span>
            <span class="sc-keys">
              <kbd v-for="(k, i) in item.keys" :key="i" class="sc-key">{{ k }}</kbd>
            </span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.sc-overlay {
  position: fixed; inset: 0; z-index: 99999;
  background: rgba(0,0,0,0.65); backdrop-filter: blur(4px);
  display: flex; align-items: center; justify-content: center;
}
.sc-modal {
  background: var(--bg-panel); border: 1px solid var(--border);
  border-radius: 14px; width: 520px; max-height: 80vh;
  display: flex; flex-direction: column;
  box-shadow: 0 24px 80px rgba(0,0,0,0.8);
  font-family: var(--font-ui);
}
.sc-header {
  display: flex; align-items: center; justify-content: space-between;
  padding: 16px 20px 12px; border-bottom: 1px solid var(--border); flex-shrink: 0;
}
.sc-title { font-size: 14px; font-weight: 700; color: var(--fg-bright); }
.sc-close {
  background: none; border: none; color: var(--fg-muted);
  font-size: 20px; cursor: pointer; width: 28px; height: 28px;
  border-radius: 6px; display: flex; align-items: center; justify-content: center;
}
.sc-close:hover { background: var(--bg-hover); color: var(--fg); }

.sc-body { overflow-y: auto; padding: 12px 20px 16px; }
.sc-body::-webkit-scrollbar { width: 4px; }
.sc-body::-webkit-scrollbar-thumb { background: var(--bg-active); border-radius: 2px; }

.sc-group { margin-bottom: 18px; }
.sc-group-title {
  font-size: 10px; font-weight: 700; letter-spacing: 1.2px;
  color: var(--accent); margin-bottom: 8px;
}
.sc-row {
  display: flex; align-items: center; justify-content: space-between;
  padding: 5px 0; border-bottom: 1px solid rgba(255,255,255,0.04);
}
.sc-row:last-child { border-bottom: none; }
.sc-desc { font-size: 12.5px; color: var(--fg-dim); }
.sc-keys { display: flex; align-items: center; gap: 3px; }
.sc-key {
  display: inline-flex; align-items: center; justify-content: center;
  background: var(--bg-mid); border: 1px solid var(--border);
  border-bottom-width: 2px; border-radius: 4px;
  padding: 2px 7px; font-size: 11px; font-family: var(--font-mono);
  color: var(--fg); min-width: 22px;
}
</style>
