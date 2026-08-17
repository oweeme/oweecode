// Agent mode: a multi-turn tool-calling loop against a local Ollama model.
// Singleton reactive store, same pattern as the other use*Store composables
// (useContainerStore.ts, etc.) — state lives at module scope so it survives
// AiPanel.vue remounting when the user switches sidebar views.
import { reactive } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { AGENT_TOOLS, buildOllamaToolsSchema, findAgentTool, type AgentToolCtx } from './agentTools'
import { useProjectMemory } from './useProjectMemory'

export interface AgentStep {
  role: 'user' | 'assistant' | 'tool'
  content: string
  toolName?: string
}

export interface PendingToolCall {
  id: string
  name: string
  label: string
  arguments: any
}

// Ollama's small quantized models don't always emit well-formed `tool_calls`
// even when `tools` is in the request — cuts the loop rather than spinning
// forever if that keeps happening.
const MAX_AGENT_STEPS = 15

interface AgentChatResponse {
  content: string
  tool_calls: { id: string; name: string; arguments: any }[]
}

const state = reactive({
  running: false,
  steps: [] as AgentStep[],
  pendingApproval: null as PendingToolCall | null,
  error: '',
})

let resumeResolver: ((approved: boolean) => void) | null = null

function approveToolCall() {
  const resolve = resumeResolver
  resumeResolver = null
  state.pendingApproval = null
  resolve?.(true)
}

function rejectToolCall() {
  const resolve = resumeResolver
  resumeResolver = null
  state.pendingApproval = null
  resolve?.(false)
}

function waitForApproval(call: PendingToolCall): Promise<boolean> {
  state.pendingApproval = call
  return new Promise(resolve => { resumeResolver = resolve })
}

function clearAgentChat() {
  state.steps = []
  state.error = ''
}

// qwen2.5-coder:3b/7b-instruct against Ollama 0.32.6 don't actually populate
// `message.tool_calls` even when `tools` is in the request — verified against
// a real local Ollama instance. Instead the whole `content` IS the call, in
// the same {name, arguments} shape as the `tools` schema we sent, either raw
// (7B) or fenced in ```json (3B). This fallback covers both observed shapes;
// `tool_calls` from the native field (if a future Ollama version fills it)
// is still tried first by the caller.
// Scans `text` for complete top-level {...} objects, respecting string
// literals (so a "}" inside a quoted value doesn't end the object early).
// Lets parseFallbackToolCall recognize more than one tool call when a model
// batches several in one response — seen in testing as one JSON object per
// line (network_ensure then create), which a single JSON.parse can't handle
// since the whole content isn't valid JSON on its own.
function extractJsonObjects(text: string): string[] {
  const objects: string[] = []
  let depth = 0, start = -1, inString = false, escape = false
  for (let i = 0; i < text.length; i++) {
    const ch = text[i]
    if (inString) {
      if (escape) escape = false
      else if (ch === '\\') escape = true
      else if (ch === '"') inString = false
      continue
    }
    if (ch === '"') { inString = true; continue }
    if (ch === '{') { if (depth === 0) start = i; depth++ }
    else if (ch === '}') { depth--; if (depth === 0 && start !== -1) { objects.push(text.slice(start, i + 1)); start = -1 } }
  }
  return objects
}

// A model writing multi-line file content into a JSON string argument
// sometimes emits a literal newline/tab instead of the escaped "\n"/"\t" JSON
// requires — technically invalid JSON that JSON.parse rejects outright, even
// though the intent is completely unambiguous. Repairs it by walking the
// text and escaping control characters, but only while inside a "..." string
// (tracked the same way extractJsonObjects does), so structural whitespace
// between tokens is left alone.
function repairJsonControlChars(text: string): string {
  let out = ''
  let inString = false, escape = false
  for (const ch of text) {
    if (inString) {
      if (escape) { out += ch; escape = false; continue }
      if (ch === '\\') { out += ch; escape = true; continue }
      if (ch === '"') { inString = false; out += ch; continue }
      if (ch === '\n') { out += '\\n'; continue }
      if (ch === '\r') { out += '\\r'; continue }
      if (ch === '\t') { out += '\\t'; continue }
      out += ch
      continue
    }
    if (ch === '"') { inString = true; out += ch; continue }
    out += ch
  }
  return out
}

function tryParseJson(raw: string): any {
  try { return JSON.parse(raw) } catch { /* fall through to the repair pass */ }
  try { return JSON.parse(repairJsonControlChars(raw)) } catch { return null }
}

