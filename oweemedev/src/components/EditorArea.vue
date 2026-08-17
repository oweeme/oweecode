<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount, nextTick } from 'vue'
const emit = defineEmits<{ (e: 'save-api-request', req: any): void }>()
import { invoke } from '@tauri-apps/api/core'
import {
  EditorView, keymap, lineNumbers, highlightActiveLineGutter,
  drawSelection, rectangularSelection, highlightActiveLine,
  showTooltip, type Tooltip, type ViewUpdate
} from '@codemirror/view'
import { EditorState as CMState, Compartment, StateField, StateEffect } from '@codemirror/state'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import {
  indentOnInput, syntaxHighlighting, defaultHighlightStyle,
  bracketMatching, foldGutter, foldKeymap, indentUnit
} from '@codemirror/language'
import { search, searchKeymap, highlightSelectionMatches, openSearchPanel, closeSearchPanel, searchPanelOpen, findNext, findPrevious } from '@codemirror/search'
import { autocompletion, completionKeymap, closeBrackets, closeBracketsKeymap, type CompletionContext, type CompletionResult, type Completion } from '@codemirror/autocomplete'
import { lintKeymap, linter, lintGutter, type Diagnostic } from '@codemirror/lint'
import { oneDark } from '@codemirror/theme-one-dark'
import { javascript } from '@codemirror/lang-javascript'
import { html } from '@codemirror/lang-html'
import { php } from '@codemirror/lang-php'
import { cpp } from '@codemirror/lang-cpp'
import { go } from '@codemirror/lang-go'
import { python } from '@codemirror/lang-python'
import { rust } from '@codemirror/lang-rust'
import { vue } from '@codemirror/lang-vue'
import { sql } from '@codemirror/lang-sql'
import { markdown } from '@codemirror/lang-markdown'
import { css } from '@codemirror/lang-css'
import { StreamLanguage } from '@codemirror/language'
import { shell } from '@codemirror/legacy-modes/mode/shell'
import { json as jsonMode } from '@codemirror/legacy-modes/mode/javascript'
import { yaml } from '@codemirror/legacy-modes/mode/yaml'
import { toml } from '@codemirror/legacy-modes/mode/toml'
import { xml } from '@codemirror/legacy-modes/mode/xml'
import { dart } from '@codemirror/legacy-modes/mode/clike'
import { jinja2 } from '@codemirror/legacy-modes/mode/jinja2'
import { svelte } from '@replit/codemirror-lang-svelte'
import { vim } from '@replit/codemirror-vim'
import { adaLanguage } from '../composables/adaLanguage'
import { astroLanguage } from '../composables/astroLanguage'
import { useEditorStore } from '../composables/useEditorStore'
import { useFtpStore } from '../composables/useFtpStore'
import { useI18n, type Locale } from '../composables/useI18n'
import { useAppTheme } from '../composables/useAppTheme'
import { lightCmTheme } from '../composables/editorThemes'
import ImageViewer from './ImageViewer.vue'
import DatabaseView from './DatabaseView.vue'
import FtpView from './FtpView.vue'
import RedisView from './RedisView.vue'
import ApiRequestTab from './ApiRequestTab.vue'
import CliPanel from './CliPanel.vue'
import AgentTab from './AgentTab.vue'
import GitDiffView from './GitDiffView.vue'
import ErDiagramView from './ErDiagramView.vue'
import ContainerLogsView from './ContainerLogsView.vue'
import ContainerShellView from './ContainerShellView.vue'
import BrowserView from './BrowserView.vue'
import DesignView from './DesignView.vue'

const store = useEditorStore()
const { connectedIds: ftpConnectedIds } = useFtpStore()
const { t, setLocale, locale } = useI18n()
const { theme: appTheme } = useAppTheme()
const editorEl = ref<HTMLElement | null>(null)

function cmThemeFor(t: string) {
  return t === 'white' ? lightCmTheme() : oneDark
}

watch(appTheme, (t) => {
  view?.dispatch({ effects: themeCompartment.reconfigure(cmThemeFor(t)) })
})

const LOCALES: { code: Locale; flag: string; label: string }[] = [
  { code: 'en', flag: '🇺🇸', label: 'English' },
  { code: 'es', flag: '🇧🇴', label: 'Español' },
  { code: 'de', flag: '🇩🇪', label: 'Deutsch' },
  { code: 'fr', flag: '🇫🇷', label: 'Français' },
  { code: 'ko', flag: '🇰🇷', label: '한국어' },
  { code: 'zh', flag: '🇨🇳', label: '中文' },
  { code: 'ja', flag: '🇯🇵', label: '日本語' },
  { code: 'ru', flag: '🇷🇺', label: 'Русский' },
]

let view: EditorView | null = null
const langCompartment = new Compartment()
const wrapCompartment = new Compartment()
const fontSizeCompartment = new Compartment()
const fontFamilyCompartment = new Compartment()
const lineHeightCompartment = new Compartment()
const tabSizeCompartment = new Compartment()
const lintCompartment = new Compartment()
const themeCompartment = new Compartment()
const vimCompartment = new Compartment()

const LINTABLE_LANGS = new Set(['python', 'javascript', 'typescript', 'php', 'rust'])

interface LintError { line: number; col: number; end_line: number; end_col: number; message: string; severity: string }

function createFileLinter(language: string) {
  return linter(async (editorView) => {
    const content = editorView.state.doc.toString()
    try {
      const errors = await invoke<LintError[]>('lint_file', { language, content })
      return errors.map((e: LintError): Diagnostic => {
        const doc = editorView.state.doc
        const maxLine = doc.lines
        const ln = Math.min(e.line + 1, maxLine)
        const fromLine = doc.line(ln)
        const from = fromLine.from + Math.min(e.col, fromLine.length)
        const endLn = Math.min(e.end_line + 1, maxLine)
        const toLine = doc.line(endLn)
        const to = Math.max(from + 1, toLine.from + Math.min(e.end_col, toLine.length))
        return { from, to, severity: (e.severity || 'error') as 'error' | 'warning' | 'info', message: e.message, source: language }
      })
    } catch { return [] }
  }, { delay: 800 })
}

// ── LSP ───────────────────────────────────────────────────────────────────────
const lspCompartment      = new Compartment()
const completionCompartment = new Compartment()
const LSP_LANGS = new Set(['typescript','javascript','jsx','tsx','vue','go','rust','php','python'])

