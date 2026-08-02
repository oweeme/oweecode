import { EditorView } from '@codemirror/view'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { tags as t } from '@lezer/highlight'

// A hand-rolled light theme (GitHub Light palette) so the editor content
// actually matches the app's "Claro" theme instead of always rendering dark.
const lightEditorTheme = EditorView.theme({
  '&': { color: '#24292f', backgroundColor: '#ffffff' },
  '.cm-content': { caretColor: '#24292f' },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: '#24292f' },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection': { backgroundColor: '#b6d7ff' },
  '.cm-activeLine': { backgroundColor: '#f6f8fa' },
  '.cm-activeLineGutter': { backgroundColor: '#eef1f4' },
  '.cm-gutters': { backgroundColor: '#ffffff', color: '#8c8f97', border: 'none' },
  '.cm-lineNumbers .cm-gutterElement': { color: '#8c8f97' },
  '.cm-foldPlaceholder': { backgroundColor: '#eaeaea', border: 'none', color: '#57606a' },
  '.cm-tooltip': { border: '1px solid #d0d7de', backgroundColor: '#ffffff' },
  '.cm-tooltip-autocomplete ul li[aria-selected]': { backgroundColor: '#0969da', color: '#ffffff' },
  '.cm-matchingBracket, .cm-nonmatchingBracket': { backgroundColor: '#d0eaff' },
}, { dark: false })

const lightHighlightStyle = HighlightStyle.define([
  { tag: t.keyword, color: '#cf222e' },
  { tag: [t.name, t.deleted, t.character, t.propertyName, t.macroName], color: '#953800' },
  { tag: [t.function(t.variableName), t.labelName], color: '#8250df' },
  { tag: [t.color, t.constant(t.name), t.standard(t.name)], color: '#0550ae' },
  { tag: [t.definition(t.name), t.separator], color: '#24292f' },
  { tag: [t.typeName, t.className, t.number, t.changed, t.annotation, t.modifier, t.self, t.namespace], color: '#0550ae' },
  { tag: [t.operator, t.operatorKeyword, t.url, t.escape, t.regexp, t.link, t.special(t.string)], color: '#0a3069' },
  { tag: [t.meta, t.comment], color: '#6e7781', fontStyle: 'italic' },
  { tag: t.strong, fontWeight: 'bold' },
  { tag: t.emphasis, fontStyle: 'italic' },
  { tag: t.strikethrough, textDecoration: 'line-through' },
  { tag: t.link, color: '#0550ae', textDecoration: 'underline' },
  { tag: t.heading, fontWeight: 'bold', color: '#0550ae' },
  { tag: [t.atom, t.bool, t.special(t.variableName)], color: '#0550ae' },
  { tag: [t.processingInstruction, t.string, t.inserted], color: '#0a3069' },
  { tag: t.invalid, color: '#82071e' },
])

export function lightCmTheme() {
  return [lightEditorTheme, syntaxHighlighting(lightHighlightStyle)]
}
