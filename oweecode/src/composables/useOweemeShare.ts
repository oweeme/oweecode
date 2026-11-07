// What can be shared in the Oweeme chat, and how a received attachment is opened:
//   code     the selection in the editor           → copy / open the file at that line
//   file     the whole active file                 → open it if it exists in your project, or copy
//   flow     the Flows tab (nodes + edges)         → import into your Flows tab
//   preview  the address shown in Browser preview  → open in your preview (LAN address for teammates)
import { invoke } from '@tauri-apps/api/core'
import { flowState } from './useFlowState'
import { useEditorStore } from './useEditorStore'
import type { OweemeAttachment } from './useOweemeChat'

export interface PreviewShare { url: string; lanUrl: string | null; device: string | null }

/** Replace localhost/127.0.0.1 with this machine's LAN IP so others on the network can open it. */
export function toLanUrl(url: string, lanIp: string | null): string | null {
  if (!lanIp) return null
  const m = /^(https?:\/\/)(localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\])(?=[:/]|$)/i.exec(url)
  return m ? url.replace(m[0], m[1] + lanIp) : null
}

export function useOweemeShare() {
  const store = useEditorStore()

  function relPath(path: string) {
    const root = store.state.rootPath ?? ''
    return root && path.startsWith(root) ? path.slice(root.length).replace(/^[\\/]+/, '') : path
  }

  function activeEditorTab() {
    const tab = store.activeTab()
    return tab && (tab.type === 'code' || !tab.type) ? tab : null
  }

  function selection(): OweemeAttachment | null {
    const code = store.state.selectedText
    const tab = activeEditorTab()
    if (!code.trim() || !tab) return null
    return { kind: 'code', name: relPath(tab.path), content: code }
  }

  function currentFile(): OweemeAttachment | null {
    const tab = activeEditorTab()
    if (!tab || !tab.content) return null
    return { kind: 'file', name: relPath(tab.path), content: tab.content }
  }

  function flow(): OweemeAttachment | null {
    if (!flowState.nodes.length) return null
    return { kind: 'flow', name: flowState.flowName, content: JSON.stringify({ name: flowState.flowName, nodes: flowState.nodes, edges: flowState.edges }) }
  }

  async function preview(): Promise<OweemeAttachment | null> {
    const tab = store.state.tabs.find((t) => t.type === 'browser' && t.browserUrl)
    if (!tab?.browserUrl) return null
    const lan = toLanUrl(tab.browserUrl, await invoke<string | null>('oweeme_lan_ip').catch(() => null))
    const data: PreviewShare = { url: tab.browserUrl, lanUrl: lan, device: tab.browserDevice ?? null }
    return { kind: 'preview', name: tab.browserUrl.replace(/^https?:\/\//, ''), content: JSON.stringify(data) }
  }

  /** Opens a shared file/selection when the same path exists in the open project. */
  async function openInProject(a: OweemeAttachment) {
    const root = store.state.rootPath
    if (!root) return false
    const path = `${root}/${a.name.replace(/:\d+$/, '')}`
    try { await store.openFile(path); return true } catch { return false }
  }

  /** Loads a shared flow into the Flows tab (the current flow is kept as backup). */
  function importFlow(a: OweemeAttachment) {
    const data = JSON.parse(a.content) as { name?: string; nodes: any[]; edges: any[] }
    flowState.flowBackup = { nodes: flowState.nodes, edges: flowState.edges }
    flowState.mode = 'flow'
    flowState.nodes = data.nodes ?? []
    flowState.edges = data.edges ?? []
    flowState.flowName = data.name ?? a.name
    store.openFlowTab()
  }

  function openPreview(a: OweemeAttachment) {
    const p = JSON.parse(a.content) as PreviewShare
    store.openBrowserTab(p.lanUrl ?? p.url)
  }

  return { selection, currentFile, flow, preview, openInProject, importFlow, openPreview }
}