// CodeMirror's own syntax highlighting needs 'jsx'/'tsx' as distinct language
// ids (so the JS grammar knows to expect JSX tags — see getLangExtension
// below), but typescript-language-server only knows 'typescript'/'javascript'
// — the same server handles .tsx/.jsx fine, it just wants that language id.
// Without this mapping, LSP silently never started for any React/Next.js/
// Qwik/Solid file (lsp_binary('jsx') matches nothing on the Rust side).
function lspLanguageId(lang: string): string {
  if (lang === 'tsx') return 'typescript'
  if (lang === 'jsx') return 'javascript'
  return lang
}

function lspEnabled(lang: string): boolean {
  return localStorage.getItem(`lsp_enabled_${lspLanguageId(lang)}`) === 'true'
}

const lspError = ref('')

interface LspItem { label: string; kind?: number; detail?: string; insert_text?: string; edit_start_col?: number; edit_end_col?: number }
interface LspDiag { message: string; severity: number; start_line: number; start_col: number; end_line: number; end_col: number }

const CM_KIND: Record<number, string> = {
  2:'method', 3:'function', 4:'function', 5:'property', 6:'variable',
  7:'class', 8:'interface', 9:'namespace', 10:'property', 14:'keyword',
  15:'text', 21:'constant',
}

function createLspCompletionSource(language: string, filePath: string) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    const word = ctx.matchBefore(/[\w$]+/)
    // Right after a member-access dot there's no word to match, but that's
    // exactly the highest-value completion moment (object. -> methods/props).
    const prevChar = ctx.state.sliceDoc(Math.max(0, ctx.pos - 1), ctx.pos)
    if (!word && prevChar !== '.' && !ctx.explicit) return null
    const content = ctx.state.doc.toString()
    const lineObj = ctx.state.doc.lineAt(ctx.pos)
    try {
      const items = await invoke<LspItem[]>('lsp_complete', {
        language, filePath, content,
        line: lineObj.number - 1,
        col: ctx.pos - lineObj.from,
      })
      if (!items.length) return null
      return {
        from: word ? word.from : ctx.pos,
        options: items.map(i => {
          const hasEdit = i.edit_start_col != null && i.edit_end_col != null
          const editFrom = hasEdit ? lineObj.from + i.edit_start_col! : 0
          return {
            label: i.label,
            type: i.kind ? (CM_KIND[i.kind] ?? 'text') : 'text',
            detail: i.detail,
            boost: 2,
            // The server's textEdit.range is authoritative for *where the edit
            // starts* — for completions right after a `.` it often starts
            // before the dot itself (replacing it), which CodeMirror's own
            // guessed `from` can't know. But its *end* was only valid at the
            // moment we asked the server, which is stale by the time the user
            // actually accepts (validFor lets them keep typing without
            // re-querying) — using that stale end left already-typed
            // characters in place and appended on top, producing "apipi" /
            // "api.getget". The `to` CodeMirror passes to apply() already
            // tracks the current, live end of the typed token, so use that
            // instead of the server's (by-then-stale) end position.
            apply: hasEdit
              ? (view: EditorView, _completion: Completion, _from: number, to: number) => {
                  const insertText = i.insert_text ?? i.label
                  view.dispatch({
                    changes: { from: editFrom, to, insert: insertText },
                    selection: { anchor: editFrom + insertText.length },
                  })
                }
              : (i.insert_text || i.label),
          } satisfies Completion
        }),
        validFor: /^\w*$/,
      }
    } catch (e) {
      lspError.value = `lsp_complete: ${String(e)}`
      return null
    }
  }
}

function createLspLinter(language: string, filePath: string) {
  return linter(async (editorView) => {
    try {
      const diags = await invoke<LspDiag[]>('lsp_diagnostics', { language, filePath })
      const doc = editorView.state.doc
      return diags.map((d): Diagnostic => {
        const fromLine = doc.line(Math.min(d.start_line + 1, doc.lines))
        const toLine   = doc.line(Math.min(d.end_line + 1, doc.lines))
        const from = fromLine.from + Math.min(d.start_col, fromLine.length)
        const to   = Math.max(from + 1, toLine.from + Math.min(d.end_col, toLine.length))
        const sev  = d.severity === 1 ? 'error' : d.severity === 2 ? 'warning' : 'info'
        return { from, to, severity: sev as 'error' | 'warning' | 'info', message: d.message, source: `lsp:${language}` }
      })
    } catch { return [] }
  }, { delay: 1200 })
}

// ── Signature help: shows a function/method's parameter list while the user
// is typing a call, e.g. `api.get(|` -> "get(url: string, config?: AxiosRequestConfig)"
// with the active parameter bolded. A separate LSP feature from completion —
// CodeMirror has no built-in widget for it, so it's a small custom tooltip.
interface LspSigParam { label: string }
interface LspSignature { label: string; documentation?: string; parameters: LspSigParam[] }
interface LspSignatureHelpResult { signatures: LspSignature[]; active_signature: number; active_parameter: number }

const setSigHelpTooltip = StateEffect.define<Tooltip | null>()
const sigHelpField = StateField.define<Tooltip | null>({
  create: () => null,
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setSigHelpTooltip)) return e.value
    return tr.docChanged ? null : value
  },
  provide: f => showTooltip.from(f),
})

function renderSigLabel(sig: LspSignature, activeParam: number): DocumentFragment {
  const frag = document.createDocumentFragment()
  const param = sig.parameters[activeParam]
  const idx = param ? sig.label.indexOf(param.label) : -1
  if (!param || idx === -1) {
    frag.append(document.createTextNode(sig.label))
    return frag
  }
  frag.append(document.createTextNode(sig.label.slice(0, idx)))
  const b = document.createElement('b')
  b.className = 'cm-sig-help-active-param'
  b.textContent = param.label
  frag.append(b)
  frag.append(document.createTextNode(sig.label.slice(idx + param.label.length)))
  return frag
}

function buildSigHelpTooltip(pos: number, result: LspSignatureHelpResult): Tooltip {
  return {
    pos,
    above: true,
    create: () => {
      const dom = document.createElement('div')
      dom.className = 'cm-sig-help'
      const sig = result.signatures[result.active_signature] ?? result.signatures[0]
      const label = document.createElement('div')
      label.className = 'cm-sig-help-label'
      label.appendChild(renderSigLabel(sig, result.active_parameter))
      dom.appendChild(label)
      if (sig.documentation) {
        const doc = document.createElement('div')
        doc.className = 'cm-sig-help-doc'
        doc.textContent = sig.documentation
        dom.appendChild(doc)
      }
      return { dom }
    },
  }
}

