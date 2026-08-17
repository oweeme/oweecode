<script setup lang="ts">
import { ref, nextTick, computed, watch, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useEditorStore } from '../composables/useEditorStore'
import { useI18n } from '../composables/useI18n'
import { useProjectMemory } from '../composables/useProjectMemory'
import { renderContent } from '../composables/aiMarkdown'
import { AI_PROVIDERS, loadProviderKey, saveProviderKey, type Provider } from '../composables/aiProviders'

const { t } = useI18n()

const emit = defineEmits<{ (e: 'run-terminal', cmd: string): void }>()

interface Message {
  role: 'user' | 'assistant'
  content: string
  context?: string  // file/selection context label shown in UI
}

const store = useEditorStore()
const messages = ref<Message[]>([])
const input = ref('')
const isLoading = ref(false)
const error = ref('')
const messagesEl = ref<HTMLElement | null>(null)
const showSettings = ref(false)
const applyTarget = ref<{ msgIdx: number; code: string } | null>(null)

// ── Shared project context (AGENTS.md) — see useProjectMemory.ts ───────────
const { projectContext, savingContext, contextStatus, loadProjectContext, saveProjectContext: saveProjectContextRaw } = useProjectMemory()

async function saveProjectContext() {
  if (messages.value.length === 0) return
  const transcript = messages.value.map(m => `${m.role === 'user' ? 'Usuario' : 'Asistente'}: ${m.content}`).join('\n\n')
  const ok = await saveProjectContextRaw(store.state.rootPath, transcript, { provider: provider.value, apiKey: apiKey.value, model: model.value })
  if (!ok) error.value = t('noProjectForContext')
}

const provider = ref<Provider>((localStorage.getItem('ai_provider') as Provider) ?? 'claude')
const apiKey = ref(loadProviderKey(provider.value))
const model = ref(localStorage.getItem('ai_model') ?? 'claude-sonnet-4-6')
const ollamaModels = ref<string[]>([])
const ollamaLoading = ref(false)
const ollamaError = ref('')

// OpenRouter's free-tier catalog changes over time (models get discontinued or
// lose their free variant), so a hardcoded list goes stale — fetch what's
// actually free right now instead.
const openrouterModels = ref<{ id: string; label: string }[]>([])
const openrouterLoading = ref(false)
const openrouterError = ref('')

const currentProvider = computed(() => AI_PROVIDERS[provider.value])
const modelOptions = computed(() => AI_PROVIDERS[provider.value].models)

// Context awareness
const activeTab = computed(() => store.activeTab())
const selectedText = computed(() => store.state.selectedText)
const hasSelection = computed(() => selectedText.value.trim().length > 0)
const hasFile = computed(() => activeTab.value?.type === 'code')

const contextLabel = computed(() => {
  if (hasSelection.value) return `${activeTab.value?.name ?? 'archivo'} · selección (${selectedText.value.split('\n').length} líneas)`
  if (hasFile.value) return activeTab.value?.name ?? ''
  return 'sin archivo activo'
})

function buildContext(includeSelection: boolean): string {
  const tab = activeTab.value
  if (!tab || tab.type !== 'code') return ''

  const selection = selectedText.value.trim()
  if (includeSelection && selection) {
    return `\n\nArchivo activo: ${tab.name} (${tab.language})\nCódigo seleccionado:\n\`\`\`${tab.language}\n${selection}\n\`\`\``
  }
  const preview = tab.content.slice(0, 4000)
  const truncated = tab.content.length > 4000 ? '\n... (truncado)' : ''
  return `\n\nArchivo activo: ${tab.name} (${tab.language})\n\`\`\`${tab.language}\n${preview}${truncated}\n\`\`\``
}

function getSystemPrompt(): string {
  return `You are an expert programming assistant integrated into OweeCode, a modern IDE built with Tauri and Vue 3.
You help with: PHP, Go, Vue, JavaScript, TypeScript, React/JSX, Svelte, Astro, Python, Rust, SQL, HTML, CSS, and more.
When providing code fixes or improvements, always use code blocks with the correct language tag.
Be concise and practical. Respond in the same language the user writes in (Spanish or English).`
}

async function detectOllamaModels() {
  ollamaLoading.value = true; ollamaError.value = ''
  try {
    const models = await invoke<string[]>('list_ollama_models')
    ollamaModels.value = models
    // Only jump to a different model if the saved one isn't actually pulled —
    // otherwise this would silently reset the user's choice back to models[0]
    // every time the settings panel reopens.
    if (models.length > 0 && !models.includes(model.value)) model.value = models[0]
    else if (models.length === 0) ollamaError.value = 'No hay modelos descargados. Usá: ollama pull qwen2.5-coder:7b-instruct-q4_K_M'
  } catch (e: any) { ollamaError.value = String(e) }
  ollamaLoading.value = false
}