function parseFallbackToolCall(content: string): { id: string; name: string; arguments: any }[] {
  const trimmed = content.trim()
  // No fenced-block pre-extraction here on purpose: a naive ```-to-``` regex
  // stops at the *first* closing fence, which breaks as soon as the file
  // content being written contains its own ``` code block (seen in testing —
  // a markdown file with embedded code fences truncated the JSON mid-string
  // and lost the whole write). extractJsonObjects's brace/quote-aware scan
  // finds the true top-level {...} directly, fenced or not — backticks
  // inside a string are just ordinary characters to it either way.
  const candidates = extractJsonObjects(trimmed)
  if (candidates.length === 0) candidates.push(trimmed)

  const calls: { id: string; name: string; arguments: any }[] = []
  for (const raw of candidates) {
    const obj = tryParseJson(raw)
    // Usually {"name": ..., "arguments": ...}, but this model has also been
    // seen using "function" for the tool name instead of "name" — accept
    // either rather than silently dropping the call and showing the raw
    // JSON to the user as if it were a real answer.
    const toolName = typeof obj?.name === 'string' ? obj.name : (typeof obj?.function === 'string' ? obj.function : null)
    if (obj && toolName) {
      calls.push({ id: `call_fallback_${calls.length}`, name: toolName, arguments: obj.arguments ?? obj.args ?? {} })
    }
  }
  return calls
}

function buildSystemPrompt(agentsContext: string): string {
  const toolList = AGENT_TOOLS.map(t => `- ${t.name}: ${t.description}`).join('\n')
  return `Sos un agente de programación integrado en OweeCode, con acceso real de lectura/escritura al proyecto que el usuario tiene abierto — no es una simulación, cada tool call se ejecuta de verdad.
Tenés estas herramientas disponibles:
${toolList}
Reglas importantes:
- Nunca inventes ni asumas el nombre exacto de un archivo o carpeta. Si no lo sabés con certeza, usá fs_list_dir o fs_search primero para confirmarlo antes de llamar fs_read_file.
- Los paths de fs_read_file/fs_write_file/fs_list_dir/fs_create_dir/fs_search ya son relativos a la raíz del proyecto — NUNCA le agregues el nombre de la carpeta del proyecto adelante (ej. si estás en el proyecto "mioceano", usá "video/archivo.md", nunca "mioceano/video/archivo.md" — eso crea una carpeta "mioceano" duplicada adentro del proyecto).
- Usá solo herramientas de esta lista exacta, con este nombre exacto. Si necesitás algo que no está en la lista, decíselo al usuario en vez de inventar un nombre de herramienta nuevo.
- Cuando el usuario te pida crear un archivo, usá EXACTAMENTE el nombre y la carpeta que te dio — nunca uses "README.md" ni ningún otro nombre genérico salvo que el usuario lo haya pedido así.
- Apenas termines lo que el usuario pidió, contestá y parás ahí — no sigas explorando el resto del proyecto ni analizando archivos que no tienen que ver con el pedido, aunque te parezca útil. Si el usuario quiere algo más, te lo va a pedir en el siguiente mensaje.
- Si fs_read_file falla, el error incluye los archivos reales de esa carpeta — usalos para corregir el nombre en el siguiente intento, no le devuelvas el error crudo al usuario ni le preguntes la ruta exacta salvo que ya lo hayas intentado.
- Cuando el usuario te pida leer, buscar, escribir o ejecutar algo, hacelo con las herramientas en el momento — no describas lo que harías, hacelo.
- Antes de crear un contenedor, llamá container_list para ver si ya existe uno de este proyecto con ese propósito — no dupliques. Para un stack de varios contenedores vinculados (ej. app + base de datos), primero container_network_ensure con un nombre de red, después cada container_create con ese mismo network.
- Si container_create falla con "name already in use" / "ya está en uso", NO llames container_remove como reacción automática — ese nombre ya en uso casi siempre significa que un create anterior tuyo (en este mismo intento) ya tuvo éxito. Primero confirmá con container_list si el contenedor está bien, y contale eso al usuario. Solo eliminá un contenedor si el usuario lo pide explícitamente.
- Cuando un paso falla y vas a reintentar, mantené el mismo nombre de contenedor y de red que usaste antes en esta misma tarea — no inventes nombres nuevos en cada intento, eso deja contenedores/redes sueltos y no soluciona nada. Corregí solo lo que causó el error puntual. Si container_network_ensure o container_image_pull ya te devolvieron éxito antes en esta tarea para el mismo nombre, no los repitas — pasá directo al siguiente paso.
- Cuando resumas el resultado de una herramienta, describí lo que realmente devolvió ESE llamado — no copies ni reformules un error de un mensaje anterior de esta misma conversación, aunque se parezca.
- Para llamar una herramienta, respondé ÚNICAMENTE con un JSON de la forma {"name": "nombre_herramienta", "arguments": {...}}, sin texto alrededor.
- Cuando ya tengas lo que necesitás para responder, contestá en texto plano normal, sin JSON y sin más llamadas a herramientas.${agentsContext ? `\n\nContexto conocido de este proyecto (AGENTS.md):\n${agentsContext}` : ''}`
}