// Triggered from the editor's updateListener on every doc change: fires a
// signatureHelp request right after `(` or `,` (standard trigger characters
// for "you're inside a call now"), and clears the tooltip on `)`.
function maybeTriggerSignatureHelp(language: string, filePath: string, update: ViewUpdate) {
  const pos = update.state.selection.main.head
  const prevChar = update.state.sliceDoc(Math.max(0, pos - 1), pos)
  if (prevChar === ')') {
    view?.dispatch({ effects: setSigHelpTooltip.of(null) })
    return
  }
  if (prevChar !== '(' && prevChar !== ',') return

  const lspLang = lspLanguageId(language)
  const content = update.state.doc.toString()
  const lineObj = update.state.doc.lineAt(pos)
  invoke<LspSignatureHelpResult | null>('lsp_signature_help', {
    language: lspLang, filePath, content,
    line: lineObj.number - 1, col: pos - lineObj.from,
  }).then(result => {
    const tooltip = result && result.signatures.length ? buildSigHelpTooltip(pos, result) : null
    view?.dispatch({ effects: setSigHelpTooltip.of(tooltip) })
  }).catch(() => {})
}

const isWordWrap = ref(localStorage.getItem('editor_word_wrap') === 'true')
const fontSize = ref(parseFloat(localStorage.getItem('editor_font_size') ?? '13.5'))
const fontFamily = ref(localStorage.getItem('editor_font_family') ?? 'JetBrains Mono')
const lineHeight = ref(parseFloat(localStorage.getItem('editor_line_height') ?? '1.6'))
const tabSize = ref(parseInt(localStorage.getItem('editor_tab_size') ?? '2'))
const autoSave = ref(localStorage.getItem('editor_auto_save') === 'true')
const isVimMode = ref(localStorage.getItem('editor_vim_mode') === 'true')

function changeFontSize(delta: number) {
  if (Math.abs(delta) > 100) { fontSize.value = 13.5 } // reset sentinel
  else if (delta !== 0) fontSize.value = Math.max(9, Math.min(32, fontSize.value + delta))
  localStorage.setItem('editor_font_size', String(fontSize.value))
  view?.dispatch({
    effects: fontSizeCompartment.reconfigure(
      EditorView.theme({ '&': { fontSize: fontSize.value + 'px' } })
    )
  })
}

function getLangExtension(lang: string) {
  switch (lang) {
    case 'javascript': return javascript()
    case 'typescript': return javascript({ typescript: true })
    // Split from plain js/ts: without jsx:true the JS/TS grammar doesn't
    // understand JSX tags at all, which is what React/Next.js/Qwik/Solid
    // .jsx/.tsx files actually are — this was silently mis-highlighting them.
    case 'jsx':        return javascript({ jsx: true })
    case 'tsx':        return javascript({ jsx: true, typescript: true })
    case 'html':       return html()
    case 'vue':        return vue()
    case 'svelte':     return svelte()
    case 'php':        return php()
    case 'cpp':        return cpp()
    case 'go':         return go()
    case 'python':     return python()
    case 'rust':       return rust()
    case 'sql':        return sql()
    case 'markdown':   return markdown()
    case 'css':        return css()
    case 'shell':      return StreamLanguage.define(shell)
    case 'json':       return StreamLanguage.define(jsonMode)
    case 'yaml':       return StreamLanguage.define(yaml)
    case 'toml':       return StreamLanguage.define(toml)
    case 'xml':        return StreamLanguage.define(xml)
    case 'dart':       return StreamLanguage.define(dart)
    case 'ada':        return StreamLanguage.define(adaLanguage)
    case 'handlebars': return StreamLanguage.define(jinja2)
    case 'astro':      return StreamLanguage.define(astroLanguage)
    default:           return []
  }
}

function toggleWordWrap() {
  isWordWrap.value = !isWordWrap.value
  view?.dispatch({ effects: wrapCompartment.reconfigure(isWordWrap.value ? EditorView.lineWrapping : []) })
}

