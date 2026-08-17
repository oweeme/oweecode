<script setup lang="ts">
// OweeCode's local coding agent — a workspace tab (like Claude Code/Aider)
// instead of a sidebar panel, so it behaves the same way as those: keeps
// working in the background while the user looks at other tabs, and they
// switch back to check progress instead of it competing for space with chat.
import { ref, onMounted, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useEditorStore } from '../composables/useEditorStore'
import { useAgentStore } from '../composables/useAgentStore'
import { useProjectMemory } from '../composables/useProjectMemory'
import { renderContent } from '../composables/aiMarkdown'
import AgentToolCallCard from './AgentToolCallCard.vue'
import OllamaSetup from './OllamaSetup.vue'

const store = useEditorStore()
const agent = useAgentStore()
const { loadProjectContext } = useProjectMemory()

const RECOMMENDED_MODELS = ['qwen2.5-coder:7b-instruct-q4_K_M', 'qwen2.5-coder:3b-instruct-q4_K_M']
const AGENT_MODEL_KEY = 'agent_model'

const input = ref('')
const messagesEl = ref<HTMLElement | null>(null)
const showOllamaSetup = ref(false)
const models = ref<string[]>([])
const model = ref(localStorage.getItem(AGENT_MODEL_KEY) ?? RECOMMENDED_MODELS[0])

async function loadModels() {
  try {
    models.value = await invoke<string[]>('list_ollama_models')
    if (!models.value.includes(model.value)) {
      model.value = models.value.find(m => RECOMMENDED_MODELS.includes(m)) ?? models.value[0] ?? model.value
    }
  } catch { /* Ollama not running — surfaced by the send button being effectively unusable */ }
}

function onModelChange() {
  localStorage.setItem(AGENT_MODEL_KEY, model.value)
}

async function scrollDown() {
  await nextTick()
  if (messagesEl.value) messagesEl.value.scrollTop = messagesEl.value.scrollHeight
}

async function send() {
  const text = input.value.trim()
  if (!text || agent.state.running) return
  input.value = ''
  await scrollDown()
  await agent.runAgentTask(text, { rootPath: store.state.rootPath, model: model.value })
  await scrollDown()
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send() }
}

onMounted(async () => {
  await loadModels()
  await loadProjectContext(store.state.rootPath)
})
</script>

