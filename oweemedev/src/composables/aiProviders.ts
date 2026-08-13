// Shared catalog of AI chat providers + per-provider API key storage. Used by
// AiPanel.vue (chat) and CliPanel.vue (Aider's own independent model picker)
// so both pick from the same list and the same saved key — one key per
// provider, reused everywhere that provider is selected, so setting it once
// in either place is enough.
export type Provider = 'deepseek' | 'claude' | 'openai' | 'gemini' | 'ollama' | 'groq' | 'openrouter'

export interface ProviderInfo {
  label: string; badge: string; color: string; icon: string
  keyLabel: string; keyPlaceholder: string; keyUrl: string; keyNote: string
  models: { id: string; label: string }[]
}

export const AI_PROVIDERS: Record<Provider, ProviderInfo> = {
  claude: {
    label: 'Claude', badge: 'Claude', color: '#d97706', icon: '◆',
    keyLabel: 'API Key — console.anthropic.com',
    keyPlaceholder: 'sk-ant-...',
    keyUrl: 'https://console.anthropic.com/settings/keys',
    keyNote: 'Tu suscripción Claude.ai Pro NO incluye API. Son cuentas separadas. Sin API key: escribe "claude" en la terminal del IDE para usar tu plan Pro directamente.',
    models: [
      { id: 'claude-sonnet-4-6',         label: 'Claude Sonnet 4.6 (Recomendado)' },
      { id: 'claude-haiku-4-5-20251001',  label: 'Claude Haiku 4.5 (Rápido, económico)' },
      { id: 'claude-opus-4-8',            label: 'Claude Opus 4.8 (Más potente)' },
    ],
  },
  deepseek: {
    label: 'DeepSeek', badge: 'DeepSeek', color: '#4d9de0', icon: '◈',
    keyLabel: 'API Key — platform.deepseek.com',
    keyPlaceholder: 'sk-...',
    keyUrl: 'https://platform.deepseek.com/api_keys',
    keyNote: 'API muy económica. $0.14/M tokens para DeepSeek-Coder. Ideal para código.',
    models: [
      { id: 'deepseek-coder',    label: 'DeepSeek Coder (Código)' },
      { id: 'deepseek-chat',     label: 'DeepSeek Chat (General)' },
      { id: 'deepseek-reasoner', label: 'DeepSeek Reasoner' },
    ],
  },
  gemini: {
    label: 'Gemini', badge: 'Gemini', color: '#4285f4', icon: '◈',
    keyLabel: 'API Key — aistudio.google.com',
    keyPlaceholder: 'AIza...',
    keyUrl: 'https://aistudio.google.com/app/apikey',
    keyNote: 'Tu plan Google AI Plus es para la app de Gemini, NO para la API. Ve a aistudio.google.com → Get API key. Es GRATIS por separado.',
    models: [
      { id: 'gemini-2.0-flash',        label: 'Gemini 2.0 Flash (Gratis)' },
      { id: 'gemini-2.0-flash-lite',   label: 'Gemini 2.0 Flash Lite' },
      { id: 'gemini-1.5-pro-latest',   label: 'Gemini 1.5 Pro' },
    ],
  },
  openai: {
    label: 'OpenAI', badge: 'OpenAI', color: '#10a37f', icon: '⬡',
    keyLabel: 'API Key — platform.openai.com',
    keyPlaceholder: 'sk-...',
    keyUrl: 'https://platform.openai.com/api-keys',
    keyNote: 'Requiere cuenta con créditos en platform.openai.com.',
    models: [
      { id: 'gpt-4o',      label: 'GPT-4o (Recomendado)' },
      { id: 'gpt-4o-mini', label: 'GPT-4o Mini (Rápido)' },
    ],
  },
  ollama: {
    label: 'Ollama (Local)', badge: 'Ollama', color: '#a6e3a1', icon: '⬡',
    keyLabel: 'Sin API key — corre localmente',
    keyPlaceholder: '(no requerida)',
    keyUrl: 'https://ollama.com',
    keyNote: 'Instala Ollama y ejecuta: ollama pull llama3. 100% privado y gratis. Para el modo Agente, usá el botón ⚙ de arriba para instalar los modelos recomendados.',
    models: [
      { id: 'qwen2.5-coder:7b-instruct-q4_K_M', label: 'Qwen2.5 Coder 7B (Recomendado — agente/chat)' },
      { id: 'qwen2.5-coder:3b-instruct-q4_K_M', label: 'Qwen2.5 Coder 3B (Rápido)' },
      { id: 'llama3',       label: 'Llama 3' },
      { id: 'codellama',    label: 'CodeLlama' },
      { id: 'deepseek-coder', label: 'DeepSeek Coder' },
      { id: 'mistral',      label: 'Mistral 7B' },
    ],
  },
  groq: {
    label: 'Groq', badge: 'Groq', color: '#f55036', icon: '⚡',
    keyLabel: 'API Key — console.groq.com',
    keyPlaceholder: 'gsk_...',
    keyUrl: 'https://console.groq.com/keys',
    keyNote: 'GRATIS con límites de uso generosos. Inferencia extremadamente rápida (LPU).',
    models: [
      { id: 'llama-3.3-70b-versatile', label: 'Llama 3.3 70B (Recomendado)' },
      { id: 'llama-3.1-8b-instant',    label: 'Llama 3.1 8B (Rápido)' },
      { id: 'gemma2-9b-it',            label: 'Gemma 2 9B' },
      { id: 'mixtral-8x7b-32768',      label: 'Mixtral 8x7B' },
    ],
  },
  openrouter: {
    label: 'OpenRouter', badge: 'OpenRouter', color: '#8b5cf6', icon: '◇',
    keyLabel: 'API Key — openrouter.ai',
    keyPlaceholder: 'sk-or-...',
    keyUrl: 'https://openrouter.ai/keys',
    keyNote: 'La lista de modelos se consulta en vivo — el catálogo gratis de OpenRouter cambia seguido, así que evitamos IDs fijos que dejan de existir.',
    // Placeholder shown only for the instant before a live fetch resolves —
    // the actual free catalog changes over time, so this single entry is
    // just a safe non-empty default, never relied on for real use.
    models: [
      { id: 'openai/gpt-oss-20b:free', label: 'Cargando catálogo gratis…' },
    ],
  },
}

export function loadProviderKey(p: Provider): string {
  if (p === 'ollama') return 'ollama'
  return localStorage.getItem(`ai_key_${p}`) ?? ''
}

export function saveProviderKey(p: Provider, key: string): void {
  if (p === 'ollama') return
  localStorage.setItem(`ai_key_${p}`, key)
}
