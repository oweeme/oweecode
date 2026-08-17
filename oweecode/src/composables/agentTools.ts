// Registry of tools the local agent can call. Each entry is a thin wrapper
// around an `invoke()` that already exists (agent.rs's project-confined fs/
// shell commands, or git.rs's existing commands unchanged) — no new business
// logic lives here, just the tool→invoke mapping plus the JSON-schema Ollama
// needs to offer them as `tools`. Kept as flat data (not a class hierarchy)
// so adding a `db`/`container`/`ftp`/`redis` category later is just pushing
// more entries onto AGENT_TOOLS, same pattern as aiClis.ts's AI_CLIS list.
import { invoke } from '@tauri-apps/api/core'

export interface AgentToolCtx {
  rootPath: string
}

export interface AgentToolResult {
  ok: boolean
  output: string
}

export interface AgentTool {
  name: string
  category: 'fs' | 'git' | 'shell' | 'container'
  label: string
  description: string
  parameters: { type: 'object'; properties: Record<string, any>; required?: string[] }
  // Tools that mutate anything (write a file, run a shell command, stage/commit)
  // always require explicit user approval — never auto-run, regardless of any
  // "allow read-only for session" toggle.
  dangerous: boolean
  run(args: any, ctx: AgentToolCtx): Promise<AgentToolResult>
}

// Lets the model refer to the project without knowing/leaking its absolute
// host path: "." or "./sub/dir" resolve against the project root, same
// convention as the fs tools' relative paths. An absolute path or a bare
// name with no slashes (a Podman/Docker-managed volume, e.g. "mysql_data")
// passes through untouched.
function resolveHostPath(host: string, rootPath: string): string {
  const h = host.trim()
  if (h === '.' || h === './') return rootPath
  if (h.startsWith('./')) return `${rootPath}/${h.slice(2)}`
  if (h.startsWith('/') || !h.includes('/')) return h
  return `${rootPath}/${h}`
}

// Podman (unlike Docker) refuses to guess a registry for a "short" image name
// (no registry host in it — e.g. "mysql:8" or "bitnami/mysql:8") unless
// unqualified-search-registries is configured in /etc/containers/registries.conf,
// which isn't a given on a fresh install. Rather than depend on the model
// remembering to write fully-qualified refs, always qualify to docker.io here.
//
// A reference with no "/" at all (e.g. "mysql:8.0") can NEVER be a qualified
// ref — a registry host is only recognizable when followed by a "/", so a
// bare name is always name[:tag] under the default registry, full stop, no
// matter how many dots or colons are in the tag (two earlier bugs here: a
// colon was mistaken for a registry:port, and a dotted version like "8.0"
// was mistaken for a registry hostname like "ghcr.io" — both only apply to
// the segment *before* a "/", which doesn't exist in a single-segment ref).
function qualifyImage(image: string): string {
  const img = image.trim()
  if (!img.includes('/')) return `docker.io/library/${img}`
  const first = img.split('/')[0]
  const looksQualified = first.includes('.') || first.includes(':') || first === 'localhost'
  return looksQualified ? img : `docker.io/${img}`
}

async function safe(fn: () => Promise<any>): Promise<AgentToolResult> {
  try {
    const result = await fn()
    const output = typeof result === 'string' ? result : JSON.stringify(result, null, 2)
    return { ok: true, output: output || '(sin salida)' }
  } catch (e: any) {
    return { ok: false, output: `Error: ${String(e)}` }
  }
}