function buildExtensions(lang: string) {
  return [
    // Must come before the keymap extensions below — codemirror-vim relies on
    // Prec.highest internally to intercept keys ahead of the normal bindings.
    vimCompartment.of(isVimMode.value ? [vim()] : []),
    lineNumbers(),
    highlightActiveLineGutter(),
    foldGutter(),
    lintGutter(),
    drawSelection(),
    rectangularSelection(),
    highlightActiveLine(),
    highlightSelectionMatches(),
    history(),
    indentOnInput(),
    bracketMatching(),
    closeBrackets(),
    completionCompartment.of(autocompletion({ activateOnTyping: true })),
    search({ top: true }),
    syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
    themeCompartment.of(cmThemeFor(appTheme.value)),
    wrapCompartment.of(isWordWrap.value ? EditorView.lineWrapping : []),
    fontSizeCompartment.of(EditorView.theme({ '&': { fontSize: fontSize.value + 'px' } })),
    fontFamilyCompartment.of(EditorView.theme({ '.cm-content, .cm-scroller': { fontFamily: `'${fontFamily.value}', monospace` } })),
    lineHeightCompartment.of(EditorView.theme({ '.cm-content': { lineHeight: String(lineHeight.value) } })),
    tabSizeCompartment.of(indentUnit.of(' '.repeat(tabSize.value))),
    lintCompartment.of([]),
    lspCompartment.of([]),
    sigHelpField,
    langCompartment.of(getLangExtension(lang)),
    keymap.of([
      { key: 'Mod-h', run: openSearchPanel },
      { key: 'F3',           run: findNext },
      { key: 'Shift-F3',     run: findPrevious },
      ...closeBracketsKeymap,
      ...defaultKeymap,
      ...searchKeymap,
      ...historyKeymap,
      ...foldKeymap,
      ...completionKeymap,
      ...lintKeymap,
      indentWithTab,
      { key: 'Mod-s',     run: () => { store.saveActiveFile(); return true } },
      { key: 'Alt-z',     run: () => { toggleWordWrap(); return true } },
      { key: 'Mod-Equal', run: () => { changeFontSize(1); return true } },
      { key: 'Mod-Plus',  run: () => { changeFontSize(1); return true } },
      { key: 'Mod-Minus', run: () => { changeFontSize(-1); return true } },
      { key: 'Mod-0',     run: () => { fontSize.value = 13.5; changeFontSize(0); return true } },
    ]),
    EditorView.updateListener.of(update => {
      if (update.docChanged) {
        const path = store.state.activeTabPath
        if (path) {
          store.updateContent(path, update.state.doc.toString())
          if (autoSave.value) store.saveFile(path)
        }
        const tab = store.activeTab()
        if (tab && path && LSP_LANGS.has(tab.language) && lspEnabled(tab.language) && !path.startsWith('ftpfile://')) {
          maybeTriggerSignatureHelp(tab.language, path, update)
        }
      }
      if (update.selectionSet) {
        const sel = update.state.selection.main
        const line = update.state.doc.lineAt(sel.head)
        store.setCursor(line.number, sel.head - line.from + 1)
        const selText = update.state.sliceDoc(sel.from, sel.to)
        store.setSelectedText(selText)
      }
    }),
    EditorView.theme({
      '&': { height: '100%', fontSize: '13.5px' },
      '.cm-scroller': { fontFamily: 'var(--font-mono)', overflow: 'auto' },
      '.cm-content': { padding: '6px 0', caretColor: '#528bff' },
      '.cm-cursor': { borderLeftColor: '#528bff', borderLeftWidth: '2px' },
      '.cm-gutters': {
        background: '#16171a',
        borderRight: '1px solid #25262b',
        color: '#44455a',
        userSelect: 'none',
      },
      '.cm-lineNumbers .cm-gutterElement': { minWidth: '48px', padding: '0 10px 0 6px', fontSize: '12px' },
      '.cm-foldGutter .cm-gutterElement': { padding: '0 4px', color: '#44455a' },
      '.cm-activeLineGutter': { background: '#1e1f23' },
      '.cm-activeLine': { background: '#1e1f2380' },
      '.cm-selectionBackground': { background: '#264f78 !important' },
      '&.cm-focused .cm-selectionBackground': { background: '#264f78 !important' },
    }),
  ]
}

function initEditor(content: string, lang: string) {
  if (view) { view.destroy(); view = null }
  if (!editorEl.value) return

  const state = CMState.create({
    doc: content,
    extensions: buildExtensions(lang),
  })
  view = new EditorView({ state, parent: editorEl.value })
  view.focus()

  const tab = store.activeTab()
  const filePath = tab?.path ?? ''

  // Activate OS-based linter (always for supported langs)
  if (LINTABLE_LANGS.has(lang)) {
    view.dispatch({ effects: lintCompartment.reconfigure(createFileLinter(lang)) })
  }

  // Activate LSP if enabled in preferences
  if (LSP_LANGS.has(lang) && lspEnabled(lang) && filePath && !filePath.startsWith('ftpfile://')) {
    const lspLang = lspLanguageId(lang)
    const rootPath = store.state.rootPath || '/'
    lspError.value = ''
    store.state.lspStatus = 'starting'
    // Starting the server (PATH resolution + spawn + waiting for its
    // "initialize" response) can take several seconds on a real project.
    // The completion/lint sources are only wired up AFTER that finishes —
    // wiring them immediately meant typing right after opening a file hit
    // lsp_complete before the session existed, which silently returns "no
    // results" (not an error), looking exactly like a broken LSP.
    invoke('lsp_start', { language: lspLang, rootPath }).then(() => {
      return invoke('lsp_notify_open', { language: lspLang, filePath, content })
    }).then(() => {
      // The FIRST completion request against a real project can take 30+
      // seconds — the server builds its whole type-analysis program on
      // demand, not just on didOpen. Firing one throwaway request now (while
      // the status badge still reads "starting") absorbs that one-time cost
      // up front, so the user's actual first keystroke gets a fast response
      // instead of silently returning nothing before the real work finished.
      return invoke('lsp_complete', { language: lspLang, filePath, content, line: 0, col: 0 }).catch(() => {})
    }).then(() => {
      view?.dispatch({ effects: completionCompartment.reconfigure(
        autocompletion({ activateOnTyping: true, override: [createLspCompletionSource(lspLang, filePath)] })
      ) })
      view?.dispatch({ effects: lspCompartment.reconfigure(createLspLinter(lspLang, filePath)) })
      store.state.lspStatus = 'ready'
    }).catch((e: any) => {
      // This used to fail silently — if the server binary genuinely isn't
      // installed (or wasn't found in PATH), the user had zero indication
      // completion/diagnostics just weren't running at all.
      lspError.value = String(e)
      store.state.lspStatus = 'error'
    })
  } else {
    store.state.lspStatus = 'idle'
  }
}

watch(
  () => store.state.activeTabPath,
  async (path) => {
    if (!path) { view?.destroy(); view = null; return }
    const tab = store.activeTab()
    if (!tab) return
    if (tab.type !== 'code' && tab.type !== 'ftp-file') { view?.destroy(); view = null; return }
    await nextTick()
    initEditor(tab.content, tab.language)
  }
)

function onPrefsChanged(e: Event) {
  const d = (e as CustomEvent).detail
  if (d.fontSize !== undefined) { fontSize.value = d.fontSize; changeFontSize(0) }
  if (d.wordWrap !== undefined) {
    isWordWrap.value = d.wordWrap
    view?.dispatch({ effects: wrapCompartment.reconfigure(isWordWrap.value ? EditorView.lineWrapping : []) })
  }
  if (d.fontFamily !== undefined) {
    fontFamily.value = d.fontFamily
    view?.dispatch({ effects: fontFamilyCompartment.reconfigure(
      EditorView.theme({ '.cm-content, .cm-scroller': { fontFamily: `'${d.fontFamily}', monospace` } })
    ) })
  }
  if (d.lineHeight !== undefined) {
    lineHeight.value = d.lineHeight
    view?.dispatch({ effects: lineHeightCompartment.reconfigure(
      EditorView.theme({ '.cm-content': { lineHeight: String(d.lineHeight) } })
    ) })
  }
  if (d.tabSize !== undefined) {
    tabSize.value = d.tabSize
    view?.dispatch({ effects: tabSizeCompartment.reconfigure(indentUnit.of(' '.repeat(d.tabSize))) })
  }
  if (d.autoSave !== undefined) autoSave.value = d.autoSave
  if (d.vimMode !== undefined) {
    isVimMode.value = d.vimMode
    view?.dispatch({ effects: vimCompartment.reconfigure(isVimMode.value ? [vim()] : []) })
  }
  // Re-init editor to pick up LSP changes
  if (d.lsp !== undefined) {
    const tab = store.activeTab()
    if (tab && (tab.type === 'code' || tab.type === 'ftp-file')) {
      initEditor(tab.content, tab.language)
    }
  }
}

