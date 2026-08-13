// Shared project memory: a single AGENTS.md at the project root, in the open
// convention that Aider/Cursor/Copilot and others already read — so whatever
// gets saved here isn't locked into OweemeIDE's own chat, it's picked up by
// other AI tools on the same project too. Extracted out of AiPanel.vue so
// both the normal chat (manual save button) and the agent loop (auto-save on
// task completion) share the exact same file/format instead of two competing
// memory systems.
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from './useI18n'

const { t } = useI18n()

const AGENTS_FILE = 'AGENTS.md'
// Keeps the file from growing forever (and from bloating every future system
// prompt) — only the last MAX_CONTEXT_SECTIONS dated summaries are kept.
const MAX_CONTEXT_SECTIONS = 10
const HEADER = `# Contexto del proyecto (AGENTS.md)\n\nResumen compartido para que cualquier asistente de IA (Claude Code, Aider, el chat de OweemeIDE, etc.) continúe con el contexto de este proyecto sin partir de cero.\n`

const projectContext = ref('')
const savingContext = ref(false)
const contextStatus = ref('')
const contextError = ref('')

function agentsFilePath(rootPath: string | null | undefined): string | null {
  return rootPath ? `${rootPath}/${AGENTS_FILE}` : null
}

async function loadProjectContext(rootPath: string | null | undefined) {
  const path = agentsFilePath(rootPath)
  if (!path) { projectContext.value = ''; return }
  try { projectContext.value = await invoke<string>('open_file', { path }) }
  catch { projectContext.value = '' }
}

interface SummarizeParams {
  provider: string
  apiKey: string
  model: string
}

// Summarizes `transcript` via `call_ai` and appends it as a new dated section
// to AGENTS.md. Called both by the manual "save context" button (chat mode)
// and automatically when an agent task finishes.
async function saveProjectContext(rootPath: string | null | undefined, transcript: string, ai: SummarizeParams): Promise<boolean> {
  const path = agentsFilePath(rootPath)
  if (!path) { contextError.value = t('noProjectForContext'); return false }
  if (!transcript.trim()) return false
  savingContext.value = true
  contextError.value = ''
  try {
    const summary = await invoke<string>('call_ai', {
      provider: ai.provider,
      apiKey: ai.apiKey,
      model: ai.model,
      system: 'Resumí la siguiente conversación de asistencia de código en 3 a 6 viñetas breves y concretas: qué se pidió, qué se decidió o cambió, y cualquier dato que una IA distinta necesitaría para continuar este proyecto sin volver a preguntar. Responde solo con las viñetas en markdown, sin introducción ni cierre.',
      messages: [{ role: 'user', content: transcript.slice(-8000) }],
    })
    const timestamp = new Date().toISOString().slice(0, 16).replace('T', ' ')
    const existingSections = projectContext.value
      ? projectContext.value.replace(HEADER, '').split(/\n(?=## )/).filter(s => s.trim())
      : []
    const newSection = `## ${timestamp}\n${summary.trim()}`
    const sections = [...existingSections, newSection].slice(-MAX_CONTEXT_SECTIONS)
    const content = HEADER + '\n' + sections.join('\n\n') + '\n'
    await invoke('save_file', { path, content })
    projectContext.value = content
    contextStatus.value = t('contextSavedPrefix')
    setTimeout(() => { contextStatus.value = '' }, 4000)
    return true
  } catch (e: any) {
    contextError.value = String(e)
    return false
  } finally {
    savingContext.value = false
  }
}

export function useProjectMemory() {
  return { projectContext, savingContext, contextStatus, contextError, agentsFilePath, loadProjectContext, saveProjectContext }
}
