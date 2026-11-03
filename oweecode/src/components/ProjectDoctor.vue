<script setup lang="ts">
// "Doctor" of the open project: what each detected project type needs and what this machine has.
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useEditorStore } from '../composables/useEditorStore'
import { useI18n } from '../composables/useI18n'
import { rustText } from '../composables/rustText'

const emit = defineEmits<{ close: [] }>()
const { t, locale } = useI18n()
const rt = (s: string) => rustText(s, locale.value)
const store = useEditorStore()
interface Item { group: string; name: string; ok: boolean; optional: boolean; detail: string; hint: string }
const items = ref<Item[]>([])
const loading = ref(true)
const error = ref('')
const copied = ref('')

async function load() {
  loading.value = true; error.value = ''
  try { items.value = await invoke<Item[]>('project_doctor', { root: store.state.rootPath }) }
  catch (e) { error.value = String(e) }
  finally { loading.value = false }
}
const groups = computed(() => {
  const m = new Map<string, Item[]>()
  for (const i of items.value) { if (!m.has(i.group)) m.set(i.group, []); m.get(i.group)!.push(i) }
  return [...m.entries()]
})
const missing = computed(() => items.value.filter(i => !i.ok && !i.optional).length)
const optionalMissing = computed(() => items.value.filter(i => !i.ok && i.optional).length)
async function copy(text: string) { try { await navigator.clipboard.writeText(text); copied.value = text; setTimeout(() => { if (copied.value === text) copied.value = '' }, 1500) } catch { /* ignore */ } }
const isCommand = (h: string) => /^(sudo |pip |python3 |mise |npm |npx |flutter )/.test(h)
function run(cmd: string) { emit('close'); window.dispatchEvent(new CustomEvent('owee-run', { detail: cmd.replace(/\s{2,}\(.*$/, '').replace(/\s+#.*/, '') })) }
onMounted(load)
</script>

<template>
  <Teleport to="body">
    <div class="pd-overlay" @click.self="emit('close')">
      <div class="pd-box">
        <div class="pd-head">
          <span>{{ t('dTitle') }} <small>{{ store.state.rootPath.split('/').pop() }}</small></span>
          <div><button class="pd-btn" @click="load">{{ t('dRecheck') }}</button><button class="pd-x" @click="emit('close')">✕</button></div>
        </div>
        <div v-if="loading" class="pd-msg">{{ t('dChecking') }}</div>
        <div v-else-if="error" class="pd-msg err">{{ error }}</div>
        <template v-else>
          <div class="pd-sum" :class="missing ? 'bad' : 'good'">
            {{ missing ? t('dMissing', { n: missing }) : t('dAllOk') }}<span v-if="optionalMissing"> · {{ t('dOptMissing', { n: optionalMissing }) }}</span>
          </div>
          <div v-for="[g, list] in groups" :key="g" class="pd-group">
            <div class="pd-gt">{{ rt(g) }}</div>
            <div v-for="i in list" :key="i.name" class="pd-row" :class="{ miss: !i.ok && !i.optional }">
              <span class="pd-ic" :class="i.ok ? 'ok' : i.optional ? 'opt' : 'no'">{{ i.ok ? '✓' : i.optional ? '·' : '✗' }}</span>
              <div class="pd-main">
                <div>{{ rt(i.name) }}<em v-if="i.optional"> {{ t('dOptional') }}</em></div>
                <div v-if="i.ok" class="pd-det">{{ rt(i.detail) }}</div>
                <div v-else class="pd-hint">
                  <code>{{ rt(i.hint) }}</code>
                  <span v-if="i.hint">
                    <button class="pd-btn sm" @click="copy(i.hint)">{{ copied === i.hint ? t('dCopied') : t('dCopy') }}</button>
                    <button v-if="isCommand(i.hint)" class="pd-btn sm" @click="run(i.hint)">{{ t('dRun') }}</button>
                  </span>
                </div>
              </div>
            </div>
          </div>
        </template>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.pd-overlay { position: fixed; inset: 0; background: rgba(0,0,0,.55); z-index: 400; display: flex; align-items: center; justify-content: center; }
.pd-box { width: 640px; max-width: 94vw; max-height: 82vh; overflow: auto; background: var(--bg-dark, #141416); border: 1px solid var(--border); border-radius: 10px; padding: 14px 16px; color: var(--fg); box-shadow: 0 16px 48px rgba(0,0,0,.6); }
.pd-head { display: flex; justify-content: space-between; align-items: center; font-weight: 600; margin-bottom: 8px; } .pd-head small { font-weight: 400; color: var(--fg-muted); margin-left: 8px; }
.pd-x { background: none; border: none; color: var(--fg-muted); font-size: 15px; cursor: pointer; margin-left: 8px; }
.pd-btn { background: none; border: 1px solid var(--border); color: var(--fg); font-size: 11px; padding: 3px 9px; border-radius: 4px; cursor: pointer; } .pd-btn:hover { background: var(--bg-hover); } .pd-btn.sm { margin-left: 6px; padding: 1px 7px; font-size: 10.5px; }
.pd-sum { padding: 7px 10px; border-radius: 6px; font-size: 12px; margin-bottom: 8px; } .pd-sum.good { background: #3fb95022; color: #3fb950; } .pd-sum.bad { background: #f8514922; color: #f85149; }
.pd-gt { font-size: 10.5px; text-transform: uppercase; letter-spacing: .06em; color: var(--fg-muted); margin: 10px 0 4px; }
.pd-row { display: flex; gap: 10px; padding: 5px 4px; font-size: 12.5px; border-bottom: 1px solid var(--border); } .pd-row.miss { background: #f8514910; }
.pd-ic { width: 16px; font-weight: 700; text-align: center; } .pd-ic.ok { color: #3fb950; } .pd-ic.no { color: #f85149; } .pd-ic.opt { color: var(--fg-muted); }
.pd-main { flex: 1; min-width: 0; } .pd-main em { color: var(--fg-muted); font-style: normal; font-size: 11px; }
.pd-det { font-size: 11px; color: var(--fg-muted); word-break: break-all; } .pd-hint { font-size: 11.5px; margin-top: 2px; } .pd-hint code { background: var(--bg-darkest); padding: 1px 5px; border-radius: 3px; word-break: break-all; }
.pd-msg { padding: 20px; text-align: center; color: var(--fg-muted); } .pd-msg.err { color: #f85149; }
</style>
