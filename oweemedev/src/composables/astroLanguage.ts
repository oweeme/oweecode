import type { StreamParser, StringStream } from '@codemirror/language'
import { typescript as tsMode } from '@codemirror/legacy-modes/mode/javascript'
import { html as htmlMode } from '@codemirror/legacy-modes/mode/xml'

// CodeMirror has no real Astro grammar (checked npm — nothing exists, official
// or community). Falling back to plain "html" for the whole file left the
// `---`-fenced frontmatter (imports, consts — often the part with the most
// logic) completely unstyled, since HTML has no idea what that fence is.
// Astro's structure is simple and fixed enough to hand-tokenize instead: an
// optional `---` fence of plain TS at the top, then an HTML-like template.
// This switches which existing legacy StreamParser handles each region,
// rather than inventing a new grammar from scratch.
interface AstroState {
  inFrontmatter: boolean
  fenceSeen: boolean
  inner: unknown
}

function innerMode(state: AstroState) {
  return state.inFrontmatter ? tsMode : htmlMode
}

export const astroLanguage: StreamParser<AstroState> = {
  name: 'astro',
  startState(indentUnit: number): AstroState {
    return { inFrontmatter: false, fenceSeen: false, inner: htmlMode.startState?.(indentUnit) }
  },
  token(stream: StringStream, state: AstroState): string | null {
    if (stream.sol() && /^---\s*$/.test(stream.string)) {
      stream.skipToEnd()
      if (!state.fenceSeen) {
        state.fenceSeen = true
        state.inFrontmatter = true
        state.inner = tsMode.startState?.(2)
      } else if (state.inFrontmatter) {
        state.inFrontmatter = false
        state.inner = htmlMode.startState?.(2)
      }
      return 'meta'
    }
    return innerMode(state).token(stream, state.inner)
  },
  blankLine(state: AstroState, indentUnit: number) {
    innerMode(state).blankLine?.(state.inner, indentUnit)
  },
  copyState(state: AstroState): AstroState {
    const mode = innerMode(state)
    return {
      inFrontmatter: state.inFrontmatter,
      fenceSeen: state.fenceSeen,
      inner: mode.copyState ? mode.copyState(state.inner) : state.inner,
    }
  },
}