async function detectOpenrouterModels() {
  openrouterLoading.value = true; openrouterError.value = ''
  try {
    const models = await invoke<{ id: string; name: string }[]>('list_openrouter_free_models')
    openrouterModels.value = models.map(m => ({ id: m.id, label: `${m.name} (Gratis)` }))
    if (models.length > 0 && !models.some(m => m.id === model.value)) {
      model.value = models[0].id
      // Persist right away (not just on "Guardar y Cerrar") so a stale saved
      // model — which OpenRouter would reject outright — gets fixed even if the
      // user never reopens this settings panel, since CliPanel's Aider launch
      // reads ai_model straight from localStorage, independently of this ref.
      if (provider.value === 'openrouter') localStorage.setItem('ai_model', model.value)
    }
    if (models.length === 0) openrouterError.value = 'OpenRouter no reporta modelos gratis en este momento.'
  } catch (e: any) { openrouterError.value = String(e) }
  openrouterLoading.value = false
}

// Chat history persists per-project (keyed by root path) so switching tabs or
// reopening the IDE doesn't silently wipe the conversation — it only ever
// lived in an in-memory ref before, which is what "no muestra la historia"
// was actually about.
function chatStorageKey(): string {
  return `ai_chat_${store.state.rootPath || 'no-project'}`
}

function persistChat() {
  try { localStorage.setItem(chatStorageKey(), JSON.stringify(messages.value)) } catch { /* quota/etc — chat still works, just won't persist */ }
}

// Self-heal on load too, not just when the user reopens settings — a model
// that was free when last saved can stop being free at any time.
onMounted(() => {
  if (provider.value === 'openrouter') detectOpenrouterModels()
  if (provider.value === 'ollama') detectOllamaModels()
  try {
    const saved = localStorage.getItem(chatStorageKey())
    if (saved) messages.value = JSON.parse(saved)
  } catch { /* corrupt/old entry — start fresh rather than crash the panel */ }
  loadProjectContext(store.state.rootPath)
})

function saveSettings() {
  localStorage.setItem('ai_provider', provider.value)
  saveProviderKey(provider.value, apiKey.value)
  localStorage.setItem('ai_model', model.value)
  showSettings.value = false
}

function onProviderChange() {
  model.value = AI_PROVIDERS[provider.value].models[0].id
  apiKey.value = loadProviderKey(provider.value)
  if (provider.value === 'openrouter' && openrouterModels.value.length === 0) detectOpenrouterModels()
  if (provider.value === 'ollama') detectOllamaModels()
}

watch(showSettings, (open) => {
  if (!open) return
  if (provider.value === 'openrouter' && openrouterModels.value.length === 0) detectOpenrouterModels()
  if (provider.value === 'ollama') detectOllamaModels()
})

// Quick action shortcuts
function quickAsk(prompt: string, useSelection = true) {
  input.value = prompt
  sendMessage(useSelection)
}

async function sendMessage(useSelection = true) {
  const text = input.value.trim()
  if (!text || isLoading.value) return
  if (!apiKey.value && provider.value !== 'ollama') {
    showSettings.value = true
    error.value = 'Configura tu API key ⚙'
    return
  }

  const ctx = buildContext(useSelection)
  const contextInfo = hasSelection.value && useSelection
    ? `📎 ${activeTab.value?.name} · selección`
    : hasFile.value
      ? `📎 ${activeTab.value?.name}`
      : undefined

  input.value = ''
  error.value = ''
  messages.value.push({ role: 'user', content: text, context: contextInfo })
  isLoading.value = true
  await scrollDown()

  try {
    const history = messages.value
      .slice(-12)
      .map(m => ({ role: m.role, content: m.content }))

    const agentsCtx = projectContext.value ? `\n\n--- AGENTS.md (contexto acumulado del proyecto) ---\n${projectContext.value}` : ''
    const systemWithCtx = getSystemPrompt() + agentsCtx + ctx

    const reply = await invoke<string>('call_ai', {
      provider: provider.value,
      apiKey: apiKey.value,
      model: model.value,
      system: systemWithCtx,
      messages: history,
    })
    messages.value.push({ role: 'assistant', content: reply })
  } catch (e: any) {
    error.value = String(e)
    messages.value.push({ role: 'assistant', content: `❌ ${String(e)}` })
  }

  persistChat()
  isLoading.value = false
  await scrollDown()
}

async function scrollDown() {
  await nextTick()
  if (messagesEl.value) messagesEl.value.scrollTop = messagesEl.value.scrollHeight
}

function clearChat() { messages.value = []; error.value = ''; localStorage.removeItem(chatStorageKey()) }

// Apply code to file: replace selection if exists, else replace whole file
function applyCode(content: string, msgIdx: number) {
  const codeMatch = content.match(/```[\w]*\n?([\s\S]+?)```/)
  const code = codeMatch ? codeMatch[1].trimEnd() : content.trim()
  applyTarget.value = { msgIdx, code }
}