function onEditorToggleSearch() {
  if (!view) return
  if (searchPanelOpen(view.state)) {
    closeSearchPanel(view)
  } else {
    openSearchPanel(view)
  }
  view.focus()
}

function onEditorEscape() {
  if (view && searchPanelOpen(view.state)) {
    closeSearchPanel(view)
    view.focus()
  }
}

onMounted(() => {
  window.addEventListener('prefs-changed', onPrefsChanged)
  window.addEventListener('editor-toggle-search', onEditorToggleSearch)
  window.addEventListener('editor-escape', onEditorEscape)
  const tab = store.activeTab()
  if (tab && (tab.type === 'code' || tab.type === 'ftp-file')) initEditor(tab.content, tab.language)
})

onBeforeUnmount(() => {
  window.removeEventListener('prefs-changed', onPrefsChanged)
  window.removeEventListener('editor-toggle-search', onEditorToggleSearch)
  window.removeEventListener('editor-escape', onEditorEscape)
  view?.destroy()
})

function closeTab(e: MouseEvent, path: string) {
  e.stopPropagation()
  store.closeTab(path)
}

const langColors: Record<string, string> = {
  typescript: '#3178c6', javascript: '#f7df1e', vue: '#42b883',
  php: '#8892be', go: '#00add8', python: '#3572a5', rust: '#ce422b',
  html: '#e44d26', css: '#264de4', sql: '#e38c00', markdown: '#aaa',
  cpp: '#00599c', json: '#cbcb41', yaml: '#cbcb41', toml: '#9c4221',
  xml: '#e37933', shell: '#89e051', text: '#888', ada: '#02f88c',
}

function tabDotColor(lang: string) {
  return langColors[lang] ?? '#888'
}

// FTP tabs used to show the same dot as everything else — the generic
// per-language color swatch, which for an ftp tab (language: 'text') lands on
// a muted gray that reads as "disconnected" even when it's actively
// connected. Show the real connection state there instead.
function ftpTabDotColor(connId: string | undefined) {
  return connId && ftpConnectedIds.value.has(connId) ? '#3fb950' : '#6e7681'
}

// Every distinct CLI tool that has an open tab — each gets ONE persistent
// CliPanel instance (v-show toggles visibility) so switching away from a CLI
// tab no longer kills its running process / conversation.
const openCliTools = computed(() =>
  [...new Set(store.state.tabs.filter(t => t.type === 'cli').map(t => t.cliTool!))]
)

// Same reasoning as openCliTools — an interactive container shell is a live
// PTY session (cwd, running processes), so switching tabs away and back must
// not tear it down and reconnect from scratch.
const openContainerShells = computed(() =>
  store.state.tabs.filter(t => t.type === 'container-shell').map(t => ({ id: t.containerId!, name: t.containerName! }))
)

// Every open browser tab, kept mounted the same way (see BrowserView below).
const openBrowserTabs = computed(() => store.state.tabs.filter(t => t.type === 'browser'))

</script>

