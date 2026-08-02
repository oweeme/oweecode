import { reactive } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export type TabType = 'code' | 'image' | 'database' | 'ftp' | 'api' | 'ftp-file' | 'cli' | 'redis' | 'git-diff' | 'er-diagram' | 'container-logs' | 'browser' | 'design'

export interface Tab {
  path: string
  name: string
  content: string
  modified: boolean
  language: string
  type: TabType
  // database tab extras
  connId?: string
  tableName?: string
  driver?: string
  // ftp tab extras
  ftpConnId?: string
  ftpConnName?: string
  ftpProtocol?: string
  // ftp-file tab extras (remote file opened in editor)
  ftpFilePath?: string   // remote path on server
  // api tab extras
  apiRequestId?: string
  // redis tab extras
  redisConnId?: string
  redisConnName?: string
  // cli tab extras
  cliTool?: string
  // git-diff tab extras
  gitFilePath?: string
  gitStaged?: boolean
  // er-diagram tab extras
  erConnId?: string
  erConnName?: string
  erDriver?: string
  // container-logs tab extras
  containerId?: string
  containerName?: string
  // browser tab extras
  browserUrl?: string
}

interface EditorState {
  rootPath: string
  tabs: Tab[]
  activeTabPath: string | null
  cursorLine: number
  cursorCol: number
  selectedText: string
  lspStatus: 'idle' | 'starting' | 'ready' | 'error'
}

const IMAGE_EXTS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'ico', 'bmp', 'avif'])

const state = reactive<EditorState>({
  rootPath: '',
  tabs: [],
  activeTabPath: null,
  cursorLine: 1,
  cursorCol: 1,
  selectedText: '',
  lspStatus: 'idle',
})

function detectLanguage(filename: string): string {
  // Exact filename matches for dotfiles and well-known config files
  const exactMap: Record<string, string> = {
    '.htaccess': 'apache', '.env': 'shell', '.envrc': 'shell',
    '.prettierrc': 'json', '.eslintrc': 'json', '.babelrc': 'json',
    '.stylelintrc': 'json', '.browserslistrc': 'text',
    'dockerfile': 'shell', 'makefile': 'shell', 'gemfile': 'ruby',
    '.gitignore': 'shell', '.gitattributes': 'text', '.editorconfig': 'toml',
    '.npmrc': 'toml', '.yarnrc': 'yaml',
  }
  const lower = filename.toLowerCase()
  if (exactMap[lower]) return exactMap[lower]

  const ext = filename.split('.').pop()?.toLowerCase() ?? ''
  const map: Record<string, string> = {
    ts: 'typescript', tsx: 'tsx',
    js: 'javascript', jsx: 'jsx', mjs: 'javascript', cjs: 'javascript',
    vue: 'vue', php: 'php', go: 'go',
    html: 'html', htm: 'html',
    astro: 'astro',
    svelte: 'svelte',
    // Handlebars' {{ }} delimiters aren't in @codemirror/legacy-modes either;
    // Jinja2's mode highlights that same {{ }}/{% %} shape reasonably well.
    hbs: 'handlebars', handlebars: 'handlebars',
    css: 'css', scss: 'css', sass: 'css',
    json: 'json', jsonc: 'json', json5: 'json', md: 'markdown',
    rs: 'rust', py: 'python', dart: 'dart',
    ada: 'ada', adb: 'ada', ads: 'ada',
    sql: 'sql', sh: 'shell', bash: 'shell', zsh: 'shell',
    toml: 'toml', yaml: 'yaml', yml: 'yaml',
    xml: 'xml', svg: 'xml', xhtml: 'xml',
    cpp: 'cpp', cc: 'cpp', c: 'cpp', h: 'cpp', hpp: 'cpp',
    env: 'shell',
  }
  return map[ext] ?? 'text'
}

function isImage(filename: string): boolean {
  const ext = filename.split('.').pop()?.toLowerCase() ?? ''
  return IMAGE_EXTS.has(ext)
}

