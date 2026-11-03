// Decides whether the preview should reload by itself when a project file changes.
// Servers with hot reload (Vite, Quasar, webpack, Next…) already refresh the page: reloading too would throw away the
// state the page keeps, so "auto" leaves them alone and only reloads the rest (PHP, Python, Go, static folders, Docker…).

export type ReloadMode = 'auto' | 'always' | 'off'

const HOT_RELOAD = /\b(vite|quasar|webpack|webpack-dev-server|next|nuxt|astro|parcel|react-scripts|vue-cli-service|ng serve|svelte-kit|sveltekit|remix|expo|snowpack|rsbuild|turbo)\b/i

/** the command line of the process that owns the port tells what kind of server it is */
export function hasHotReload(cmdline: string): boolean { return HOT_RELOAD.test(cmdline) }

export interface PortInfo { port: number; process: string; cmdline: string }

/** localhost URLs only: an external page is not ours to reload */
export function localPort(url: string): number | null {
  try {
    const u = new URL(url)
    if (!['localhost', '127.0.0.1', '0.0.0.0', '[::1]'].includes(u.hostname)) return null
    return Number(u.port || (u.protocol === 'https:' ? 443 : 80))
  } catch { return null }
}

export function shouldReload(mode: ReloadMode, url: string, ports: PortInfo[]): boolean {
  if (mode === 'off') return false
  const port = localPort(url)
  if (port === null) return false
  if (mode === 'always') return true
  const info = ports.find(p => p.port === port)
  return !(info && hasHotReload(info.cmdline))
}

/** a changed .css on a static-folder server is swapped in place by the page itself: no reload */
export function cssHandledInPage(file: string, url: string, staticPorts: number[]): boolean {
  const port = localPort(url)
  return /\.css$/i.test(file) && port !== null && staticPorts.includes(port)
}
