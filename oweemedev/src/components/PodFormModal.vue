<script setup lang="ts">
// Minimal pod-creation form — a pod only needs a name and (optionally) ports
// published at the pod level, since containers that join it later share its
// network namespace instead of getting their own. Much smaller than
// ContainerFormModal.vue on purpose: no volumes/env/command here, those
// belong to the containers that join the pod, not the pod itself.
import { reactive, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '../composables/useI18n'

const { t } = useI18n()
const props = defineProps<{ rootPath: string }>()
const emit = defineEmits<{ close: []; created: [] }>()

interface PortRow { host: string; container: string; protocol: string }

const name = ref('')
const ports = reactive<PortRow[]>([])
const saving = ref(false)
const saveError = ref('')

function addPort() { ports.push({ host: '', container: '', protocol: 'tcp' }) }
function removePort(i: number) { ports.splice(i, 1) }

async function submit() {
  if (!name.value.trim()) { saveError.value = t('podNameRequired'); return }
  saving.value = true
  saveError.value = ''
  try {
    await invoke('pod_create', {
      name: name.value.trim(),
      ports: ports.filter(p => p.host.trim() && p.container.trim()),
      label: props.rootPath,
    })
    emit('created')
    emit('close')
  } catch (e: any) {
    saveError.value = String(e)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div class="pf-overlay" @click.self="emit('close')">
    <div class="pf-modal">
      <div class="pf-head">
        <span>{{ t('podFormTitle') }}</span>
        <button class="pf-close" @click="emit('close')">×</button>
      </div>

      <div class="pf-body">
        <p class="pf-hint">{{ t('podFormHint') }}</p>

        <div class="pf-field">
          <label>{{ t('podNameLabel') }}</label>
          <input v-model="name" class="pf-input" :placeholder="t('podNamePlaceholder')" @keydown.enter="submit" />
        </div>

        <div class="pf-section">
          <div class="pf-section-head">
            <span>{{ t('podPortsLabel') }}</span>
            <button class="pf-add-row-btn" @click="addPort">+ {{ t('addPort') }}</button>
          </div>
          <div v-for="(p, i) in ports" :key="i" class="pf-row">
            <input v-model="p.host" class="pf-input pf-input-sm" :placeholder="t('hostPort')" />
            <span class="pf-sep">:</span>
            <input v-model="p.container" class="pf-input pf-input-sm" :placeholder="t('containerPort')" />
            <select v-model="p.protocol" class="pf-input pf-input-xs">
              <option value="tcp">TCP</option>
              <option value="udp">UDP</option>
            </select>
            <button class="pf-row-del" @click="removePort(i)">×</button>
          </div>
        </div>

        <div v-if="saveError" class="pf-inline-error">{{ saveError }}</div>
      </div>

      <div class="pf-foot">
        <button class="pf-cancel-btn" @click="emit('close')">{{ t('cancel') }}</button>
        <button class="pf-save-btn" :disabled="saving || !name.trim()" @click="submit">
          <span v-if="saving" class="pf-spin" />
          <span v-else>{{ t('create') }}</span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.pf-overlay {
  position: fixed; inset: 0; z-index: 99999; background: rgba(0,0,0,0.6);
  display: flex; align-items: center; justify-content: center; backdrop-filter: blur(3px);
}
.pf-modal {
  background: var(--bg-panel); border: 1px solid var(--border); border-radius: 12px;
  width: 380px; max-height: 80vh; display: flex; flex-direction: column;
  box-shadow: 0 20px 60px rgba(0,0,0,0.6); font-family: var(--font-ui);
}
.pf-head {
  display: flex; align-items: center; justify-content: space-between;
  padding: 14px 16px; border-bottom: 1px solid var(--border);
  font-size: 13.5px; font-weight: 700; color: var(--fg-bright); flex-shrink: 0;
}
.pf-close { background: none; border: none; color: var(--fg-muted); font-size: 18px; cursor: pointer; line-height: 1; }
.pf-close:hover { color: var(--fg); }

.pf-body { flex: 1; overflow-y: auto; padding: 14px 16px; display: flex; flex-direction: column; gap: 12px; }
.pf-hint { font-size: 10.5px; color: var(--fg-muted); line-height: 1.5; margin: 0; }
.pf-field { display: flex; flex-direction: column; gap: 4px; }
.pf-field label { font-size: 10.5px; color: var(--fg-muted); font-weight: 600; }
.pf-input {
  background: var(--bg-mid); border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg); font-size: 12px; padding: 6px 8px; outline: none; font-family: var(--font-ui);
  box-sizing: border-box; width: 100%;
}
.pf-input:focus { border-color: var(--accent); }
.pf-input-sm { width: 70px; flex: 1; }
.pf-input-xs { width: 62px; flex: none; }

.pf-section { display: flex; flex-direction: column; gap: 6px; }
.pf-section-head { display: flex; align-items: center; justify-content: space-between; }
.pf-section-head > span { font-size: 10.5px; color: var(--fg-muted); font-weight: 600; }
.pf-add-row-btn { background: none; border: none; color: var(--accent); font-size: 10.5px; cursor: pointer; padding: 2px 4px; }
.pf-add-row-btn:hover { text-decoration: underline; }

.pf-row { display: flex; align-items: center; gap: 6px; }
.pf-row .pf-input { flex: 1; min-width: 0; }
.pf-sep { color: var(--fg-muted); font-size: 12px; flex-shrink: 0; }
.pf-row-del {
  width: 20px; height: 20px; flex-shrink: 0; background: none; border: none;
  color: var(--fg-muted); font-size: 14px; cursor: pointer; border-radius: 4px;
  display: flex; align-items: center; justify-content: center;
}
.pf-row-del:hover { color: #f85149; background: var(--bg-hover); }

.pf-inline-error { font-size: 11px; color: #f85149; padding: 6px 8px; background: rgba(248,81,73,0.1); border-radius: 5px; }

.pf-foot { display: flex; gap: 8px; padding: 12px 16px; border-top: 1px solid var(--border); flex-shrink: 0; }
.pf-cancel-btn {
  flex: 1; background: none; border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg-muted); font-size: 12px; padding: 7px; cursor: pointer;
}
.pf-cancel-btn:hover { border-color: var(--fg-muted); color: var(--fg); }
.pf-save-btn {
  flex: 2; background: var(--accent); border: none; border-radius: 6px;
  color: var(--accent-fg); font-size: 12px; font-weight: 600; padding: 7px; cursor: pointer;
  display: flex; align-items: center; justify-content: center;
}
.pf-save-btn:hover:not(:disabled) { opacity: 0.85; }
.pf-save-btn:disabled { opacity: 0.5; cursor: default; }
.pf-spin {
  width: 12px; height: 12px; border: 2px solid rgba(255,255,255,0.4);
  border-top-color: var(--accent-fg); border-radius: 50%; animation: pf-spin 0.7s linear infinite; display: inline-block;
}
@keyframes pf-spin { to { transform: rotate(360deg); } }
</style>