<template>
  <div class="editor-area">

    <!-- LSP start failure — used to fail silently, leaving no clue that
         completion/diagnostics just weren't running -->
    <div v-if="lspError" class="lsp-error-banner">
      <span>{{ t('lspStartErrorPrefix') }} {{ lspError }}</span>
      <button @click="lspError = ''">×</button>
    </div>

    <!-- Welcome screen -->
    <div v-if="store.state.tabs.length === 0" class="welcome">
      <div class="welcome-inner">
        <div class="welcome-logo">
          <img src="/oweedev.png" width="96" height="96" style="border-radius:20px;display:block;" />
        </div>
        <h1 class="welcome-title">OweemeIDE</h1>
        <p class="welcome-sub">{{ t('welcomeSub') }}</p>

        <div class="welcome-langs">
          <span class="lang-badge" style="color:#f7df1e">JS</span>
          <span class="lang-badge" style="color:#3178c6">TS</span>
          <span class="lang-badge" style="color:#42b883">Vue</span>
          <span class="lang-badge" style="color:#8892be">PHP</span>
          <span class="lang-badge" style="color:#00add8">Go</span>
          <span class="lang-badge" style="color:#3572a5">Python</span>
          <span class="lang-badge" style="color:#ce422b">Rust</span>
          <span class="lang-badge" style="color:#e44d26">HTML</span>
          <span class="lang-badge" style="color:#264de4">CSS</span>
          <span class="lang-badge" style="color:#e38c00">SQL</span>
          <span class="lang-badge" style="color:#00599c">C++</span>
          <span class="lang-badge" style="color:#00b4ab">Dart</span>
          <span class="lang-badge" style="color:#02f88c">Ada</span>
        </div>

        <div class="welcome-tips">
          <div class="tip"><kbd>Ctrl+S</kbd> {{ t('tipSave') }}</div>
          <div class="tip"><kbd>Ctrl+Z</kbd> {{ t('tipUndo') }}</div>
          <div class="tip"><kbd>Ctrl+F</kbd> {{ t('tipFind') }}</div>
          <div class="tip"><kbd>Tab</kbd> {{ t('tipIndent') }}</div>
        </div>

        <p class="welcome-hint">{{ t('welcomeHint') }}</p>

        <!-- Language switcher card -->
        <div class="welcome-lang-card">
          <div class="welcome-lang-card-title">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="12" cy="12" r="10"/><path d="M2 12h20M12 2a15.3 15.3 0 010 20M12 2a15.3 15.3 0 000 20"/>
            </svg>
            {{ t('prefsLanguage') }}
          </div>
          <div class="welcome-locale-row">
            <button
              v-for="loc in LOCALES"
              :key="loc.code"
              class="welcome-locale-btn"
              :class="{ active: locale === loc.code }"
              @click="setLocale(loc.code)"
              :title="loc.label"
            >
              <span class="welcome-locale-flag">{{ loc.flag }}</span>
              <span class="welcome-locale-name">{{ loc.label }}</span>
            </button>
          </div>
          <p class="welcome-lang-hint">
            ⚙ {{ t('settingsPrefs') }} → {{ t('prefsLanguage') }} → {{ t('prefsApply') }}
          </p>
        </div>
      </div>
    </div>

    <!-- Tab bar -->
    <div v-else class="tab-bar">
      <div
        v-for="tab in store.state.tabs"
        :key="tab.path"
        class="tab"
        :class="{ 'tab--active': tab.path === store.state.activeTabPath }"
        @click="store.setActive(tab.path)"
        :title="tab.path"
      >
        <span
          class="tab-lang-dot"
          :title="tab.type === 'ftp' ? (ftpConnectedIds.has(tab.ftpConnId!) ? t('connected') : t('disconnectedTitle')) : undefined"
          :style="{ background: tab.type === 'ftp' ? ftpTabDotColor(tab.ftpConnId) : tabDotColor(tab.language) }"
        />
        <span class="tab-name">{{ tab.name }}</span>
        <span v-if="tab.type === 'ftp-file'" class="tab-badge-ftp" :title="t('ftpSaveHint')">{{ t('ftpBadge') }}</span>
        <span v-if="tab.modified" class="tab-modified">●</span>
        <button class="tab-close" @click="closeTab($event, tab.path)" title="Close">×</button>
      </div>
      <div class="tab-bar-spacer" />
      <div class="zoom-group">
        <button class="zoom-btn" @click="changeFontSize(-1)" title="Zoom Out (Ctrl+-)">−</button>
        <span class="zoom-label" @click="changeFontSize(-999)" title="Reset zoom (Ctrl+0)">{{ fontSize.toFixed(0) }}px</span>
        <button class="zoom-btn" @click="changeFontSize(1)" title="Zoom In (Ctrl+=)">+</button>
      </div>
      <button
        class="wrap-btn"
        :class="{ active: isWordWrap }"
        @click="toggleWordWrap"
        title="Toggle Word Wrap (Alt+Z)"
      >{{ t('wrapBtn') }}</button>
    </div>

    <!-- Image viewer -->
    <ImageViewer
      v-if="store.activeTab()?.type === 'image'"
      :path="store.activeTab()!.path"
      :name="store.activeTab()!.name"
    />

    <!-- FTP/SFTP view -->
    <FtpView
      v-else-if="store.activeTab()?.type === 'ftp'"
      :key="store.activeTab()!.path"
      :conn-id="store.activeTab()!.ftpConnId!"
      :conn-name="store.activeTab()!.ftpConnName!"
      :protocol="store.activeTab()!.ftpProtocol ?? 'sftp'"
    />

    <!-- Database view -->
    <DatabaseView
      v-else-if="store.activeTab()?.type === 'database'"
      :key="store.activeTab()!.path"
      :conn-id="store.activeTab()!.connId!"
      :table-name="store.activeTab()!.tableName!"
      :driver="store.activeTab()!.driver ?? 'mysql'"
    />

    <!-- Redis view -->
    <RedisView
      v-else-if="store.activeTab()?.type === 'redis'"
      :key="store.activeTab()!.path"
      :conn-id="store.activeTab()!.redisConnId!"
      :conn-name="store.activeTab()!.redisConnName!"
    />

    <!-- Git diff view -->
    <GitDiffView
      v-else-if="store.activeTab()?.type === 'git-diff'"
      :key="store.activeTab()!.path"
      :file-path="store.activeTab()!.gitFilePath!"
      :staged="store.activeTab()!.gitStaged ?? false"
    />

    <!-- ER diagram view -->
    <ErDiagramView
      v-else-if="store.activeTab()?.type === 'er-diagram'"
      :key="store.activeTab()!.path"
      :conn-id="store.activeTab()!.erConnId!"
      :conn-name="store.activeTab()!.erConnName!"
      :driver="store.activeTab()!.erDriver ?? 'mysql'"
    />

    <!-- Container logs view -->
    <ContainerLogsView
      v-else-if="store.activeTab()?.type === 'container-logs'"
      :key="store.activeTab()!.path"
      :container-id="store.activeTab()!.containerId!"
      :container-name="store.activeTab()!.containerName!"
    />


    <!-- Visual design / page builder -->
    <DesignView
      v-else-if="store.activeTab()?.type === 'design'"
      :key="store.activeTab()!.path"
    />

    <!-- API Request tab -->
    <ApiRequestTab
      v-if="store.activeTab()?.type === 'api'"
      :key="store.activeTab()!.path"
      :tab-path="store.activeTab()!.path"
      :initial-json="store.activeTab()!.content"
      @update-content="(c) => store.updateContent(store.activeTab()!.path, c)"
      @save-request="(req) => emit('save-api-request', req)"
    />

    <!-- AI CLI tabs — one persistent instance per open tool; v-show only
         toggles visibility so switching tabs never kills the running CLI. -->
    <CliPanel
      v-for="toolId in openCliTools"
      :key="`cli-tab-${toolId}`"
      v-show="store.activeTab()?.type === 'cli' && store.activeTab()?.cliTool === toolId"
      :cli="toolId"
      @open-file="store.openFile($event)"
    />

    <!-- Container interactive shells — one persistent instance per open
         container, same v-show-only pattern as the CLI tabs. -->
    <ContainerShellView
      v-for="cs in openContainerShells"
      :key="`container-shell-${cs.id}`"
      v-show="store.activeTab()?.type === 'container-shell' && store.activeTab()?.containerId === cs.id"
      :container-id="cs.id"
      :container-name="cs.name"
    />

    <!-- OweemeIDE local agent (Ollama) — single persistent instance, same
         v-show-only pattern as the CLI tabs so an in-progress task keeps
         running while the user looks at other tabs. -->
    <AgentTab
      v-if="store.state.tabs.some(t => t.type === 'agent')"
      v-show="store.activeTab()?.type === 'agent'"
    />

    <!-- Embedded browser / responsive preview — one persistent instance per
         open tab (v-show only), same reasoning as the CLI tabs above: without
         this, switching away to another workspace tab destroyed the iframe,
         losing in-page navigation/scroll/form state and reverting the address
         bar back to whatever URL the tab was created with. -->
    <BrowserView
      v-for="tab in openBrowserTabs"
      :key="tab.path"
      v-show="store.state.activeTabPath === tab.path"
      :initial-url="tab.browserUrl ?? 'http://localhost:3000'"
      :path="tab.path"
    />

    <!-- Code editor host (also for ftp-file tabs) -->
    <div
      v-show="store.state.tabs.length > 0 && (store.activeTab()?.type === 'code' || store.activeTab()?.type === 'ftp-file')"
      ref="editorEl"
      class="cm-host"
    />
  </div>