<template>
  <div class="agent-tab">
    <div class="agent-header">
      <div class="agent-header-left">
        <span class="agent-icon">⬡</span>
        <span class="agent-title">Agente (Ollama)</span>
        <select v-model="model" class="agent-model-select" @change="onModelChange">
          <option v-if="models.length === 0" :value="model">{{ model }}</option>
          <option v-for="m in models" :key="m" :value="m">{{ m }}</option>
        </select>
      </div>
      <div class="agent-header-right">
        <button class="agent-hdr-btn" @click="showOllamaSetup = true" title="Instalar Ollama / descargar modelos">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M8 1a1 1 0 011 1v1.07A6.002 6.002 0 0113.93 8H15a1 1 0 110 2h-1.07A6.002 6.002 0 019 13.93V15a1 1 0 11-2 0v-1.07A6.002 6.002 0 012.07 10H1a1 1 0 110-2h1.07A6.002 6.002 0 017 3.07V2a1 1 0 011-1zm0 3a4 4 0 100 8 4 4 0 000-8z"/></svg>
        </button>
        <button class="agent-hdr-btn" @click="agent.clearAgentChat" title="Limpiar conversación">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M5.5 5.5A.5.5 0 016 6v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm2.5 0a.5.5 0 01.5.5v6a.5.5 0 01-1 0V6a.5.5 0 01.5-.5zm3 .5a.5.5 0 00-1 0v6a.5.5 0 001 0V6z"/><path fill-rule="evenodd" d="M14.5 3a1 1 0 01-1 1H13v9a2 2 0 01-2 2H5a2 2 0 01-2-2V4h-.5a1 1 0 01-1-1V2a1 1 0 011-1H6a1 1 0 011-1h2a1 1 0 011 1h3.5a1 1 0 011 1v1zM4.118 4L4 4.059V13a1 1 0 001 1h6a1 1 0 001-1V4.059L11.882 4H4.118zM2.5 3V2h11v1h-11z"/></svg>
        </button>
      </div>
    </div>

    <div ref="messagesEl" class="agent-messages">
      <div v-if="agent.state.steps.length === 0" class="agent-empty">
        <img src="/oweedev.png" width="40" height="40" style="border-radius:10px;opacity:.7;margin-bottom:4px" />
        <p class="agent-empty-title">Agente de OweeCode</p>
        <p class="agent-empty-sub">⬡ Ollama · {{ model }}</p>
        <p class="agent-empty-desc">
          Lee y escribe archivos, corre comandos y usa git en este proyecto. Toda acción que modifique algo pide tu aprobación primero.
        </p>
      </div>

      <div v-for="(step, i) in agent.state.steps" :key="i" class="agent-msg" :class="step.role === 'user' ? 'agent-msg--user' : 'agent-msg--bot'">
        <template v-if="step.role === 'user'">
          <div class="agent-user-text">{{ step.content }}</div>
        </template>
        <template v-else-if="step.role === 'tool'">
          <div class="agent-msg-avatar">⚙</div>
          <div class="agent-msg-body">
            <div class="agent-tool-result">
              <div class="agent-tool-result-name">{{ step.toolName }}</div>
              <pre>{{ step.content.slice(0, 2000) }}</pre>
            </div>
          </div>
        </template>
        <template v-else>
          <div class="agent-msg-avatar">⬡</div>
          <div class="agent-msg-body">
            <div v-html="renderContent(step.content)" class="agent-rendered" />
          </div>
        </template>
      </div>

      <AgentToolCallCard
        v-if="agent.state.pendingApproval"
        :call="agent.state.pendingApproval"
        @approve="agent.approveToolCall"
        @reject="agent.rejectToolCall"
      />

      <div v-if="agent.state.running && !agent.state.pendingApproval" class="agent-msg agent-msg--bot">
        <div class="agent-msg-avatar">⬡</div>
        <div class="agent-typing"><span /><span /><span /></div>
      </div>
    </div>

    <div class="agent-input-area">
      <div v-if="agent.state.error" class="agent-error">{{ agent.state.error }}</div>
      <div class="agent-input-row">
        <textarea
          v-model="input"
          class="agent-textarea"
          placeholder="Pedile una tarea al agente (leer, escribir, correr algo)…"
          rows="2"
          :disabled="agent.state.running"
          @keydown="onKeydown"
        />
        <button class="agent-send-btn" :disabled="agent.state.running || !input.trim()" @click="send">
          <svg v-if="!agent.state.running" width="15" height="15" viewBox="0 0 16 16" fill="currentColor"><path d="M15.854.146a.5.5 0 01.11.54l-5.819 14.547a.75.75 0 01-1.329.124l-3.178-4.995L.643 7.184a.75.75 0 01.124-1.33L15.314.037a.5.5 0 01.54.11z"/></svg>
          <span v-else class="agent-send-spinner" />
        </button>
      </div>
    </div>

    <OllamaSetup v-if="showOllamaSetup" @close="showOllamaSetup = false; loadModels()" />
  </div>
</template>

<style scoped>
.agent-tab {
  display: flex; flex-direction: column; height: 100%;
  background: var(--bg-dark); font-family: var(--font-ui); overflow: hidden;
}

