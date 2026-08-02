// Single source of truth for every keyboard shortcut this app defines itself
// (not third-party defaults like CodeMirror's or Vue Flow's own keymaps —
// those are documented inline as comments where our own code sits next to
// them, since they're stable and maintained upstream).
//
// `scope` matters for conflict detection: 'global' shortcuts fire regardless
// of what has focus, so two 'global' entries sharing a combo is a real bug.
// Any other scope only fires while that specific panel/element has DOM
// focus, so the same combo can safely repeat across different scoped groups
// (e.g. F2 means "rename" in both the file explorer and the FTP panes —
// fine, since only one of them can hold focus at a time). A scoped combo
// conflicts only with a 'global' one (global always listens on top of
// whatever's focused) or with another entry in the exact same scope.
export type ShortcutScope =
  | 'global'
  | 'editor'
  | 'file-explorer'
  | 'ftp'
  | 'er-diagram'
  | 'design-canvas'
  | 'terminal'

export interface ShortcutDef {
  keys: string[]
  descKey: string
  scope: ShortcutScope
}

export interface ShortcutGroup {
  titleKey: string
  items: ShortcutDef[]
}

export const SHORTCUT_GROUPS: ShortcutGroup[] = [
  {
    titleKey: 'scEditor',
    items: [
      { keys: ['Ctrl', 'S'], descKey: 'tipSave', scope: 'editor' },
      { keys: ['Ctrl', 'F'], descKey: 'scToggleSearch', scope: 'editor' },
      { keys: ['Esc'], descKey: 'scCloseSearch', scope: 'editor' },
      { keys: ['F3'], descKey: 'scFindNext', scope: 'editor' },
      { keys: ['Shift', 'F3'], descKey: 'scFindPrev', scope: 'editor' },
      { keys: ['Ctrl', 'Z'], descKey: 'scUndo', scope: 'editor' },
      { keys: ['Ctrl', 'Y'], descKey: 'scRedo', scope: 'editor' },
      { keys: ['Ctrl', '/'], descKey: 'scToggleComment', scope: 'editor' },
      { keys: ['Ctrl', 'D'], descKey: 'scSelectNext', scope: 'editor' },
      { keys: ['Alt', 'Z'], descKey: 'scToggleWrap', scope: 'editor' },
      { keys: ['Ctrl', '+'], descKey: 'scZoomIn', scope: 'editor' },
      { keys: ['Ctrl', '-'], descKey: 'scZoomOut', scope: 'editor' },
      { keys: ['Ctrl', '0'], descKey: 'scResetZoom', scope: 'editor' },
      { keys: ['Tab'], descKey: 'scIndent', scope: 'editor' },
      { keys: ['Shift', 'Tab'], descKey: 'scUnindent', scope: 'editor' },
    ],
  },
  {
    titleKey: 'scTabs',
    items: [
      { keys: ['Ctrl', 'W'], descKey: 'scCloseTab', scope: 'global' },
      { keys: ['Ctrl', 'Tab'], descKey: 'scNextTab', scope: 'global' },
      { keys: ['Ctrl', 'Shift', 'Tab'], descKey: 'scPrevTab', scope: 'global' },
    ],
  },
  {
    titleKey: 'scInterface',
    items: [
      { keys: ['Ctrl', 'B'], descKey: 'scToggleSidebar', scope: 'global' },
      { keys: ['Ctrl', 'Shift', 'F'], descKey: 'scSearchFilesShortcut', scope: 'global' },
      { keys: ['Ctrl', '`'], descKey: 'scToggleTerminal', scope: 'global' },
      { keys: ['Ctrl', 'Shift', 'A'], descKey: 'scOpenAI', scope: 'global' },
      { keys: ['Ctrl', 'Shift', 'C'], descKey: 'scOpenClaude', scope: 'global' },
    ],
  },
  {
    // Jump straight to a sidebar panel or pop open a fresh workspace tab —
    // picked to avoid both this app's other global combos above and
    // CodeMirror's own Mod-Shift-K/L/M (delete line / select matches / lint
    // panel) and the common native "paste as plain text" (Ctrl+Shift+V).
    titleKey: 'scPanels',
    items: [
      { keys: ['Ctrl', 'Shift', 'E'], descKey: 'scPanelExplorer', scope: 'global' },
      { keys: ['Ctrl', 'Shift', 'D'], descKey: 'scPanelDatabase', scope: 'global' },
      { keys: ['Ctrl', 'Shift', 'T'], descKey: 'scPanelFtp', scope: 'global' },
      { keys: ['Ctrl', 'Shift', 'R'], descKey: 'scPanelRedis', scope: 'global' },
      { keys: ['Ctrl', 'Shift', 'G'], descKey: 'scPanelGit', scope: 'global' },
      { keys: ['Ctrl', 'Shift', 'N'], descKey: 'scPanelContainers', scope: 'global' },
      { keys: ['Ctrl', 'Shift', 'Q'], descKey: 'scPanelApi', scope: 'global' },
      { keys: ['Ctrl', 'Shift', 'X'], descKey: 'scPanelDesign', scope: 'global' },
      { keys: ['Ctrl', 'Shift', 'B'], descKey: 'scPanelBrowser', scope: 'global' },
    ],
  },
  {
    titleKey: 'scFileExplorer',
    items: [
      { keys: ['↑ / ↓'], descKey: 'scFileNavigate', scope: 'file-explorer' },
      { keys: ['→'], descKey: 'scFileExpand', scope: 'file-explorer' },
      { keys: ['←'], descKey: 'scFileCollapse', scope: 'file-explorer' },
      { keys: ['F2'], descKey: 'scRenameFileFolder', scope: 'file-explorer' },
      { keys: ['Esc'], descKey: 'scCancelRenameCreate', scope: 'file-explorer' },
      { keys: ['Enter'], descKey: 'scConfirmRenameCreate', scope: 'file-explorer' },
    ],
  },
  {
    titleKey: 'scTerminal',
    items: [
      { keys: ['Ctrl', 'C'], descKey: 'scInterruptProcess', scope: 'terminal' },
      { keys: ['Ctrl', 'D'], descKey: 'scSendEOF', scope: 'terminal' },
      { keys: ['↑ / ↓'], descKey: 'scNavHistory', scope: 'terminal' },
      { keys: ['Ctrl', 'Shift', '`'], descKey: 'scNewTerminalTab', scope: 'global' },
      { keys: ['Ctrl', 'PageDown'], descKey: 'scNextTerminalTab', scope: 'global' },
      { keys: ['Ctrl', 'PageUp'], descKey: 'scPrevTerminalTab', scope: 'global' },
    ],
  },
  {
    titleKey: 'scFtp',
    items: [
      { keys: ['↑ / ↓'], descKey: 'scFtpNavigate', scope: 'ftp' },
      { keys: ['Espacio'], descKey: 'scFtpToggleSelect', scope: 'ftp' },
      { keys: ['Ctrl', 'A'], descKey: 'scFtpSelectAll', scope: 'ftp' },
      { keys: ['Enter'], descKey: 'scFtpOpenEnter', scope: 'ftp' },
      { keys: ['Backspace'], descKey: 'scFtpGoBack', scope: 'ftp' },
      { keys: ['Esc'], descKey: 'scFtpDeselect', scope: 'ftp' },
      { keys: ['U'], descKey: 'scFtpUpload', scope: 'ftp' },
      { keys: ['D'], descKey: 'scFtpDownload', scope: 'ftp' },
      { keys: ['Supr'], descKey: 'scFtpDelete', scope: 'ftp' },
      { keys: ['F2'], descKey: 'scFtpRename', scope: 'ftp' },
    ],
  },
  {
    titleKey: 'scErDiagram',
    items: [
      { keys: ['Ctrl', '+'], descKey: 'scErZoomIn', scope: 'er-diagram' },
      { keys: ['Ctrl', '-'], descKey: 'scErZoomOut', scope: 'er-diagram' },
      { keys: ['Ctrl', '0'], descKey: 'scErResetZoom', scope: 'er-diagram' },
    ],
  },
  {
    titleKey: 'scDesignGroup',
    items: [
      { keys: ['Supr'], descKey: 'scDesignDelete', scope: 'design-canvas' },
      { keys: ['Shift', '⇧ arrastrar'], descKey: 'scDesignBoxSelect', scope: 'design-canvas' },
      { keys: ['Ctrl', 'clic'], descKey: 'scDesignMultiSelect', scope: 'design-canvas' },
    ],
  },
]

function comboKey(keys: string[]): string {
  return keys.join('+').toLowerCase()
}

// Dev-time check (not called anywhere in the UI) — flags any combo shared
// between two 'global' entries, or between a 'global' entry and any scoped
// one (since global always listens on top of whatever's focused).
export function findShortcutConflicts(): string[] {
  const globalCombos = new Map<string, string>() // combo -> titleKey
  const problems: string[] = []

  for (const g of SHORTCUT_GROUPS) {
    for (const item of g.items) {
      if (item.scope !== 'global') continue
      const combo = comboKey(item.keys)
      if (globalCombos.has(combo)) problems.push(`${combo}: ${globalCombos.get(combo)} vs ${g.titleKey}`)
      else globalCombos.set(combo, g.titleKey)
    }
  }
  for (const g of SHORTCUT_GROUPS) {
    for (const item of g.items) {
      if (item.scope === 'global') continue
      const combo = comboKey(item.keys)
      if (globalCombos.has(combo)) problems.push(`${combo}: global (${globalCombos.get(combo)}) vs scoped (${g.titleKey})`)
    }
  }
  return problems
}
