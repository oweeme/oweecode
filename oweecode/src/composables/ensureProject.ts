// Makes sure a project folder is open before saving generated files: offers to create
// ~/Dev/<name> (or pick another folder) when nothing is open.
import { invoke } from '@tauri-apps/api/core'
import { ask, open as openDialog } from '@tauri-apps/plugin-dialog'
import { useEditorStore } from './useEditorStore'

export async function ensureProject(nameHint: string): Promise<boolean> {
  const store = useEditorStore()
  if (store.state.rootPath) return true
  const slug = nameHint.trim().toLowerCase().replace(/[^a-z0-9\-_]+/g, '-').replace(/^-+|-+$/g, '') || 'mi-proyecto'
  const home = await invoke<string>('get_home_dir')
  const dir = `${home}/Dev/${slug}`
  const create = await ask(`No hay ningún proyecto abierto.\n\n¿Crear el proyecto "${dir}" y guardar ahí?`,
    { title: 'Guardar en un proyecto', kind: 'info', okLabel: 'Crear proyecto', cancelLabel: 'Elegir otra carpeta' })
  let target: string | null = dir
  if (create) await invoke('create_dir_cmd', { path: dir })
  else { const picked = await openDialog({ directory: true, title: 'Elegí o creá la carpeta del proyecto' }); target = typeof picked === 'string' ? picked : null }
  if (!target) return false
  store.setRootPath(target)
  return true
}
