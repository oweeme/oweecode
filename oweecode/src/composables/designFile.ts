// Open an existing .vue (Quasar) or .html file in the visual designer and write it back
// without touching the parts the designer does not own (<script>, <style> of Vue files,
// <head> of HTML files).

export interface VueParts { before: string; inner: string; after: string; scriptsAfter: boolean }

/** Splits a Vue SFC around its root <template> block (depth-aware: nested <template v-if> are kept inside). */
export function splitVueSfc(src: string): VueParts | null {
  const re = /<(\/?)(template|script|style)\b[^>]*>/g
  let m: RegExpExecArray | null, depth = 0, start = -1, innerStart = -1
  while ((m = re.exec(src))) {
    const closing = m[1] === '/', tag = m[2]
    if (tag !== 'template') {
      if (!closing && depth === 0) { const end = src.indexOf(`</${tag}>`, re.lastIndex); if (end < 0) return null; re.lastIndex = end + tag.length + 3 }
      continue
    }
    if (!closing) { if (depth === 0) { start = m.index; innerStart = re.lastIndex } depth++ }
    else {
      depth--
      if (depth === 0 && start >= 0) return { before: src.slice(0, innerStart), inner: src.slice(innerStart, m.index), after: src.slice(m.index), scriptsAfter: /<script\b/.test(src.slice(m.index)) }
      if (depth < 0) return null
    }
  }
  return null
}
export const joinVueSfc = (p: VueParts, inner: string) => `${p.before}\n${inner.replace(/^\n+|\s+$/g, '')}\n${p.after}`

export interface Risk { level: 'ok' | 'info'; reasons: string[] }

/** Names, @events, #slots, v-model/v-if/v-for and attribute/tag case survive the designer untouched (checked on real
 *  projects). What it cannot do is *run* the logic: the canvas shows every v-if/v-else branch and every v-for item once. */
export function analyzeTemplate(inner: string): Risk {
  const reasons: string[] = []
  const logic = (inner.match(/\sv-(for|if|else-if|else|show)\b/g) ?? []).length
  if (logic) reasons.push(`${logic} v-if/v-for/v-show: el lienzo muestra todas las ramas a la vez (en el archivo quedan igual)`)
  return { level: reasons.length ? 'info' : 'ok', reasons }
}

// ── HTML files ──
export interface HtmlParts { head: string; css: string; hasStyle: boolean; body: string }
export function splitHtml(src: string): HtmlParts {
  const doc = new DOMParser().parseFromString(src, 'text/html')
  const style = doc.querySelector('style')
  return { head: doc.head.innerHTML, css: style?.textContent ?? '', hasStyle: !!style, body: doc.body.innerHTML }
}
export function joinHtml(original: string, html: string, css: string): string {
  let out = original
  if (/<body[^>]*>[\s\S]*<\/body>/i.test(out)) out = out.replace(/(<body[^>]*>)[\s\S]*(<\/body>)/i, (_m, a, b) => `${a}\n${html}\n${b}`)
  else out = `${out}\n${html}`
  if (/<style[^>]*>[\s\S]*?<\/style>/i.test(out)) out = out.replace(/(<style[^>]*>)[\s\S]*?(<\/style>)/i, (_m, a, b) => `${a}\n${css}\n${b}`)
  else if (css.trim()) out = out.replace(/<\/head>/i, `<style>\n${css}\n</style>\n</head>`)
  return out
}