function confirmApply(replaceAll: boolean) {
  if (!applyTarget.value) return
  const tab = activeTab.value
  if (!tab || tab.type !== 'code') { applyTarget.value = null; return }

  if (replaceAll || !hasSelection.value) {
    store.updateContent(tab.path, applyTarget.value.code)
  } else {
    // Replace selected text in file
    const sel = selectedText.value
    const newContent = tab.content.replace(sel, applyTarget.value.code)
    store.updateContent(tab.path, newContent)
  }
  applyTarget.value = null
}

// Run code block in terminal
function runInTerminal(content: string) {
  const codeMatch = content.match(/```(?:bash|sh|shell|zsh)?\n?([\s\S]+?)```/)
  if (codeMatch) emit('run-terminal', codeMatch[1].trim())
}

function hasRunnable(content: string): boolean {
  return /```(?:bash|sh|shell|zsh)/.test(content)
}

// Insert code at cursor (append)
function insertCode(content: string) {
  const codeMatch = content.match(/```[\w]*\n?([\s\S]+?)```/)
  const code = codeMatch ? codeMatch[1] : content
  const tab = activeTab.value
  if (tab?.type === 'code') store.updateContent(tab.path, tab.content + '\n' + code)
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendMessage() }
}
</script>

<template>
  <div class="ai-panel">
    <!-- Header -->
    <div class="ai-header">
      <div class="ai-header-left">
        <img src="/oweedev.png" width="16" height="16" style="border-radius:4px;flex-shrink:0" />
        <span class="ai-title">AI Code Assistant</span>
        <span class="ai-badge" :style="{ background: currentProvider.color + '22', color: currentProvider.color, borderColor: currentProvider.color + '55' }">
          {{ currentProvider.icon }} {{ currentProvider.badge }}
        </span>
      </div>
      <div class="ai-header-right">
        <button class="ai-hdr-btn" @click="saveProjectContext" :disabled="savingContext || messages.length === 0" :title="t('saveContextTitle')">
          <span v-if="savingContext" class="mini-spin" />
          <svg v-else width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M2 1a1 1 0 00-1 1v12a1 1 0 001 1h12a1 1 0 001-1V4.5a.5.5 0 00-.146-.354l-3-3A.5.5 0 0011.5 1H2zm5 1h1v3.5a.5.5 0 01-.5.5h-4a.5.5 0 01-.5-.5V2h4zm-2 8a2 2 0 114 0 2 2 0 01-4 0z"/></svg>
        </button>
        <button class="ai-hdr-btn" @click="clearChat" :title="t('clearChat')">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M5.5 5.5A.5.5 0 016 6v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm2.5 0a.5.5 0 01.5.5v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm3 .5a.5.5 0 00-1 0v6a.5.5 0 001 0V6z"/><path fill-rule="evenodd" d="M14.5 3a1 1 0 01-1 1H13v9a2 2 0 01-2 2H5a2 2 0 01-2-2V4h-.5a1 1 0 01-1-1V2a1 1 0 011-1H6a1 1 0 011-1h2a1 1 0 011 1h3.5a1 1 0 011 1v1zM4.118 4L4 4.059V13a1 1 0 001 1h6a1 1 0 001-1V4.059L11.882 4H4.118zM2.5 3V2h11v1h-11z"/></svg>
        </button>
        <button class="ai-hdr-btn" @click="showSettings = !showSettings" :title="t('configuration')">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M9.405 1.05c-.413-1.4-2.397-1.4-2.81 0l-.1.34a1.464 1.464 0 01-2.105.872l-.31-.17c-1.283-.698-2.686.705-1.987 1.987l.169.311c.446.82.023 1.841-.872 2.105l-.34.1c-1.4.413-1.4 2.397 0 2.81l.34.1a1.464 1.464 0 01.872 2.105l-.17.31c-.698 1.283.705 2.686 1.987 1.987l.311-.169a1.464 1.464 0 012.105.872l.1.34c.413 1.4 2.397 1.4 2.81 0l.1-.34a1.464 1.464 0 012.105-.872l.31.17c1.283.698 2.686-.705 1.987-1.987l-.169-.311a1.464 1.464 0 01.872-2.105l.34-.1c1.4-.413 1.4-2.397 0-2.81l-.34-.1a1.464 1.464 0 01-.872-2.105l.17-.31c.698-1.283-.705-2.686-1.987-1.987l-.311.169a1.464 1.464 0 01-2.105-.872l-.1-.34zM8 10.93a2.929 2.929 0 110-5.858 2.929 2.929 0 010 5.858z"/></svg>
        </button>
      </div>
    </div>

    <!-- Context bar: shows active file / selection -->
    <div class="ai-context-bar" v-if="hasFile || hasSelection">
      <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor" style="flex-shrink:0;opacity:.6"><path d="M4 0h5.293A1 1 0 0 1 10 .293L13.707 4a1 1 0 0 1 .293.707V14a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V2a2 2 0 0 1 2-2zm5.5 1.5v2a1 1 0 0 0 1 1h2L9.5 1.5z"/></svg>
      <span class="ai-context-label">{{ contextLabel }}</span>
      <span v-if="hasSelection" class="ai-selection-chip">selección activa</span>
    </div>

    <!-- Settings panel -->
    <transition name="slide">
      <div v-if="showSettings" class="ai-settings">
        <div class="settings-title-row">
          <span class="settings-title">Configuración AI</span>
          <button class="settings-close" @click="showSettings = false">✕</button>
        </div>

        <div class="settings-field">
          <label>{{ t('provider') }}</label>
          <div class="provider-grid">
            <label v-for="(info, key) in AI_PROVIDERS" :key="key" class="provider-opt"
              :class="{ active: provider === key }"
              :style="provider === key ? { borderColor: info.color, color: info.color, background: info.color + '15' } : {}">
              <input type="radio" v-model="provider" :value="key" @change="onProviderChange" hidden />
              <span class="provider-icon">{{ info.icon }}</span>
              <span class="provider-name">{{ info.label }}</span>
            </label>
          </div>
        </div>

        <div class="settings-field">
          <label>{{ t('model') }}</label>
          <div v-if="provider === 'ollama'" class="ollama-row">
            <select v-model="model" class="settings-select" style="flex:1">
              <option v-for="m in (ollamaModels.length ? ollamaModels : modelOptions.map(x=>x.id))" :key="m" :value="m">{{ m }}</option>
            </select>
            <button class="detect-btn" @click="detectOllamaModels" :disabled="ollamaLoading">{{ ollamaLoading ? '…' : '⟳' }}</button>
          </div>
          <div v-else-if="provider === 'openrouter'" class="ollama-row">
            <select v-model="model" class="settings-select" style="flex:1">
              <option v-for="m in (openrouterModels.length ? openrouterModels : modelOptions)" :key="m.id" :value="m.id">{{ m.label }}</option>
            </select>
            <button class="detect-btn" @click="detectOpenrouterModels" :disabled="openrouterLoading" :title="t('reload')">{{ openrouterLoading ? '…' : '⟳' }}</button>
          </div>
          <select v-else v-model="model" class="settings-select">
            <option v-for="m in modelOptions" :key="m.id" :value="m.id">{{ m.label }}</option>
          </select>
          <div v-if="ollamaError" class="settings-err">{{ ollamaError }}</div>
          <div v-if="openrouterError" class="settings-err">{{ openrouterError }}</div>
        </div>

        <div class="settings-field" v-if="provider !== 'ollama'">
          <label>{{ currentProvider.keyLabel }}</label>
          <input v-model="apiKey" type="password" class="settings-input" :placeholder="currentProvider.keyPlaceholder" />
          <a class="settings-link" :href="currentProvider.keyUrl" target="_blank">{{ t('getApiKey') }}</a>
        </div>

        <div class="settings-note" :class="provider === 'claude' ? 'note-warn' : ''">
          {{ currentProvider.keyNote }}
          <template v-if="provider === 'claude'">
            <br><br><strong>💡 Sin API key:</strong> En la terminal del IDE escribe <code>claude</code> — usa tu plan Pro directamente.
          </template>
        </div>

        <!-- Claude Code CLI shortcut -->
        <div v-if="provider === 'claude'" class="claude-cli-box">
          <div class="cli-box-title">◆ Claude Code CLI</div>
          <p class="cli-box-desc">Abre Claude Code completo en la terminal del IDE (sin necesidad de API key, usa tu plan Pro)</p>
          <button class="cli-launch-btn" @click="emit('run-terminal', 'claude'); showSettings = false">
            Abrir Claude en Terminal →
          </button>
        </div>

        <button class="settings-save" @click="saveSettings">{{ t('saveAndClose') }}</button>
      </div>
    </transition>

    <!-- Messages -->
    <div ref="messagesEl" class="ai-messages">
        <!-- Empty state with quick actions -->
        <div v-if="messages.length === 0" class="ai-empty">
          <img src="/oweedev.png" width="40" height="40" style="border-radius:10px;opacity:.7;margin-bottom:4px" />
          <p class="ai-empty-title">OweeCode AI Assistant</p>
          <p class="ai-empty-sub">{{ currentProvider.icon }} {{ currentProvider.label }} · {{ model }}</p>

          <div v-if="hasFile" class="ai-quick-actions">
            <p class="ai-quick-label">{{ t('quickActionsFor') }} <strong>{{ activeTab?.name }}</strong></p>
            <button class="qa-btn" @click="quickAsk('Explica este código en detalle')">📖 {{ t('explainCode') }}</button>
            <button class="qa-btn" @click="quickAsk('Encuentra bugs y problemas en este código')">🐛 {{ t('findBugs') }}</button>
            <button class="qa-btn" @click="quickAsk('Refactoriza y mejora este código')">⚡ {{ t('refactorCode') }}</button>
            <button class="qa-btn" @click="quickAsk('Escribe pruebas unitarias para este código')">🧪 {{ t('generateTests') }}</button>
            <button class="qa-btn" @click="quickAsk('Agrega comentarios JSDoc/PHPDoc a este código')">📝 {{ t('documentCode') }}</button>
            <button class="qa-btn" @click="quickAsk('Optimiza el rendimiento de este código')">🚀 {{ t('optimizeCode') }}</button>
          </div>
          <div v-else class="ai-quick-actions">
            <p class="ai-quick-label">{{ t('suggestions') }}</p>
            <button class="qa-btn" @click="quickAsk('¿Cómo crear una API REST en PHP?', false)">PHP API REST</button>
            <button class="qa-btn" @click="quickAsk('¿Cómo hacer queries complejos en SQL?', false)">SQL Queries</button>
            <button class="qa-btn" @click="quickAsk('Explícame Vue 3 Composition API', false)">Vue 3 Composition</button>
            <button class="qa-btn" @click="quickAsk('¿Cómo funciona async/await en JS?', false)">Async/Await JS</button>
          </div>
        </div>

        <!-- Messages list -->
        <div v-for="(msg, i) in messages" :key="i" class="ai-msg" :class="msg.role === 'user' ? 'ai-msg--user' : 'ai-msg--bot'">
          <!-- User message -->
          <template v-if="msg.role === 'user'">
            <div class="ai-msg-bubble ai-msg-bubble--user">
              <div v-if="msg.context" class="ai-msg-ctx">{{ msg.context }}</div>
              <div class="ai-user-text">{{ msg.content }}</div>
            </div>
          </template>

          <!-- Assistant message -->
          <template v-else>
            <div class="ai-msg-avatar">
              <span :style="{ color: currentProvider.color }">{{ currentProvider.icon }}</span>
            </div>
            <div class="ai-msg-body">
              <div v-html="renderContent(msg.content)" class="ai-rendered" />
              <!-- Action buttons for assistant messages with code -->
              <div v-if="msg.content.includes('```')" class="ai-msg-actions">
                <button class="ai-action-btn ai-action-apply" @click="applyCode(msg.content, i)" :title="t('applyToFileTitle')">
                  {{ t('applyBtnShort') }}
                </button>
                <button class="ai-action-btn" @click="insertCode(msg.content)" :title="t('insertAtEndTitle')">
                  {{ t('insertBtnShort') }}
                </button>
                <button v-if="hasRunnable(msg.content)" class="ai-action-btn ai-action-run" @click="runInTerminal(msg.content)" :title="t('runInTerminalTitle')">
                  {{ t('runBtnShort') }}
                </button>
              </div>
            </div>
          </template>
        </div>

        <!-- Loading -->
        <div v-if="isLoading" class="ai-msg ai-msg--bot">
          <div class="ai-msg-avatar"><span :style="{ color: currentProvider.color }">{{ currentProvider.icon }}</span></div>
          <div class="ai-typing"><span /><span /><span /></div>
        </div>
    </div>

    <!-- Apply confirmation modal -->
    <Teleport to="body">
      <div v-if="applyTarget" class="ai-apply-overlay" @click.self="applyTarget = null">
        <div class="ai-apply-modal">
          <div class="ai-apply-title">{{ t('applyCodeToFileTitle') }}</div>
          <div class="ai-apply-file">📄 {{ activeTab?.name ?? t('activeFileFallback') }}</div>
          <div class="ai-apply-actions">
            <button v-if="hasSelection" class="ai-apply-btn ai-apply-btn--sel" @click="confirmApply(false)">
              {{ t('replaceSelectionBtn') }}
            </button>
            <button class="ai-apply-btn ai-apply-btn--all" @click="confirmApply(true)">
              {{ t('replaceWholeFileBtn') }}
            </button>
            <button class="ai-apply-btn ai-apply-btn--cancel" @click="applyTarget = null">{{ t('cancel') }}</button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Input area -->
    <div class="ai-input-area">
      <div v-if="error && !isLoading" class="ai-error">{{ error }}</div>
      <div v-if="contextStatus" class="ai-context-status">✓ {{ contextStatus }}</div>

      <!-- Context indicator in input -->
      <div v-if="hasSelection" class="ai-input-ctx">
        <svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor"><path d="M4 0h5.293A1 1 0 0 1 10 .293L13.707 4a1 1 0 0 1 .293.707V14a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V2a2 2 0 0 1 2-2zm5.5 1.5v2a1 1 0 0 0 1 1h2L9.5 1.5z"/></svg>
        {{ t('usingSelectionOf') }} {{ activeTab?.name }}
      </div>
      <div v-else-if="hasFile" class="ai-input-ctx">
        <svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor"><path d="M4 0h5.293A1 1 0 0 1 10 .293L13.707 4a1 1 0 0 1 .293.707V14a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V2a2 2 0 0 1 2-2zm5.5 1.5v2a1 1 0 0 0 1 1h2L9.5 1.5z"/></svg>
        {{ t('contextLabel2') }}: {{ activeTab?.name }} ({{ activeTab?.language }})
      </div>

      <div class="ai-input-row">
        <textarea
          v-model="input"
          class="ai-textarea"
          :placeholder="hasSelection ? t('inputPlaceholderSelection') : t('inputPlaceholderCode')"
          rows="2"
          @keydown="onKeydown"
        />
        <button class="ai-send-btn" :disabled="isLoading || !input.trim()" @click="sendMessage()" :style="{ background: currentProvider.color }">
          <svg v-if="!isLoading" width="15" height="15" viewBox="0 0 16 16" fill="currentColor"><path d="M15.854.146a.5.5 0 01.11.54l-5.819 14.547a.75.75 0 01-1.329.124l-3.178-4.995L.643 7.184a.75.75 0 01.124-1.33L15.314.037a.5.5 0 01.54.11z"/></svg>
          <span v-else class="send-spinner" />
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ai-panel {
  display: flex; flex-direction: column; height: 100%;
  background: var(--bg-dark); font-family: var(--font-ui); overflow: hidden;
}

/* Header */
.ai-header {
  display: flex; align-items: center; justify-content: space-between;
  padding: 8px 10px; border-bottom: 1px solid var(--border);
  flex-shrink: 0; background: var(--bg-darkest);
}
.ai-header-left { display: flex; align-items: center; gap: 7px; min-width: 0; }
.ai-title { font-size: 12px; font-weight: 700; color: var(--fg-bright); white-space: nowrap; }
.ai-badge {
  font-size: 10px; font-weight: 600; border-radius: 10px;
  border: 1px solid; padding: 1px 7px; white-space: nowrap;
}
.ai-header-right { display: flex; gap: 2px; flex-shrink: 0; }
.ai-hdr-btn {
  width: 26px; height: 26px; background: none; border: none;
  border-radius: 5px; color: var(--fg-muted); cursor: pointer;
  display: flex; align-items: center; justify-content: center;
}
.ai-hdr-btn:hover { background: var(--bg-hover); color: var(--fg); }
.ai-hdr-btn:disabled { opacity: 0.35; cursor: default; }
.ai-hdr-btn:disabled:hover { background: none; color: var(--fg-muted); }

/* Context bar */
.ai-context-bar {
  display: flex; align-items: center; gap: 5px;
  padding: 4px 10px; background: rgba(82,139,255,0.06);
  border-bottom: 1px solid var(--border); flex-shrink: 0;
}
.ai-context-label { font-size: 10.5px; color: var(--fg-muted); flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.ai-selection-chip {
  font-size: 9.5px; background: rgba(82,139,255,0.2); color: #89b4fa;
  border: 1px solid rgba(82,139,255,0.3); border-radius: 8px; padding: 1px 6px; white-space: nowrap;
}

/* Settings */
.ai-settings {
  background: var(--bg-mid); border-bottom: 1px solid var(--border);
  padding: 12px 14px; flex-shrink: 0; max-height: 60vh; overflow-y: auto;
}
.settings-title-row { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }
.settings-title { font-size: 12px; font-weight: 700; color: var(--fg); }
.settings-close { background: none; border: none; color: var(--fg-muted); cursor: pointer; font-size: 13px; padding: 2px 6px; border-radius: 4px; }
.settings-close:hover { background: var(--bg-hover); color: var(--fg); }
.settings-field { margin-bottom: 10px; }
.settings-field label { display: block; font-size: 11px; color: var(--fg-muted); margin-bottom: 4px; }
.provider-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 5px; }
.provider-opt {
  border: 1px solid var(--border); border-radius: 7px;
  padding: 6px 8px; cursor: pointer;
  display: flex; align-items: center; gap: 5px;
  transition: all 0.12s; color: var(--fg-dim); font-size: 12px;
}
.provider-opt:hover { background: var(--bg-hover); }
.provider-icon { font-size: 13px; }
.provider-name { font-size: 11px; font-weight: 600; }
.ollama-row { display: flex; gap: 5px; }
.detect-btn {
  background: var(--bg-hover); border: 1px solid var(--border);
  border-radius: 5px; color: var(--accent); font-size: 14px;
  padding: 4px 10px; cursor: pointer;
}
.detect-btn:disabled { opacity: 0.5; }
.settings-select, .settings-input {
  width: 100%; background: var(--bg-darkest); border: 1px solid var(--border);
  border-radius: 5px; color: var(--fg); font-size: 12px; padding: 5px 8px; outline: none;
}
.settings-select:focus, .settings-input:focus { border-color: var(--accent); }
.settings-link { display: inline-block; margin-top: 4px; font-size: 10.5px; color: var(--accent); text-decoration: none; }
.settings-link:hover { text-decoration: underline; }
.settings-note { font-size: 10.5px; color: var(--fg-muted); margin: 8px 0; line-height: 1.5; }
.note-warn { border-left: 2px solid #d97706; padding-left: 8px; }
.settings-err { font-size: 10px; color: #f85149; margin-top: 3px; }
.settings-save {
  background: var(--accent); border: none; border-radius: 6px;
  color: var(--accent-fg); font-size: 12px; font-weight: 600;
  padding: 6px 16px; cursor: pointer; width: 100%; margin-top: 6px;
}
.settings-save:hover { opacity: 0.85; }

/* Claude CLI box */
.claude-cli-box {
  background: rgba(217, 119, 6, 0.08); border: 1px solid rgba(217,119,6,0.3);
  border-radius: 8px; padding: 10px 12px; margin: 10px 0;
}
.cli-box-title { font-size: 12px; font-weight: 700; color: #d97706; margin-bottom: 4px; }
.cli-box-desc { font-size: 10.5px; color: var(--fg-muted); margin-bottom: 8px; line-height: 1.4; }
.cli-launch-btn {
  background: #d97706; border: none; border-radius: 6px;
  color: #fff; font-size: 11.5px; font-weight: 600;
  padding: 5px 12px; cursor: pointer; width: 100%;
}
.cli-launch-btn:hover { opacity: 0.85; }

/* Messages */
.ai-messages {
  flex: 1; overflow-y: auto; padding: 10px 0;
  display: flex; flex-direction: column; gap: 1px;
}
.ai-messages::-webkit-scrollbar { width: 3px; }
.ai-messages::-webkit-scrollbar-thumb { background: var(--bg-active); border-radius: 2px; }

/* Empty state */
.ai-empty {
  flex: 1; display: flex; flex-direction: column;
  align-items: center; padding: 20px 12px; text-align: center; gap: 4px;
}
.ai-empty-title { font-size: 13px; font-weight: 700; color: var(--fg-bright); margin-top: 4px; }
.ai-empty-sub { font-size: 10.5px; color: var(--fg-muted); margin-bottom: 12px; }
.ai-quick-actions { width: 100%; display: flex; flex-direction: column; gap: 4px; }
.ai-quick-label { font-size: 10.5px; color: var(--fg-muted); text-align: left; margin-bottom: 2px; }
.qa-btn {
  background: var(--bg-mid); border: 1px solid var(--border);
  color: var(--fg-dim); border-radius: 6px; padding: 7px 10px;
  font-size: 11.5px; cursor: pointer; text-align: left;
  transition: all 0.12s;
}
.qa-btn:hover { background: var(--bg-hover); color: var(--fg); border-color: var(--accent); }

/* Messages */
.ai-msg { display: flex; gap: 8px; padding: 5px 12px; }
.ai-msg--user { justify-content: flex-end; }
.ai-msg--bot { align-items: flex-start; }

.ai-msg-bubble--user { max-width: 88%; }
.ai-msg-ctx {
  font-size: 9.5px; color: var(--fg-muted);
  background: rgba(82,139,255,0.1); border-radius: 6px 6px 0 0;
  padding: 3px 8px; border: 1px solid rgba(82,139,255,0.15); border-bottom: none;
}
.ai-user-text {
  background: var(--accent); color: var(--accent-fg);
  border-radius: 12px 2px 12px 12px;
  padding: 8px 12px; font-size: 12.5px; line-height: 1.5; word-break: break-word;
}
.ai-msg-ctx + .ai-user-text { border-radius: 0 2px 12px 12px; }

.ai-msg-avatar {
  width: 24px; height: 24px; border-radius: 50%;
  background: var(--bg-active); display: flex; align-items: center;
  justify-content: center; font-size: 13px; flex-shrink: 0; margin-top: 2px;
}
.ai-msg-body { flex: 1; min-width: 0; }
.ai-rendered { font-size: 12.5px; color: var(--fg); line-height: 1.6; word-break: break-word; }

.ai-msg-actions { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 6px; }
.ai-action-btn {
  background: var(--bg-hover); border: 1px solid var(--border);
  border-radius: 5px; color: var(--fg-dim); font-size: 10.5px;
  padding: 3px 9px; cursor: pointer; transition: all 0.12s;
}
.ai-action-btn:hover { background: var(--bg-active); color: var(--fg); }
.ai-action-apply { color: var(--accent); border-color: rgba(82,139,255,0.3); background: rgba(82,139,255,0.08); }
.ai-action-apply:hover { background: rgba(82,139,255,0.15); }
.ai-action-run { color: #a6e3a1; border-color: rgba(166,227,161,0.3); background: rgba(166,227,161,0.08); }
.ai-action-run:hover { background: rgba(166,227,161,0.15); }

.ai-typing { display: flex; gap: 4px; padding: 8px 0; align-items: center; }
.ai-typing span { width: 6px; height: 6px; border-radius: 50%; background: var(--fg-muted); animation: blink 1.2s infinite; }
.ai-typing span:nth-child(2) { animation-delay: .2s; }
.ai-typing span:nth-child(3) { animation-delay: .4s; }
@keyframes blink { 0%,60%,100%{opacity:.2} 30%{opacity:1} }

/* Apply modal */
.ai-apply-overlay {
  position: fixed; inset: 0; background: rgba(0,0,0,0.5);
  display: flex; align-items: center; justify-content: center; z-index: 9999;
}
.ai-apply-modal {
  background: var(--bg-mid); border: 1px solid var(--border);
  border-radius: 10px; padding: 20px; min-width: 300px; max-width: 400px;
}
.ai-apply-title { font-size: 14px; font-weight: 700; color: var(--fg-bright); margin-bottom: 8px; }
.ai-apply-file { font-size: 11.5px; color: var(--fg-muted); margin-bottom: 16px; }
.ai-apply-actions { display: flex; flex-direction: column; gap: 6px; }
.ai-apply-btn { border: none; border-radius: 7px; padding: 9px 14px; font-size: 12.5px; font-weight: 600; cursor: pointer; }
.ai-apply-btn--sel { background: var(--accent); color: var(--accent-fg); }
.ai-apply-btn--all { background: var(--bg-hover); border: 1px solid var(--border); color: var(--fg); }
.ai-apply-btn--cancel { background: none; border: 1px solid var(--border); color: var(--fg-muted); }
.ai-apply-btn:hover { opacity: 0.85; }

/* Input */
.ai-input-area { border-top: 1px solid var(--border); padding: 8px; flex-shrink: 0; background: var(--bg-darkest); }
.ai-error { font-size: 11px; color: #f85149; padding: 0 2px 5px; }
.ai-context-status { font-size: 11px; color: #3fb950; padding: 0 2px 5px; }
.mini-spin { width:12px; height:12px; border:2px solid rgba(208,208,216,0.3); border-top-color:currentColor; border-radius:50%; animation:ai-spin 0.7s linear infinite; display:inline-block; }
@keyframes ai-spin { to { transform:rotate(360deg); } }
.ai-input-ctx {
  display: flex; align-items: center; gap: 4px;
  font-size: 10px; color: var(--fg-muted); padding: 0 2px 5px;
}
.ai-input-row { display: flex; gap: 6px; align-items: flex-end; }
.ai-textarea {
  flex: 1; background: var(--bg-mid); border: 1px solid var(--border);
  border-radius: 8px; color: var(--fg); font-size: 12.5px; line-height: 1.5;
  padding: 7px 10px; resize: none; outline: none;
  font-family: var(--font-ui); min-height: 36px; transition: border-color 0.12s;
}
.ai-textarea:focus { border-color: var(--accent); }
.ai-send-btn {
  width: 34px; height: 34px; border: none; border-radius: 8px;
  color: #fff; cursor: pointer; flex-shrink: 0;
  display: flex; align-items: center; justify-content: center; transition: opacity 0.12s;
}
.ai-send-btn:hover { opacity: 0.85; }
.ai-send-btn:disabled { opacity: 0.4; cursor: default; }
.send-spinner {
  width: 13px; height: 13px; border: 2px solid rgba(255,255,255,.3);
  border-top-color: #fff; border-radius: 50%; animation: spin .7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

.slide-enter-active, .slide-leave-active { transition: all 0.18s ease; }
.slide-enter-from, .slide-leave-to { opacity: 0; transform: translateY(-6px); }
</style>

<style>
.ai-rendered .ai-code-block {
  background: #0d1117; border: 1px solid #30363d;
  border-radius: 8px; margin: 8px 0; overflow-x: auto;
  font-family: 'JetBrains Mono', 'Fira Code', Consolas, monospace;
  font-size: 12px; line-height: 1.5;
}
.ai-rendered .ai-code-lang {
  font-size: 10px; color: #6e7681; padding: 4px 12px 0;
  font-family: var(--font-ui); text-transform: uppercase; letter-spacing: .05em;
}
.ai-rendered .ai-code-block code { display: block; padding: 6px 12px 10px; color: #e6edf3; white-space: pre; }
.ai-rendered .ai-inline-code {
  background: rgba(82,139,255,.1); border: 1px solid rgba(82,139,255,.2);
  border-radius: 3px; padding: 1px 5px;
  font-family: 'JetBrains Mono', Consolas, monospace; font-size: 11.5px; color: #89b4fa;
}
.ai-rendered .ai-heading { font-weight: 700; color: var(--fg-bright); margin: 6px 0 2px; font-size: 13px; }
</style>
