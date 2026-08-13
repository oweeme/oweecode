import { invoke } from '@tauri-apps/api/core'

export interface AiCliDef {
  id: string
  label: string
  icon: string
  color: string
  command: string
  installCmd: string
  installCmdWindows?: string // used instead of installCmd when running on Windows
  tip: string
}

// Registry of AI coding CLIs that can be launched as an embedded terminal tab.
// Add new entries here — CliPanel, App.vue's CLI picker, and the tab system all
// read from this single list.
export const AI_CLIS: AiCliDef[] = [
  {
    id: 'claude', label: 'Claude Code', icon: '◆', color: '#d97706',
    command: 'claude', installCmd: 'npm install -g @anthropic-ai/claude-code',
    tip: 'Usa tu suscripción Claude Pro sin API key adicional',
  },
  {
    id: 'copilot', label: 'GitHub Copilot CLI', icon: '◎', color: '#8957e5',
    command: 'copilot', installCmd: 'npm install -g @github/copilot',
    tip: 'Requiere una cuenta de GitHub con Copilot (hay plan gratuito limitado)',
  },
  {
    id: 'aider', label: 'Aider', icon: '✦', color: '#f5a623',
    command: 'aider', installCmd: 'curl -LsSf https://aider.chat/install.sh | sh',
    installCmdWindows: 'python -m pip install aider-install; aider-install',
    tip: 'Open source, usa automáticamente el proveedor configurado en "Configuración AI" (incluye Ollama y OpenRouter)',
  },
]

export function cliMeta(id: string): AiCliDef {
  return AI_CLIS.find(c => c.id === id) ?? {
    id, label: id, icon: '◇', color: '#8b949e', command: id, installCmd: '', tip: '',
  }
}

// OpenRouter's free-tier catalog changes over time — a model that was free last
// week can lose that status, breaking whatever has its id saved (the chat panel,
// Aider's launch env). Both call this before actually using the saved model, so
// a stale id gets silently swapped for a currently-free one instead of erroring.
// Takes the localStorage key names since the chat panel and Aider's own
// independent model picker each keep their own provider/model selection.
export async function ensureFreshOpenrouterModel(providerKey = 'ai_provider', modelKey = 'ai_model'): Promise<void> {
  if (localStorage.getItem(providerKey) !== 'openrouter') return
  try {
    const models = await invoke<{ id: string; name: string }[]>('list_openrouter_free_models')
    if (!models.length) return
    const current = localStorage.getItem(modelKey) ?? ''
    if (!models.some(m => m.id === current)) {
      localStorage.setItem(modelKey, models[0].id)
    }
  } catch { /* best-effort — leave the saved model as-is if the check itself fails */ }
}