async function runAgentTask(userMessage: string, opts: { rootPath: string; model: string }) {
  if (state.running) return
  state.running = true
  state.error = ''

  const memory = useProjectMemory()
  await memory.loadProjectContext(opts.rootPath)

  const ctx: AgentToolCtx = { rootPath: opts.rootPath }
  const tools = buildOllamaToolsSchema()
  const system = buildSystemPrompt(memory.projectContext.value)

  state.steps.push({ role: 'user', content: userMessage })

  // Tracks how many times each exact (tool, arguments) pair has run across
  // the *whole* task, not just within one model response — the dedupe above
  // only catches immediate repeats in a single reply. Testing showed both
  // the 3B and 7B model can still get stuck re-confirming the same idempotent
  // step (e.g. container_network_ensure) turn after turn instead of moving
  // on, which the "don't repeat" prompt rule alone didn't reliably stop.
  // This is the structural backstop: past a small number of repeats, cut the
  // task instead of grinding silently toward MAX_AGENT_STEPS.
  const callCounts = new Map<string, number>()
  const MAX_REPEATS = 2

  try {
    outer: for (let step = 0; step < MAX_AGENT_STEPS; step++) {
      const history = state.steps.slice(-24).map(s => ({ role: s.role === 'tool' ? 'tool' : s.role, content: s.content }))

      const response = await invoke<AgentChatResponse>('agent_ollama_chat', {
        model: opts.model,
        system,
        messages: history,
        tools,
      })

      const nativeToolCalls = response.tool_calls?.length ? response.tool_calls : []
      // When there's no native tool_calls, the model's whole `content` IS the
      // call (raw or ```json-fenced JSON) rather than prose alongside one —
      // don't echo that JSON to the user as if it were a real reply.
      let toolCalls = nativeToolCalls.length ? nativeToolCalls : parseFallbackToolCall(response.content)

      // A small model batching several calls in one response sometimes just
      // repeats itself (seen in testing: the exact same container_create
      // twice in a row) rather than meaning two distinct steps — the second
      // one then fails ("name already in use") and the model panics into
      // deleting what the first call just successfully made. Collapsing
      // byte-identical consecutive calls removes the self-inflicted retry
      // without touching genuinely different steps (e.g. network_ensure
      // followed by a create).
      toolCalls = toolCalls.filter((call, i) => {
        const prev = toolCalls[i - 1]
        return !prev || prev.name !== call.name || JSON.stringify(prev.arguments) !== JSON.stringify(call.arguments)
      })

      if (!toolCalls.length) {
        state.steps.push({ role: 'assistant', content: response.content || '(sin respuesta)' })
        break
      }

      if (nativeToolCalls.length && response.content.trim()) {
        state.steps.push({ role: 'assistant', content: response.content })
      }

      for (const call of toolCalls) {
        const tool = findAgentTool(call.name)
        if (!tool) {
          state.steps.push({ role: 'tool', content: `Herramienta desconocida: ${call.name}`, toolName: call.name })
          continue
        }

        const sig = `${call.name}:${JSON.stringify(call.arguments)}`
        const repeats = (callCounts.get(sig) ?? 0) + 1
        callCounts.set(sig, repeats)
        if (repeats > MAX_REPEATS) {
          state.steps.push({
            role: 'assistant',
            content: `Corté la tarea acá: "${call.name}" se repitió ${repeats} veces con los mismos argumentos sin avanzar a un paso distinto. Revisá los pasos de arriba — probablemente falte corregir algo puntual a mano, o convenga un modelo más grande para esta tarea.`,
          })
          break outer
        }

        if (tool.dangerous) {
          const approved = await waitForApproval({ id: call.id, name: call.name, label: tool.label, arguments: call.arguments })
          if (!approved) {
            state.steps.push({ role: 'tool', content: 'El usuario rechazó esta acción.', toolName: call.name })
            continue
          }
        }

        const result = await tool.run(call.arguments, ctx)
        state.steps.push({ role: 'tool', content: result.output, toolName: call.name })
      }
    }
  } catch (e: any) {
    state.error = String(e)
    state.steps.push({ role: 'assistant', content: `❌ ${String(e)}` })
  } finally {
    state.running = false
  }

  // Auto-save memory at the end of the task — same AGENTS.md flow the manual
  // "save context" button in chat mode uses, just triggered automatically
  // here so the agent doesn't need the user to remember to click it.
  const transcript = state.steps
    .map(s => `${s.role === 'user' ? 'Usuario' : s.role === 'tool' ? `Herramienta(${s.toolName})` : 'Agente'}: ${s.content}`)
    .join('\n\n')
  await memory.saveProjectContext(opts.rootPath, transcript, { provider: 'ollama', apiKey: 'ollama', model: opts.model })
}

export function useAgentStore() {
  return { state, runAgentTask, approveToolCall, rejectToolCall, clearAgentChat }
}