</template>

<style scoped>
.editor-area {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.lsp-error-banner {
  display: flex; align-items: center; justify-content: space-between; gap: 10px;
  padding: 6px 12px; font-size: 12px; flex-shrink: 0;
  background: rgba(248,81,73,0.12); color: #f85149; border-bottom: 1px solid rgba(248,81,73,0.25);
}
.lsp-error-banner button {
  background: none; border: none; color: inherit; font-size: 15px; cursor: pointer;
  line-height: 1; padding: 0 4px;
}

/* Welcome */
.welcome {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--bg-mid);
}
.welcome-inner {
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 14px;
  padding: 32px;
}
.welcome-logo { margin-bottom: 2px; }
.welcome-title {
  font-family: var(--font-ui);
  font-size: 32px;
  font-weight: 800;
  color: var(--fg-bright);
  letter-spacing: -1px;
}
.welcome-sub {
  font-size: 13px;
  color: var(--fg-muted);
  font-family: var(--font-ui);
}
.welcome-langs {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  justify-content: center;
  margin: 4px 0;
}
.lang-badge {
  font-family: var(--font-mono);
  font-size: 11px;
  font-weight: 700;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 2px 8px;
}
.welcome-tips {
  display: flex;
  gap: 16px;
  flex-wrap: wrap;
  justify-content: center;
  margin-top: 4px;
}
.tip {
  font-size: 12px;
  color: var(--fg-dim);
  display: flex;
  align-items: center;
  gap: 6px;
  font-family: var(--font-ui);
}
kbd {
  background: var(--bg-active);
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 1px 7px;
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--fg);
}
.welcome-hint {
  font-size: 12px;
  color: var(--fg-muted);
  margin-top: 8px;
  font-style: italic;
}

/* Language switcher card */
.welcome-lang-card {
  margin-top: 18px;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 14px 16px 12px;
  width: 100%;
  max-width: 560px;
}
.welcome-lang-card-title {
  display: flex; align-items: center; gap: 6px;
  font-size: 11px; font-weight: 700; color: var(--fg-muted);
  letter-spacing: 0.8px; text-transform: uppercase;
  margin-bottom: 10px; font-family: var(--font-ui);
}
.welcome-locale-row {
  display: flex; flex-wrap: wrap; gap: 6px;
  justify-content: center; margin-bottom: 10px;
}
.welcome-locale-btn {
  display: flex; flex-direction: column; align-items: center; gap: 2px;
  background: var(--bg-mid); border: 1px solid var(--border);
  border-radius: 8px; padding: 6px 10px; cursor: pointer;
  transition: all 0.15s; min-width: 58px;
}
.welcome-locale-btn:hover {
  background: var(--bg-hover); border-color: var(--fg-muted);
}
.welcome-locale-btn.active {
  background: rgba(208,208,216,0.15); border-color: var(--accent);
}
.welcome-locale-flag { font-size: 20px; line-height: 1.2; }
.welcome-locale-name {
  font-size: 10px; color: var(--fg-dim); font-family: var(--font-ui);
  white-space: nowrap;
}
.welcome-locale-btn.active .welcome-locale-name { color: var(--accent); font-weight: 700; }
.welcome-lang-hint {
  font-size: 10.5px; color: var(--fg-muted);
  text-align: center; font-family: var(--font-ui);
  line-height: 1.4; margin: 0;
  padding-top: 8px; border-top: 1px solid var(--border);
}

/* Tabs */
.tab-bar {
  display: flex;
  align-items: stretch;
  background: var(--bg-dark);
  border-bottom: 1px solid var(--border);
  overflow-x: auto;
  flex-shrink: 0;
  height: var(--tab-height);
}
.tab-bar::-webkit-scrollbar { height: 2px; }
.tab-bar::-webkit-scrollbar-thumb { background: var(--bg-active); border-radius: 2px; }
.tab-bar-spacer { flex: 1; }
.zoom-group {
  display: flex; align-items: center;
  border-left: 1px solid var(--border);
  border-right: 1px solid var(--border);
  height: 100%;
}
.zoom-btn {
  background: none; border: none;
  color: var(--fg-muted); font-size: 14px; font-weight: 700;
  width: 24px; height: 100%; cursor: pointer;
  display: flex; align-items: center; justify-content: center;
  transition: color 0.1s, background 0.1s;
}
.zoom-btn:hover { color: var(--fg); background: var(--bg-hover); }
.zoom-label {
  font-size: 10.5px; color: var(--fg-muted);
  padding: 0 4px; cursor: pointer; min-width: 30px; text-align: center;
  font-family: var(--font-mono);
}
.zoom-label:hover { color: var(--accent); }

.wrap-btn {
  background: none; border: none;
  color: var(--fg-muted); font-size: 11px; font-family: var(--font-ui);
  padding: 0 10px; cursor: pointer; white-space: nowrap;
  border-left: 1px solid var(--border);
  transition: color 0.12s, background 0.12s;
}
.wrap-btn:hover { color: var(--fg); background: var(--bg-hover); }
.wrap-btn.active { color: var(--accent); }

.tab {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 12px;
  min-width: 90px;
  max-width: 200px;
  cursor: pointer;
  border-right: 1px solid var(--border);
  font-family: var(--font-ui);
  font-size: 12.5px;
  color: var(--fg-dim);
  user-select: none;
  flex-shrink: 0;
  transition: background 0.12s, color 0.12s;
  position: relative;
}
.tab:hover { background: var(--bg-hover); color: var(--fg); }
.tab--active {
  background: var(--bg-mid);
  color: var(--fg-bright);
}
.tab--active::after {
  content: '';
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  height: 2px;
  background: var(--accent);
}