export const AGENT_TOOLS: AgentTool[] = [
  {
    name: 'fs_read_file', category: 'fs', label: 'Leer archivo',
    description: 'Lee el contenido completo de un archivo del proyecto, dado un path relativo a la raíz.',
    parameters: { type: 'object', properties: { path: { type: 'string', description: 'Path relativo del archivo, ej. src/App.vue' } }, required: ['path'] },
    dangerous: false,
    run: (args, ctx) => safe(() => invoke<string>('agent_read_file', { root: ctx.rootPath, path: args.path })),
  },
  {
    name: 'fs_list_dir', category: 'fs', label: 'Listar carpeta',
    description: 'Lista los archivos y carpetas dentro de un directorio del proyecto (no recursivo). Los paths que devuelve son relativos a la raíz del proyecto — no le agregues el nombre de la carpeta del proyecto adelante, ya estás parado ahí.',
    parameters: { type: 'object', properties: { path: { type: 'string', description: 'Path relativo del directorio, "." para la raíz' } } },
    dangerous: false,
    // Delegates to agent_list_dir, but re-relativizes the paths in the
    // response before showing them to the model: it returns absolute host
    // paths (e.g. "/home/user/myproject/images"), and a model reading that
    // literal "myproject/" segment in the output was seen copying it into
    // its own next relative-path argument (creating a nested "myproject/
    // myproject/..." folder — real damage cleaned up once in testing).
    run: (args, ctx) => safe(async () => {
      const entries = await invoke<{ name: string; path: string; is_dir: boolean; size: number; modified: string }[]>('agent_list_dir', { root: ctx.rootPath, path: args.path ?? '.', showHidden: false })
      return entries.map(e => ({
        name: e.name,
        is_dir: e.is_dir,
        size: e.size,
        path: e.path.startsWith(ctx.rootPath) ? e.path.slice(ctx.rootPath.length).replace(/^[/\\]/, '') : e.path,
      }))
    }),
  },
  {
    name: 'fs_search', category: 'fs', label: 'Buscar en el proyecto',
    description: 'Busca un texto/substring en todos los archivos del proyecto y devuelve las coincidencias con archivo y línea.',
    parameters: { type: 'object', properties: { query: { type: 'string' } }, required: ['query'] },
    dangerous: false,
    run: (args, ctx) => safe(() => invoke('agent_search', { root: ctx.rootPath, query: args.query, caseSensitive: false, useRegex: false, maxResults: 50 })),
  },
  {
    name: 'fs_write_file', category: 'fs', label: 'Escribir archivo',
    description: 'Crea o sobrescribe un archivo del proyecto con el contenido dado. Requiere aprobación del usuario.',
    parameters: { type: 'object', properties: { path: { type: 'string' }, content: { type: 'string' } }, required: ['path', 'content'] },
    dangerous: true,
    // Hard guard, not just a prompt rule: a rewrite with empty/blank content
    // silently blanked a file that already had real content in testing (the
    // model reported "success" both times, so nothing in the transcript
    // flagged it). Refuse instead of trusting the model noticed its own
    // mistake — an empty *new* file is still allowed, only overwriting an
    // existing non-empty one with nothing is blocked.
    run: (args, ctx) => safe(async () => {
      const content = String(args.content ?? '')
      if (content.trim() === '') {
        let existing = ''
        try { existing = await invoke<string>('agent_read_file', { root: ctx.rootPath, path: args.path }) } catch { /* doesn't exist yet — an empty new file is fine */ }
        if (existing.trim() !== '') {
          throw new Error(`Bloqueado: ibas a sobrescribir "${args.path}" con contenido vacío, pero ese archivo ya tiene contenido real — seguramente no era la intención. Si de verdad hay que vaciarlo, confirmalo con el usuario primero.`)
        }
      }
      await invoke('agent_write_file', { root: ctx.rootPath, path: args.path, content })
      return `Escrito: ${args.path}`
    }),
  },
  {
    name: 'fs_create_dir', category: 'fs', label: 'Crear carpeta',
    description: 'Crea una carpeta vacía en el proyecto. No hace falta si vas a escribir un archivo adentro — fs_write_file ya crea las carpetas que falten. Requiere aprobación del usuario.',
    parameters: { type: 'object', properties: { path: { type: 'string' } }, required: ['path'] },
    dangerous: true,
    run: (args, ctx) => safe(async () => { await invoke('agent_create_dir', { root: ctx.rootPath, path: args.path }); return `Carpeta creada: ${args.path}` }),
  },
  {
    name: 'git_status', category: 'git', label: 'Estado de git',
    description: 'Devuelve el estado del repo: rama, ahead/behind, y archivos modificados/staged/untracked.',
    parameters: { type: 'object', properties: {} },
    dangerous: false,
    run: (_args, ctx) => safe(() => invoke('git_status', { path: ctx.rootPath })),
  },
  {
    name: 'git_diff', category: 'git', label: 'Diff de un archivo',
    description: 'Muestra el diff de un archivo puntual (staged o no).',
    parameters: { type: 'object', properties: { file: { type: 'string' }, staged: { type: 'boolean' } }, required: ['file'] },
    dangerous: false,
    run: (args, ctx) => safe(() => invoke<string>('git_diff', { path: ctx.rootPath, file: args.file, staged: !!args.staged })),
  },
  {
    name: 'git_stage', category: 'git', label: 'Agregar al stage',
    description: 'Agrega archivos al stage de git (git add). Requiere aprobación del usuario.',
    parameters: { type: 'object', properties: { files: { type: 'array', items: { type: 'string' } } }, required: ['files'] },
    dangerous: true,
    run: (args, ctx) => safe(async () => { await invoke('git_stage', { path: ctx.rootPath, files: args.files }); return `Agregado al stage: ${(args.files as string[]).join(', ')}` }),
  },
  {
    name: 'git_commit', category: 'git', label: 'Commit',
    description: 'Crea un commit con los archivos ya en stage. Requiere aprobación del usuario.',
    parameters: { type: 'object', properties: { message: { type: 'string' } }, required: ['message'] },
    dangerous: true,
    run: (args, ctx) => safe(() => invoke<string>('git_commit', { path: ctx.rootPath, message: args.message })),
  },
  {
    name: 'shell_run_command', category: 'shell', label: 'Ejecutar comando',
    description: 'Ejecuta un comando de shell puntual (build, tests, etc.) dentro del proyecto y devuelve stdout/stderr/exit code. Siempre requiere aprobación del usuario.',
    parameters: { type: 'object', properties: { command: { type: 'string' }, cwd: { type: 'string', description: 'Directorio relativo, opcional' } }, required: ['command'] },
    dangerous: true,
    run: (args, ctx) => safe(() => invoke('agent_run_command', { root: ctx.rootPath, cwd: args.cwd ?? null, command: args.command })),
  },
  {
    name: 'container_list', category: 'container', label: 'Listar contenedores del proyecto',
    description: 'Lista los contenedores (Podman/Docker) que pertenecen a este proyecto — los que se crearon con container_create quedan etiquetados automáticamente, así que esto no muestra contenedores de otros proyectos ni del sistema en general.',
    parameters: { type: 'object', properties: {} },
    dangerous: false,
    run: (_args, ctx) => safe(() => invoke('container_list_for_project', { root: ctx.rootPath, all: true })),
  },
  {
    name: 'container_create', category: 'container', label: 'Crear contenedor',
    description: 'Crea un contenedor Podman/Docker para este proyecto — queda etiquetado a él automáticamente (container_list solo lo va a mostrar acá). El volumen con host "." (raíz del proyecto) es SOLO para contenedores de código/aplicación (container: "/app", o "/var/www/html" para Apache/Nginx+PHP-FPM) — así el contenedor puede leer y ejecutar tu código. NUNCA montes "." como carpeta de datos de una base de datos (mysql, mariadb, postgres, redis) — esos necesitan un volumen propio y aislado, con un nombre simple sin puntos ni barras (ej. host: "<nombre_proyecto>_mysql_data", container: "/var/lib/mysql" para MySQL/MariaDB, "/var/lib/postgresql/data" para Postgres, "/data" para Redis). Montar el proyecto ahí rompe la base de datos (no puede inicializar con esos archivos) y además le cambia el dueño a los archivos de tu proyecto — nunca lo hagas. Para una base de datos vinculada a una app, primero container_network_ensure con un nombre de red, creá el contenedor de la base de datos en esa red con su volumen propio, y despues el de la app en la misma red (se resuelven por nombre de contenedor como hostname). Usá exactamente estos tags (no inventes versiones): mysql:8, mariadb:11, postgres:16, redis:7, php:8.4-fpm, php:8.4-apache, nginx:latest, golang:1.22, node:20, python:3.12. MySQL y MariaDB tienen esquemas de versión distintos — no uses "mysql:11", esa versión no existe. No hace falta que la imagen incluya el registro (docker.io/...), se completa solo.',
    parameters: {
      type: 'object',
      properties: {
        name: { type: 'string' },
        image: { type: 'string' },
        ports: { type: 'array', items: { type: 'object', properties: { host: { type: 'string' }, container: { type: 'string' }, protocol: { type: 'string' } } } },
        volumes: { type: 'array', items: { type: 'object', properties: { host: { type: 'string', description: '"." para la raíz del proyecto, o un nombre simple (ej. mysql_data) para un volumen gestionado' }, container: { type: 'string' } } } },
        env: { type: 'array', items: { type: 'object', properties: { key: { type: 'string' }, value: { type: 'string' } } } },
        command: { type: 'string' },
        restart_policy: { type: 'string', description: 'ej. unless-stopped, o vacío' },
        network: { type: 'string' },
        workdir: { type: 'string' },
      },
      required: ['name', 'image'],
    },
    dangerous: true,
    run: (args, ctx) => safe(async () => {
      const opts = {
        name: String(args.name ?? ''),
        image: qualifyImage(String(args.image ?? '')),
        ports: (args.ports ?? []).map((p: any) => ({ host: String(p.host ?? ''), container: String(p.container ?? ''), protocol: p.protocol || 'tcp' })),
        volumes: (args.volumes ?? []).map((v: any) => ({ host: resolveHostPath(String(v.host ?? ''), ctx.rootPath), container: String(v.container ?? '') })),
        env: (args.env ?? []).map((e: any) => ({ key: String(e.key ?? ''), value: String(e.value ?? '') })),
        command: args.command ?? '',
        restart_policy: args.restart_policy ?? '',
      }
      // Hard block, not just a prompt instruction: a database image mounting
      // the whole project root as its data dir isn't recoverable advice, it's
      // real damage — MySQL's entrypoint chown's its data dir to the
      // container's internal user, which silently reassigned ownership of
      // every file in a real user's project the one time this happened in
      // testing (still group-writable, so not locked out, but wrong owner).
      // Prompt wording alone didn't stop a 3B model from doing it twice.
      const DB_IMAGE_MARKERS = ['mysql', 'mariadb', 'postgres', 'redis', 'mongo']
      const suggestion = `${ctx.rootPath.split('/').filter(Boolean).pop() || 'proyecto'}_data`
      if (DB_IMAGE_MARKERS.some(m => opts.image.toLowerCase().includes(m))) {
        const badVolume = opts.volumes.find((v: { host: string }) => v.host === ctx.rootPath)
        if (badVolume) {
          throw new Error(`Bloqueado: no se puede montar la raíz del proyecto como carpeta de datos de una base de datos — le rompe la inicialización y le cambia el dueño a los archivos del proyecto. Usá un volumen con nombre simple en su lugar, ej. host: "${suggestion}", container: "${badVolume.container}".`)
        }
        // Same idea, different mistake: a host path that isn't the project
        // root but IS some other absolute filesystem path (e.g. "/var/lib/mysql"
        // taken literally from the container side) — the agent has no business
        // touching arbitrary system directories either. Only a bare name (a
        // Podman/Docker-managed volume, isolated from the real filesystem) is
        // safe for a database's data directory.
        const absoluteVolume = opts.volumes.find((v: { host: string }) => v.host.startsWith('/'))
        if (absoluteVolume) {
          throw new Error(`Bloqueado: "${absoluteVolume.host}" es una ruta absoluta del sistema, no un volumen gestionado — un contenedor de base de datos no debe montar carpetas reales del host salvo la del proyecto (y esa tampoco sirve acá). Usá un volumen con nombre simple, ej. host: "${suggestion}", container: "${absoluteVolume.container}".`)
        }
      }
      const finalOpts = {
        ...opts,
        network: args.network ?? '',
        workdir: args.workdir ?? '',
        label: ctx.rootPath,
      }
      try {
        const id = await invoke<string>('container_create', { opts: finalOpts })
        return `Contenedor creado: ${finalOpts.name} (id ${id.slice(0, 12)})`
      } catch (e: any) {
        const msg = String(e)
        if (msg.includes('already in use')) {
          throw new Error(`El nombre "${finalOpts.name}" ya está en uso — lo más probable es que ya se haya creado en un paso anterior de esta misma tarea. Usá container_list para confirmarlo en vez de reintentar o borrarlo. (${msg})`)
        }
        throw e
      }
    }),
  },
  {
    name: 'container_start', category: 'container', label: 'Iniciar contenedor',
    description: 'Arranca un contenedor detenido, por id.',
    parameters: { type: 'object', properties: { id: { type: 'string' } }, required: ['id'] },
    dangerous: true,
    run: (args) => safe(async () => { await invoke('container_start', { id: args.id }); return `Contenedor iniciado: ${args.id}` }),
  },
  {
    name: 'container_stop', category: 'container', label: 'Detener contenedor',
    description: 'Detiene un contenedor en ejecución, por id.',
    parameters: { type: 'object', properties: { id: { type: 'string' } }, required: ['id'] },
    dangerous: true,
    run: (args) => safe(async () => { await invoke('container_stop', { id: args.id }); return `Contenedor detenido: ${args.id}` }),
  },
  {
    name: 'container_remove', category: 'container', label: 'Eliminar contenedor',
    description: 'Elimina un contenedor (parado, o forzado si sigue corriendo).',
    parameters: { type: 'object', properties: { id: { type: 'string' }, force: { type: 'boolean' } }, required: ['id'] },
    dangerous: true,
    run: (args) => safe(async () => { await invoke('container_remove', { id: args.id, force: !!args.force }); return `Contenedor eliminado: ${args.id}` }),
  },
  {
    name: 'container_logs', category: 'container', label: 'Ver logs del contenedor',
    description: 'Devuelve las últimas líneas de stdout/stderr de un contenedor — usalo para chequear si arrancó bien (ej. si MySQL ya está listo para aceptar conexiones).',
    parameters: { type: 'object', properties: { id: { type: 'string' }, tail: { type: 'number' } }, required: ['id'] },
    dangerous: false,
    run: (args) => safe(() => invoke<string>('container_logs', { id: args.id, tail: args.tail ?? 100 })),
  },
  {
    name: 'container_image_pull', category: 'container', label: 'Descargar imagen',
    description: 'Descarga una imagen de contenedor (ej. antes de crear un contenedor con ella, para ver el progreso antes o evitar que container_create tarde de más).',
    parameters: { type: 'object', properties: { image: { type: 'string' } }, required: ['image'] },
    dangerous: true,
    run: (args) => safe(async () => { const image = qualifyImage(String(args.image ?? '')); await invoke('image_pull', { image }); return `Imagen descargada: ${image}` }),
  },
  {
    name: 'container_network_ensure', category: 'container', label: 'Crear red de contenedores',
    description: 'Crea (si no existe) una red Podman/Docker para conectar varios contenedores entre sí por nombre — necesario antes de vincular, por ejemplo, un contenedor de app con uno de base de datos.',
    parameters: { type: 'object', properties: { name: { type: 'string' } }, required: ['name'] },
    dangerous: true,
    run: (args) => safe(async () => { await invoke('network_ensure', { name: args.name }); return `Red lista: ${args.name}` }),
  },
  {
    name: 'container_export', category: 'container', label: 'Exportar contenedor a .tar',
    description: 'Empaqueta el estado actual del contenedor (no solo la imagen original, cualquier cambio hecho desde que arrancó) en un archivo .tar dentro del proyecto, para llevarlo a otra máquina con "podman load -i archivo.tar" o "docker load -i archivo.tar".',
    parameters: {
      type: 'object',
      properties: {
        id: { type: 'string' },
        name: { type: 'string', description: 'nombre para el tag temporal, ej. mibolivia-mysql' },
        outPath: { type: 'string', description: 'ruta relativa dentro del proyecto, ej. exports/mibolivia-mysql.tar' },
      },
      required: ['id', 'name', 'outPath'],
    },
    dangerous: true,
    run: (args, ctx) => safe(async () => {
      const outPath = resolveHostPath(String(args.outPath ?? ''), ctx.rootPath)
      await invoke('container_export', { id: args.id, name: args.name, outPath })
      return `Exportado a ${outPath}`
    }),
  },
]

export function findAgentTool(name: string): AgentTool | undefined {
  return AGENT_TOOLS.find(t => t.name === name)
}

// Ollama's `/api/chat` `tools` field, in the OpenAI-style function-calling
// shape it expects — derived from AGENT_TOOLS so there's one source of truth
// instead of a schema kept in sync by hand.
export function buildOllamaToolsSchema(): any[] {
  return AGENT_TOOLS.map(tool => ({
    type: 'function',
    function: {
      name: tool.name,
      description: tool.description,
      parameters: tool.parameters,
    },
  }))
}