.agent-header {
  display: flex; align-items: center; justify-content: space-between;
  padding: 8px 10px; border-bottom: 1px solid var(--border);
  flex-shrink: 0; background: var(--bg-darkest);
}
.agent-header-left { display: flex; align-items: center; gap: 8px; min-width: 0; }
.agent-icon { color: #a6e3a1; font-size: 15px; }
.agent-title { font-size: 12px; font-weight: 700; color: var(--fg-bright); white-space: nowrap; }
.agent-model-select {
  background: var(--bg-hover); border: 1px solid var(--border);
  border-radius: 5px; color: #a6e3a1; font-size: 10.5px; font-family: var(--font-mono);
  padding: 3px 6px; max-width: 220px;
}
.agent-header-right { display: flex; gap: 2px; flex-shrink: 0; }
.agent-hdr-btn {
  width: 26px; height: 26px; background: none; border: none;
  border-radius: 5px; color: var(--fg-muted); cursor: pointer;
  display: flex; align-items: center; justify-content: center;
}
.agent-hdr-btn:hover { background: var(--bg-hover); color: var(--fg); }

.agent-messages { flex: 1; overflow-y: auto; padding: 10px 0; display: flex; flex-direction: column; gap: 1px; }
.agent-messages::-webkit-scrollbar { width: 3px; }
.agent-messages::-webkit-scrollbar-thumb { background: var(--bg-active); border-radius: 2px; }

.agent-empty { flex: 1; display: flex; flex-direction: column; align-items: center; padding: 20px 12px; text-align: center; gap: 4px; }
.agent-empty-title { font-size: 13px; font-weight: 700; color: var(--fg-bright); margin-top: 4px; }
.agent-empty-sub { font-size: 10.5px; color: var(--fg-muted); margin-bottom: 8px; }
.agent-empty-desc { font-size: 10.5px; color: var(--fg-muted); max-width: 260px; line-height: 1.5; }

.agent-msg { display: flex; gap: 8px; padding: 5px 12px; }
.agent-msg--user { justify-content: flex-end; }
.agent-msg--bot { align-items: flex-start; }
.agent-user-text {
  background: var(--accent); color: var(--accent-fg); max-width: 88%;
  border-radius: 12px 2px 12px 12px; padding: 8px 12px; font-size: 12.5px; line-height: 1.5; word-break: break-word;
}
.agent-msg-avatar {
  width: 24px; height: 24px; border-radius: 50%; background: var(--bg-active);
  display: flex; align-items: center; justify-content: center; font-size: 13px; flex-shrink: 0; margin-top: 2px;
}
.agent-msg-body { flex: 1; min-width: 0; }
.agent-rendered { font-size: 12.5px; color: var(--fg); line-height: 1.6; word-break: break-word; }

.agent-tool-result { background: var(--bg-darker); border: 1px solid var(--border); border-radius: 6px; padding: 6px 8px; font-family: var(--font-mono); font-size: 10.5px; }
.agent-tool-result-name { font-weight: 700; color: #a6e3a1; margin-bottom: 3px; }
.agent-tool-result pre { white-space: pre-wrap; word-break: break-word; color: var(--fg-muted); margin: 0; max-height: 220px; overflow-y: auto; }

.agent-typing { display: flex; gap: 4px; padding: 8px 0; align-items: center; }
.agent-typing span { width: 6px; height: 6px; border-radius: 50%; background: var(--fg-muted); animation: agent-blink 1.2s infinite; }
.agent-typing span:nth-child(2) { animation-delay: .2s; }
.agent-typing span:nth-child(3) { animation-delay: .4s; }
@keyframes agent-blink { 0%,60%,100%{opacity:.2} 30%{opacity:1} }

.agent-input-area { border-top: 1px solid var(--border); padding: 8px; flex-shrink: 0; background: var(--bg-darkest); }
.agent-error { font-size: 11px; color: #f85149; padding: 0 2px 5px; }
.agent-input-row { display: flex; gap: 6px; align-items: flex-end; }
.agent-textarea {
  flex: 1; background: var(--bg-mid); border: 1px solid var(--border);
  border-radius: 8px; color: var(--fg); font-size: 12.5px; line-height: 1.5;
  padding: 7px 10px; resize: none; outline: none;
  font-family: var(--font-ui); min-height: 36px; transition: border-color 0.12s;
}
.agent-textarea:focus { border-color: #a6e3a1; }
.agent-send-btn {
  width: 34px; height: 34px; border: none; border-radius: 8px; background: #a6e3a1;
  color: #0d1117; cursor: pointer; flex-shrink: 0;
  display: flex; align-items: center; justify-content: center; transition: opacity 0.12s;
}
.agent-send-btn:hover { opacity: 0.85; }
.agent-send-btn:disabled { opacity: 0.4; cursor: default; }
.agent-send-spinner {
  width: 13px; height: 13px; border: 2px solid rgba(13,17,23,.3);
  border-top-color: #0d1117; border-radius: 50%; animation: agent-spin .7s linear infinite;
}
@keyframes agent-spin { to { transform: rotate(360deg); } }
</style>