.tab-lang-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
  opacity: 0.85;
}
.tab-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tab-modified {
  color: var(--accent2);
  font-size: 14px;
  line-height: 1;
  flex-shrink: 0;
}
.tab-badge-ftp {
  font-size: 9px;
  font-weight: 700;
  background: rgba(137,180,250,0.15);
  color: #89b4fa;
  border: 1px solid rgba(137,180,250,0.3);
  border-radius: 3px;
  padding: 1px 4px;
  flex-shrink: 0;
  letter-spacing: 0.5px;
}
.tab-close {
  background: none;
  border: none;
  color: var(--fg-muted);
  cursor: pointer;
  font-size: 15px;
  line-height: 1;
  width: 18px;
  height: 18px;
  border-radius: 3px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  opacity: 0;
  transition: opacity 0.12s, background 0.12s, color 0.12s;
  padding: 0;
}
.tab:hover .tab-close { opacity: 1; }
.tab-close:hover { background: var(--bg-active); color: var(--red); }

/* Editor */
.cm-host {
  flex: 1;
  overflow: hidden;
  position: relative;
}
.cm-host :deep(.cm-editor) {
  height: 100%;
  outline: none;
}
.cm-host :deep(.cm-scroller) { overflow: auto; }

/* ── Search panel (VSCode-like) ─────────────────────────────── */
.cm-host :deep(.cm-search) {
  position: absolute;
  top: 0;
  right: 0;
  z-index: 20;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 8px 10px;
  background: #1e2028;
  border: 1px solid #3a3d4a;
  border-top: none;
  border-radius: 0 0 0 8px;
  box-shadow: 0 4px 20px rgba(0,0,0,0.5);
  min-width: 340px;
  font-family: var(--font-ui);
}

.cm-host :deep(.cm-search label) {
  display: flex;
  align-items: center;
  gap: 4px;
}

.cm-host :deep(.cm-search .cm-textfield) {
  flex: 1;
  background: #141618;
  border: 1px solid #3a3d4a;
  border-radius: 5px;
  color: #cdd6f4;
  font-size: 12.5px;
  font-family: var(--font-mono);
  padding: 4px 8px;
  outline: none;
  min-width: 180px;
  transition: border-color 0.12s;
}
.cm-host :deep(.cm-search .cm-textfield:focus) {
  border-color: var(--accent);
}

.cm-host :deep(.cm-search button) {
  background: #2a2d38;
  border: 1px solid #3a3d4a;
  border-radius: 4px;
  color: #9aa0b0;
  font-size: 11px;
  padding: 3px 7px;
  cursor: pointer;
  transition: all 0.1s;
  white-space: nowrap;
}
.cm-host :deep(.cm-search button:hover) {
  background: #35384a;
  color: #cdd6f4;
  border-color: #555;
}
.cm-host :deep(.cm-search button[name="close"]) {
  background: none;
  border: none;
  font-size: 16px;
  color: #6b7280;
  padding: 0 2px;
  line-height: 1;
  border-radius: 3px;
}
.cm-host :deep(.cm-search button[name="close"]:hover) {
  color: #cdd6f4;
  background: #35384a;
}
.cm-host :deep(.cm-search input[type="checkbox"]) {
  accent-color: var(--accent);
}
.cm-host :deep(.cm-search br) { display: none; }

/* ── Lint gutter markers ─────────────────────────────────────── */
.cm-host :deep(.cm-lint-marker) {
  width: 14px;
  height: 14px;
  cursor: pointer;
}
.cm-host :deep(.cm-lint-marker-error) { color: #f85149; }
.cm-host :deep(.cm-lint-marker-warning) { color: #e3b341; }
.cm-host :deep(.cm-lint-marker-info) { color: #79b8ff; }

/* ── Lint underlines ─────────────────────────────────────────── */
.cm-host :deep(.cm-diagnostic-error) {
  text-decoration: underline wavy #f85149;
  text-underline-offset: 2px;
}
.cm-host :deep(.cm-diagnostic-warning) {
  text-decoration: underline wavy #e3b341;
  text-underline-offset: 2px;
}

/* ── Lint tooltip panel ──────────────────────────────────────── */
.cm-host :deep(.cm-diagnosticText) {
  font-family: var(--font-ui);
  font-size: 12px;
  line-height: 1.5;
}
.cm-host :deep(.cm-tooltip-lint) {
  background: #1e2028;
  border: 1px solid #3a3d4a;
  border-radius: 6px;
  padding: 6px 10px;
  font-size: 12px;
  color: #cdd6f4;
  max-width: 400px;
  box-shadow: 0 4px 16px rgba(0,0,0,0.5);
}

/* ── Signature help (parameter hints) ────────────────────────── */
.cm-host :deep(.cm-sig-help) {
  background: #1e2028;
  border: 1px solid #3a3d4a;
  border-radius: 6px;
  padding: 6px 10px;
  font-family: var(--font-mono);
  font-size: 12px;
  color: #cdd6f4;
  max-width: 480px;
  box-shadow: 0 4px 16px rgba(0,0,0,0.5);
}
.cm-host :deep(.cm-sig-help-label) { white-space: pre-wrap; }
.cm-host :deep(.cm-sig-help-active-param) { color: var(--accent); font-weight: 700; }
.cm-host :deep(.cm-sig-help-doc) {
  margin-top: 4px; padding-top: 4px; border-top: 1px solid #3a3d4a;
  font-family: var(--font-ui); color: #9a9db3; font-size: 11.5px;
}

/* ── Autocomplete dropdown ───────────────────────────────────── */
.cm-host :deep(.cm-tooltip-autocomplete) {
  background: #1e2028;
  border: 1px solid #3a3d4a;
  border-radius: 6px;
  box-shadow: 0 8px 24px rgba(0,0,0,0.5);
  font-family: var(--font-mono);
  font-size: 12.5px;
  overflow: hidden;
}
.cm-host :deep(.cm-completionLabel) { color: #cdd6f4; }
.cm-host :deep(.cm-completionMatchedText) { color: var(--accent); font-weight: 700; text-decoration: none; }
.cm-host :deep(.cm-tooltip-autocomplete ul li[aria-selected]) {
  background: rgba(208,208,216,0.2);
  color: #e8eaf0;
}
.cm-host :deep(.cm-completionDetail) {
  color: #6b7280;
  font-size: 11px;
  margin-left: 6px;
}
.cm-host :deep(.cm-completionInfo) {
  background: #252830;
  border-left: 1px solid #3a3d4a;
  padding: 8px 12px;
  font-size: 11.5px;
  color: #9aa0b0;
  max-width: 300px;
}
</style>
