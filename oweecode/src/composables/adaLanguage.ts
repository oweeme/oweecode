import type { StreamParser, StringStream } from '@codemirror/language'

// CodeMirror ships no Ada mode (not even in @codemirror/legacy-modes), so this
// is a small hand-written StreamParser — same shape CM5-style legacy modes use,
// which is what StreamLanguage.define() expects.
const KEYWORDS = new Set([
  'abort', 'abs', 'abstract', 'accept', 'access', 'aliased', 'all', 'and', 'array', 'at',
  'begin', 'body', 'case', 'constant', 'declare', 'delay', 'delta', 'digits', 'do',
  'else', 'elsif', 'end', 'entry', 'exception', 'exit', 'for', 'function', 'generic',
  'goto', 'if', 'in', 'interface', 'is', 'limited', 'loop', 'mod', 'new', 'not', 'null',
  'of', 'or', 'others', 'out', 'overriding', 'package', 'pragma', 'private', 'procedure',
  'protected', 'raise', 'range', 'record', 'rem', 'renames', 'requeue', 'return',
  'reverse', 'select', 'separate', 'some', 'subtype', 'synchronized', 'tagged',
  'task', 'terminate', 'then', 'type', 'until', 'use', 'when', 'while', 'with', 'xor',
])

interface AdaState { inComment: boolean }

export const adaLanguage: StreamParser<AdaState> = {
  startState(): AdaState {
    return { inComment: false }
  },
  token(stream: StringStream): string | null {
    if (stream.eatSpace()) return null

    if (stream.match('--')) {
      stream.skipToEnd()
      return 'comment'
    }

    // Based numeric literals (16#FF#, 2#1010_1010#) and plain decimals/reals
    if (stream.match(/^\d[\d_]*#[0-9a-fA-F_]+#(?:[eE][+-]?\d+)?/)) return 'number'
    if (stream.match(/^\d[\d_]*\.?[\d_]*(?:[eE][+-]?\d+)?/)) return 'number'

    if (stream.match(/^"(?:[^"]|"")*"/)) return 'string'
    // Character literal, e.g. 'A' — deliberately narrow so a lone tick used
    // for an attribute reference (Integer'Last) isn't mistaken for one.
    if (stream.match(/^'.'/)) return 'string'

    if (stream.match(/^[A-Za-z][A-Za-z0-9_]*/)) {
      const word = stream.current().toLowerCase()
      return KEYWORDS.has(word) ? 'keyword' : 'variableName'
    }

    if (stream.match(/^(:=|=>|\.\.|\*\*|<=|>=|\/=|<<|>>|<>)/)) return 'operator'

    stream.next()
    return null
  },
}
