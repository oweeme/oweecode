import { reactive } from 'vue'
import { check, type Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'

interface UpdaterState {
  checking: boolean
  available: boolean
  version: string
  currentVersion: string
  notes: string
  downloading: boolean
  progress: number
  installing: boolean
  error: string
  dismissed: boolean
  // Set instead of `error` when the install fails on Linux — the Tauri
  // updater can only self-replace an AppImage; a .deb/.rpm install lives in
  // a system directory a regular process can't overwrite, so a failure here
  // almost always means "wrong install method," not "something broke."
  manualUpdateUrl: string
}

const state = reactive<UpdaterState>({
  checking: false,
  available: false,
  version: '',
  currentVersion: '',
  notes: '',
  downloading: false,
  progress: 0,
  installing: false,
  error: '',
  dismissed: false,
  manualUpdateUrl: '',
})

const RELEASES_URL = 'https://github.com/oweeme/oweecode/releases/latest'

let pendingUpdate: Update | null = null

async function checkForUpdates() {
  state.checking = true
  state.error = ''
  state.manualUpdateUrl = ''
  try {
    const update = await check()
    if (update) {
      pendingUpdate = update
      state.available = true
      state.dismissed = false
      state.version = update.version
      state.currentVersion = update.currentVersion
      state.notes = update.body ?? ''
    } else {
      state.available = false
      pendingUpdate = null
    }
  } catch (e: any) {
    state.error = String(e)
  } finally {
    state.checking = false
  }
}

async function installUpdate() {
  if (!pendingUpdate) return
  state.downloading = true
  state.progress = 0
  state.error = ''
  state.manualUpdateUrl = ''
  let total = 0
  let downloaded = 0
  try {
    await pendingUpdate.downloadAndInstall((event) => {
      if (event.event === 'Started') {
        total = event.data.contentLength ?? 0
      } else if (event.event === 'Progress') {
        downloaded += event.data.chunkLength
        state.progress = total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : 0
      } else if (event.event === 'Finished') {
        state.progress = 100
      }
    })
    state.downloading = false
    state.installing = true
    await relaunch()
  } catch (e: any) {
    state.downloading = false
    state.installing = false
    const isLinux = !navigator.userAgent.includes('Windows') && !navigator.userAgent.includes('Mac')
    if (isLinux) {
      state.manualUpdateUrl = RELEASES_URL
    } else {
      state.error = String(e)
    }
  }
}

function dismiss() {
  state.dismissed = true
}

export function useAppUpdater() {
  return { state, checkForUpdates, installUpdate, dismiss }
}
