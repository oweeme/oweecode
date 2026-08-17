<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from '../composables/useI18n'

defineProps<{ podName: string }>()
const emit = defineEmits<{ close: []; confirm: [full: boolean] }>()
const { t } = useI18n()

const full = ref(true)
</script>

<template>
  <div class="pe-overlay" @click.self="emit('close')">
    <div class="pe-modal">
      <div class="pe-head">
        <span>{{ t('exportPod') }}: {{ podName }}</span>
        <button class="pe-close" @click="emit('close')">×</button>
      </div>

      <div class="pe-body">
        <label class="pe-option" :class="{ selected: full }">
          <input type="radio" :checked="full" @change="full = true" />
          <div>
            <div class="pe-option-title">{{ t('exportModeFull') }}</div>
            <div class="pe-option-hint">{{ t('exportModeFullHint') }}</div>
          </div>
        </label>
        <label class="pe-option" :class="{ selected: !full }">
          <input type="radio" :checked="!full" @change="full = false" />
          <div>
            <div class="pe-option-title">{{ t('exportModeLight') }}</div>
            <div class="pe-option-hint">{{ t('exportModeLightHint') }}</div>
          </div>
        </label>
      </div>

      <div class="pe-foot">
        <button class="pe-cancel-btn" @click="emit('close')">{{ t('cancel') }}</button>
        <button class="pe-save-btn" @click="emit('confirm', full)">{{ t('exportPod') }}</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.pe-overlay {
  position: fixed; inset: 0; z-index: 99999; background: rgba(0,0,0,0.6);
  display: flex; align-items: center; justify-content: center; backdrop-filter: blur(3px);
}
.pe-modal {
  background: var(--bg-panel); border: 1px solid var(--border); border-radius: 12px;
  width: 400px; max-height: 80vh; display: flex; flex-direction: column;
  box-shadow: 0 20px 60px rgba(0,0,0,0.6); font-family: var(--font-ui);
}
.pe-head {
  display: flex; align-items: center; justify-content: space-between;
  padding: 14px 16px; border-bottom: 1px solid var(--border);
  font-size: 13.5px; font-weight: 700; color: var(--fg-bright); flex-shrink: 0;
}
.pe-close { background: none; border: none; color: var(--fg-muted); font-size: 18px; cursor: pointer; line-height: 1; }
.pe-close:hover { color: var(--fg); }

.pe-body { flex: 1; overflow-y: auto; padding: 14px 16px; display: flex; flex-direction: column; gap: 10px; }
.pe-option {
  display: flex; align-items: flex-start; gap: 10px; padding: 10px 12px;
  border: 1px solid var(--border); border-radius: 8px; cursor: pointer; transition: border-color 0.15s;
}
.pe-option:hover { border-color: var(--fg-muted); }
.pe-option.selected { border-color: var(--accent); background: rgba(166,227,161,0.06); }
.pe-option input { margin-top: 2px; flex-shrink: 0; }
.pe-option-title { font-size: 12.5px; font-weight: 600; color: var(--fg-bright); }
.pe-option-hint { font-size: 10.5px; color: var(--fg-muted); line-height: 1.5; margin-top: 2px; }

.pe-foot { display: flex; gap: 8px; padding: 12px 16px; border-top: 1px solid var(--border); flex-shrink: 0; }
.pe-cancel-btn {
  flex: 1; background: none; border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg-muted); font-size: 12px; padding: 7px; cursor: pointer;
}
.pe-cancel-btn:hover { border-color: var(--fg-muted); color: var(--fg); }
.pe-save-btn {
  flex: 2; background: var(--accent); border: none; border-radius: 6px;
  color: var(--accent-fg); font-size: 12px; font-weight: 600; padding: 7px; cursor: pointer;
}
.pe-save-btn:hover { opacity: 0.85; }
</style>
