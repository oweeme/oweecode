// The app imports "@tauri-apps/api/core" through this file (see the alias in vite.config.ts): same API, but the errors
// that Rust rejects with are translated to the language of the interface. Tauri freezes window.__TAURI_INTERNALS__,
// so wrapping the function at the import is the only place where this can be done for the whole app at once.
import { invoke as tauriInvoke, type InvokeArgs, type InvokeOptions } from '../../node_modules/@tauri-apps/api/core.js'
import { errorText } from '../composables/errorText'
import { useI18n } from '../composables/useI18n'

export * from '../../node_modules/@tauri-apps/api/core.js'

export async function invoke<T>(cmd: string, args?: InvokeArgs, options?: InvokeOptions): Promise<T> {
  try { return await tauriInvoke<T>(cmd, args, options) }
  catch (e) { throw typeof e === 'string' ? errorText(e, useI18n().locale.value) : e }
}
