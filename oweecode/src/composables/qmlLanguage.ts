import type { StreamParser, StringStream } from '@codemirror/language'

// QML is not JavaScript (`Rectangle { width: 100 }`), so the JS grammar highlights it badly.
// This small hand-written parser covers what matters: imports/keywords, Type { } names,
// property names before `:`, onSignal handlers, strings, numbers, colors and comments.
const KEYWORDS = new Set([
  'import', 'as', 'property', 'readonly', 'required', 'default', 'signal', 'function', 'component', 'enum', 'pragma',
  'alias', 'on', 'if', 'else', 'for', 'while', 'do', 'switch', 'case', 'break', 'continue', 'return', 'var', 'let', 'const',
  'new', 'delete', 'typeof', 'in', 'of', 'try', 'catch', 'finally', 'throw', 'this', 'with', 'async', 'await',
])
const ATOMS = new Set(['true', 'false', 'null', 'undefined', 'parent', 'root', 'Qt', 'Math', 'console', 'JSON'])
const BUILTIN_TYPES = new Set(['int', 'real', 'double', 'string', 'bool', 'color', 'url', 'date', 'point', 'size', 'rect', 'variant', 'var', 'list', 'font'])

interface QmlState { inBlockComment: boolean; afterColon: boolean }

export const qmlLanguage: StreamParser<QmlState> = {
  startState: () => ({ inBlockComment: false, afterColon: false }),
  token(stream: StringStream, state: QmlState): string | null {
    if (state.inBlockComment) {
      if (stream.skipTo('*/')) { stream.match('*/'); state.inBlockComment = false } else stream.skipToEnd()
      return 'comment'
    }
    if (stream.sol()) state.afterColon = false
    if (stream.eatSpace()) return null
    if (stream.match('//')) { stream.skipToEnd(); return 'comment' }
    if (stream.match('/*')) { state.inBlockComment = true; return 'comment' }
    const q = stream.peek()
    if (q === '"' || q === "'" || q === '`') {
      stream.next(); let esc = false, c: string | void
      while ((c = stream.next()) != null) { if (c === q && !esc) break; esc = !esc && c === '\\' }
      return 'string'
    }
    if (stream.match(/^#[0-9a-fA-F]{3,8}\b/)) return 'number'
    if (stream.match(/^(0x[0-9a-fA-F]+|\d+\.?\d*([eE][+-]?\d+)?)/)) return 'number'
    if (stream.match(/^[A-Za-z_][\w]*(\.[A-Za-z_][\w]*)*/)) {
      const word = stream.current()
      const next = stream.string.slice(stream.pos).trimStart()[0]
      if (KEYWORDS.has(word)) return 'keyword'
      if (ATOMS.has(word)) return 'atom'
      if (BUILTIN_TYPES.has(word)) return 'typeName'
      if (/^on[A-Z]/.test(word) && next === ':') return 'function'                 // onClicked:
      if (/^[A-Z]/.test(word) && (next === '{' || state.afterColon === false && next === undefined)) return 'typeName' // Rectangle {
      if (/^[A-Z]/.test(word)) return 'className'
      if (next === ':' && !state.afterColon) return 'propertyName'                  // width:
      if (next === '(') return 'function'
      return 'variableName'
    }
    const ch = stream.next()
    if (ch === ':') state.afterColon = true
    if (ch === ';') state.afterColon = false
    return /[{}()[\]]/.test(ch ?? '') ? 'bracket' : 'operator'
  },
  languageData: { commentTokens: { line: '//', block: { open: '/*', close: '*/' } } },
}