export function useEditorStore() {
  function setRootPath(p: string) {
    state.rootPath = p
  }

  async function openFile(path: string, _line?: number) {
    const existing = state.tabs.find(t => t.path === path)
    if (existing) { state.activeTabPath = path; return }

    const name = path.split('/').pop() ?? path

    if (isImage(name)) {
      state.tabs.push({ path, name, content: '', modified: false, language: 'image', type: 'image' })
      state.activeTabPath = path
      return
    }

    try {
      const content = await invoke<string>('open_file', { path })
      state.tabs.push({ path, name, content, modified: false, language: detectLanguage(name), type: 'code' })
      state.activeTabPath = path
    } catch (e) {
      console.error('Failed to open file:', e)
    }
  }

  async function saveFile(path: string) {
    const tab = state.tabs.find(t => t.path === path)
    if (!tab || tab.type === 'image') return
    if (tab.type === 'ftp-file') {
      await invoke('remote_write_file', { id: tab.ftpConnId!, path: tab.ftpFilePath!, content: tab.content })
      tab.modified = false
      return
    }
    if (tab.type === 'api' || tab.type === 'database' || tab.type === 'ftp' || tab.type === 'redis' || tab.type === 'cli' || tab.type === 'git-diff' || tab.type === 'er-diagram' || tab.type === 'container-logs' || tab.type === 'browser' || tab.type === 'design') return
    await invoke('save_file', { path, content: tab.content })
    tab.modified = false
    window.dispatchEvent(new CustomEvent('file-saved'))
  }

  async function saveActiveFile() {
    if (state.activeTabPath) await saveFile(state.activeTabPath)
  }

  function updateContent(path: string, content: string) {
    const tab = state.tabs.find(t => t.path === path)
    if (tab && (tab.type === 'code' || tab.type === 'ftp-file' || tab.type === 'api')) { tab.content = content; tab.modified = true }
  }

  function closeTab(path: string) {
    const idx = state.tabs.findIndex(t => t.path === path)
    if (idx === -1) return
    state.tabs.splice(idx, 1)
    if (state.activeTabPath === path) {
      state.activeTabPath = state.tabs[Math.max(0, idx - 1)]?.path ?? null
    }
  }

  function setActive(path: string) { state.activeTabPath = path }
  function setCursor(line: number, col: number) { state.cursorLine = line; state.cursorCol = col }
  function setSelectedText(text: string) { state.selectedText = text }
  const activeTab = () => state.tabs.find(t => t.path === state.activeTabPath) ?? null

  function renameTab(oldPath: string, newPath: string, newName: string) {
    const tab = state.tabs.find(t => t.path === oldPath)
    if (!tab) return
    tab.path = newPath
    tab.name = newName
    if (state.activeTabPath === oldPath) state.activeTabPath = newPath
  }

  function openFtpConn(connId: string, connName: string, protocol: string) {
    const path = `ftp://${connId}`
    const existing = state.tabs.find(t => t.path === path)
    if (existing) { state.activeTabPath = path; return }
    state.tabs.push({ path, name: connName, content: '', modified: false, language: 'text', type: 'ftp', ftpConnId: connId, ftpConnName: connName, ftpProtocol: protocol })
    state.activeTabPath = path
  }

  function openErDiagram(connId: string, connName: string, driver: string) {
    const path = `er-diagram://${connId}`
    const existing = state.tabs.find(t => t.path === path)
    if (existing) { state.activeTabPath = path; return }
    state.tabs.push({
      path, name: `${connName} (ER)`, content: '', modified: false, language: 'text',
      type: 'er-diagram', erConnId: connId, erConnName: connName, erDriver: driver,
    })
    state.activeTabPath = path
  }

  function openContainerLogs(containerId: string, containerName: string) {
    const path = `container-logs://${containerId}`
    const existing = state.tabs.find(t => t.path === path)
    if (existing) { state.activeTabPath = path; return }
    state.tabs.push({
      path, name: `${containerName} (logs)`, content: '', modified: false, language: 'text',
      type: 'container-logs', containerId, containerName,
    })
    state.activeTabPath = path
  }

  function openGitDiff(filePath: string, staged: boolean) {
    const path = `git-diff://${staged ? 'staged' : 'unstaged'}/${filePath}`
    const existing = state.tabs.find(t => t.path === path)
    if (existing) { state.activeTabPath = path; return }
    const name = filePath.split('/').pop() ?? filePath
    state.tabs.push({
      path, name: `${name} (diff)`, content: '', modified: false, language: 'text',
      type: 'git-diff', gitFilePath: filePath, gitStaged: staged,
    })
    state.activeTabPath = path
  }

  function openRedisConn(connId: string, connName: string) {
    const path = `redis://${connId}`
    const existing = state.tabs.find(t => t.path === path)
    if (existing) { state.activeTabPath = path; return }
    state.tabs.push({ path, name: connName, content: '', modified: false, language: 'text', type: 'redis', redisConnId: connId, redisConnName: connName })
    state.activeTabPath = path
  }

  function openDbTable(connId: string, tableName: string, _connName: string, driver: string) {
    const path = `db://${connId}/${tableName}`
    const existing = state.tabs.find(t => t.path === path)
    if (existing) { state.activeTabPath = path; return }
    state.tabs.push({ path, name: tableName, content: '', modified: false, language: 'sql', type: 'database', connId, tableName, driver })
    state.activeTabPath = path
  }

  function openNewTable(connId: string, driver: string) {
    const path = `db://${connId}/__new__`
    const existing = state.tabs.find(t => t.path === path)
    if (existing) { state.activeTabPath = path; return }
    state.tabs.push({ path, name: '+', content: '', modified: false, language: 'sql', type: 'database', connId, tableName: '', driver })
    state.activeTabPath = path
  }

  function tabTableCreated(path: string, newTableName: string) {
    const tab = state.tabs.find(t => t.path === path)
    if (!tab) return
    const newPath = `db://${tab.connId}/${newTableName}`
    tab.path = newPath
    tab.name = newTableName
    tab.tableName = newTableName
    if (state.activeTabPath === path) state.activeTabPath = newPath
  }

  function openFtpFile(connId: string, remotePath: string, fileName: string, content: string) {
    const path = `ftpfile://${connId}${remotePath}`
    const existing = state.tabs.find(t => t.path === path)
    if (existing) { state.activeTabPath = path; return }
    state.tabs.push({
      path, name: fileName, content, modified: false,
      language: detectLanguage(fileName), type: 'ftp-file',
      ftpConnId: connId, ftpFilePath: remotePath,
    })
    state.activeTabPath = path
  }

  // Each call opens a fresh independent tab (not a singleton like the
  // DB/FTP/Redis connections) — you might want localhost:3000 and :8080 open
  // side by side, so there's no reason to force reuse of a single tab.
  let browserTabCounter = 0
  function openBrowserTab(url?: string) {
    const id = crypto.randomUUID()
    const path = `browser://${id}`
    browserTabCounter++
    state.tabs.push({
      path, name: `${browserTabCounter > 1 ? `Navegador ${browserTabCounter}` : 'Navegador'}`,
      content: '', modified: false, language: 'text',
      type: 'browser', browserUrl: url || 'http://localhost:3000',
    })
    state.activeTabPath = path
  }

  // Each call opens a fresh independent design canvas — same reasoning as
  // openBrowserTab, you might be maquetando two different pages at once.
  let designTabCounter = 0
  function openDesignTab() {
    const id = crypto.randomUUID()
    const path = `design://${id}`
    designTabCounter++
    state.tabs.push({
      path, name: `${designTabCounter > 1 ? `Diseño ${designTabCounter}` : 'Diseño'}`,
      content: '', modified: false, language: 'text',
      type: 'design',
    })
    state.activeTabPath = path
  }

  function openCliTab(cliId: string, label: string) {
    const path = `cli://${cliId}`
    const existing = state.tabs.find(t => t.path === path)
    if (existing) { state.activeTabPath = path; return }
    state.tabs.push({ path, name: label, content: '', modified: false, language: 'shell', type: 'cli', cliTool: cliId })
    state.activeTabPath = path
  }

  function openApiRequest(requestId: string, name: string, initialJson?: string) {
    const path = `api://${requestId}`
    const existing = state.tabs.find(t => t.path === path)
    if (existing) { state.activeTabPath = path; return }
    state.tabs.push({ path, name, content: initialJson ?? '{}', modified: false, language: 'json', type: 'api', apiRequestId: requestId })
    state.activeTabPath = path
  }

  function closeTabByPath(path: string) {
    // Close any tab whose path starts with the deleted path (handles folder deletes)
    const toClose = state.tabs.filter(t => t.path === path || t.path.startsWith(path + '/'))
    toClose.forEach(t => closeTab(t.path))
  }

  return { state, activeTab, setRootPath, openFile, saveFile, saveActiveFile, updateContent, closeTab, closeTabByPath, renameTab, setActive, setCursor, setSelectedText, openDbTable, openNewTable, tabTableCreated, openFtpConn, openFtpFile, openApiRequest, openCliTab, openRedisConn, openGitDiff, openErDiagram, openContainerLogs, openBrowserTab, openDesignTab }
}
