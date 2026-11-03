<script setup lang="ts">
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog'
import { useEditorStore } from '../composables/useEditorStore'
import { useFtpStore, type FtpConn } from '../composables/useFtpStore'
import { useI18n } from '../composables/useI18n'
import { connectFtp, disconnectFtp } from '../composables/useFtpConnect'

const store = useEditorStore()
const { t } = useI18n()
const emit = defineEmits<{ 'open-conn': [connId: string, connName: string, protocol: string] }>()

const { connections, connectedIds, save, keychainBlocked } = useFtpStore()
const connectingId = ref<string | null>(null)
const errorMsg = ref('')
const showForm = ref(false)
const editingConn = ref<FtpConn | null>(null)

const form = ref<Omit<FtpConn, 'id'>>({
  name: '', protocol: 'sftp', host: '', port: 22,
  user: '', pass: '', root: '/', authType: 'password',
})

function openAddForm() {
  editingConn.value = null
  form.value = { name: '', protocol: 'sftp', host: '', port: 22, user: '', pass: '', root: '/', authType: 'password' }
  showForm.value = true
  errorMsg.value = ''
}

function openEditForm(c: FtpConn) {
  editingConn.value = c
  form.value = { ...c }
  showForm.value = true
  errorMsg.value = ''
}

function onProtocolChange() {
  form.value.port = form.value.protocol === 'sftp' ? 22 : 21   // ftp and ftps(explicit) both use 21
}

function cancelForm() { showForm.value = false; editingConn.value = null }

function submitForm() {
  if (!form.value.host || !form.value.user) { errorMsg.value = `${t('host')} + ${t('user')} (${t('required')})`; return }
  if (editingConn.value) {
    Object.assign(editingConn.value, form.value)
  } else {
    connections.value.push({ ...form.value, id: crypto.randomUUID() })
  }
  save()
  showForm.value = false
  editingConn.value = null
}

function removeConn(id: string) {
  if (connectedIds.value.has(id)) void disconnectFtp(id)
  connectedIds.value.delete(id)
  connections.value = connections.value.filter(c => c.id !== id)
  save()
}

async function toggleConnect(c: FtpConn) {
  errorMsg.value = ''
  if (connectedIds.value.has(c.id)) { await disconnectFtp(c.id); return }
  connectingId.value = c.id
  try {
    await connectFtp(c)
    emit('open-conn', c.id, c.name, c.protocol)
    store.openFtpConn(c.id, c.name, c.protocol)
  } catch (e: any) {
    errorMsg.value = String(e)
  } finally {
    connectingId.value = null
  }
}

function openConn(c: FtpConn) {
  if (!connectedIds.value.has(c.id)) return
  store.openFtpConn(c.id, c.name, c.protocol)
}

// ── Import sites from a FileZilla sitemanager.xml export ──────────────────────
function directText(el: Element): string {
  for (const node of Array.from(el.childNodes)) {
    if (node.nodeType === Node.TEXT_NODE && node.textContent?.trim()) return node.textContent.trim()
  }
  return ''
}

async function importFileZilla() {
  const path = await openDialog({ filters: [{ name: 'FileZilla Sites', extensions: ['xml'] }], multiple: false })
  if (typeof path !== 'string') return
  errorMsg.value = ''
  try {
    const xmlText = await invoke<string>('open_file', { path })
    const doc = new DOMParser().parseFromString(xmlText, 'application/xml')
    if (doc.querySelector('parsererror')) throw new Error(t('pXmlBad'))
    const serversRoot = doc.querySelector('Servers')
    if (!serversRoot) throw new Error(t('pXmlNoSites'))

    const imported: FtpConn[] = []
    const walk = (el: Element, prefix: string) => {
      for (const child of Array.from(el.children)) {
        if (child.tagName === 'Server') {
          const get = (tag: string) => child.querySelector(`:scope > ${tag}`)?.textContent?.trim() ?? ''
          const host = get('Host')
          if (!host) continue
          const protoCode = get('Protocol')
          const protocol: 'ftp' | 'sftp' | 'ftps' = protoCode === '1' ? 'sftp' : (protoCode === '4' || protoCode === '3') ? 'ftps' : 'ftp'
          const port = parseInt(get('Port') || '', 10) || (protocol === 'sftp' ? 22 : 21)
          const user = get('User')
          const passEl = child.querySelector(':scope > Pass')
          let pass = passEl?.textContent?.trim() ?? ''
          if (pass && passEl?.getAttribute('encoding') === 'base64') {
            try { pass = atob(pass) } catch { /* leave as-is if not valid base64 */ }
          }
          const name = get('Name') || host
          imported.push({
            id: crypto.randomUUID(),
            name: prefix ? `${prefix}/${name}` : name,
            protocol, host, port, user, pass,
            root: get('RemoteDir') || '/',
            authType: 'password',
          })
        } else if (child.tagName === 'Folder') {
          const folderName = directText(child) || 'Carpeta'
          walk(child, prefix ? `${prefix}/${folderName}` : folderName)
        }
      }
    }
    walk(serversRoot, '')

    if (imported.length === 0) throw new Error(t('pXmlNoConn'))
    connections.value.push(...imported)
    save()
    errorMsg.value = `✓ ${imported.length} ${t('connectionsImported')}`
  } catch (e: any) {
    errorMsg.value = `${t('importErrorPrefix')} ${e.message ?? e}`
  }
}

// ── Export all sites as a FileZilla sitemanager.xml — FileZilla can import it
// directly, and it's the same format importFileZilla() above already reads,
// so it doubles as the way to move connections to another OweeCode install ──
function escapeXml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
}

async function exportFileZilla() {
  errorMsg.value = ''
  if (connections.value.length === 0) { errorMsg.value = t('noConnectionsToExport'); return }
  const path = await saveDialog({
    defaultPath: 'oweecode-ftp-sites.xml',
    filters: [{ name: 'FileZilla Sites', extensions: ['xml'] }],
  })
  if (!path) return
  try {
    const serversXml = connections.value.map(c => `    <Server>
      <Host>${escapeXml(c.host)}</Host>
      <Port>${c.port}</Port>
      <Protocol>${c.protocol === 'sftp' ? 1 : 0}</Protocol>
      <Type>0</Type>
      <User>${escapeXml(c.user)}</User>
      <Pass encoding="base64">${btoa(c.pass)}</Pass>
      <Logontype>1</Logontype>
      <EncodingType>Auto</EncodingType>
      <BypassProxy>0</BypassProxy>
      <Name>${escapeXml(c.name)}</Name>
      <RemoteDir>${escapeXml(c.root)}</RemoteDir>
    </Server>`).join('\n')
    const xml = `<?xml version="1.0" encoding="UTF-8"?>\n<FileZilla3>\n  <Servers>\n${serversXml}\n  </Servers>\n</FileZilla3>\n`
    await invoke('save_file', { path, content: xml })
    errorMsg.value = `✓ ${connections.value.length} ${t('connectionsExported')}`
  } catch (e: any) {
    errorMsg.value = `${t('importErrorPrefix')} ${e.message ?? e}`
  }
}
</script>

<template>
  <div class="ftp-panel">
    <div class="ftp-header">
      <span class="ftp-title">{{ t('ftp') }}</span>
      <div style="display:flex;gap:2px">
        <button class="ftp-add-btn" @click="importFileZilla" :title="t('importFileZillaTitle')">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor">
            <path d="M.5 9.9a.5.5 0 01.5.5v2.5a1 1 0 001 1h12a1 1 0 001-1v-2.5a.5.5 0 011 0v2.5a2 2 0 01-2 2H2a2 2 0 01-2-2v-2.5a.5.5 0 01.5-.5z"/>
            <path d="M7.646 10.854a.5.5 0 00.708 0l3-3a.5.5 0 00-.708-.708L8.5 9.293V1.5a.5.5 0 00-1 0v7.793L5.354 7.146a.5.5 0 10-.708.708l3 3z"/>
          </svg>
        </button>
        <button class="ftp-add-btn" @click="exportFileZilla" :title="t('exportFileZillaTitle')">
          <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor">
            <path d="M.5 9.9a.5.5 0 01.5.5v2.5a1 1 0 001 1h12a1 1 0 001-1v-2.5a.5.5 0 011 0v2.5a2 2 0 01-2 2H2a2 2 0 01-2-2v-2.5a.5.5 0 01.5-.5z"/>
            <path d="M7.646 1.146a.5.5 0 01.708 0l3 3a.5.5 0 01-.708.708L8.5 2.707V10.5a.5.5 0 01-1 0V2.707L5.354 4.854a.5.5 0 11-.708-.708l3-3z"/>
          </svg>
        </button>
        <button class="ftp-add-btn" @click="openAddForm" :title="t('newConnection')">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 2a.5.5 0 01.5.5v5h5a.5.5 0 010 1h-5v5a.5.5 0 01-1 0v-5h-5a.5.5 0 010-1h5v-5A.5.5 0 018 2z"/>
          </svg>
        </button>
      </div>
    </div>

    <!-- Error -->
    <div v-if="errorMsg" class="ftp-error" :class="{ 'ftp-error--ok': errorMsg.startsWith('✓') }">{{ errorMsg }}</div>

    <!-- Connection list -->
    <div class="ftp-list">
      <div v-if="connections.length === 0" class="ftp-empty">
        <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" opacity=".3">
          <path d="M9 3H5a2 2 0 00-2 2v4m6-6h10a2 2 0 012 2v4M9 3v18m0 0h10a2 2 0 002-2V9M9 21H5a2 2 0 01-2-2V9m0 0h18"/>
        </svg>
        <span>{{ t('noConnections') }}</span>
        <button class="ftp-new-big" @click="openAddForm">+ {{ t('newConnection') }}</button>
      </div>

      <div
        v-for="c in connections" :key="c.id"
        class="ftp-conn-row"
        :class="{ connected: connectedIds.has(c.id) }"
        @click="openConn(c)"
      >
        <span class="ftp-conn-dot" :class="{ on: connectedIds.has(c.id) }" />
        <div class="ftp-conn-icon">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M9 3H5a2 2 0 00-2 2v4m6-6h10a2 2 0 012 2v4M9 3v18m0 0h10a2 2 0 002-2V9M9 21H5a2 2 0 01-2-2V9m0 0h18"/>
          </svg>
        </div>
        <div class="ftp-conn-info">
          <div class="ftp-conn-name">
            {{ c.name }}
            <span v-if="connectedIds.has(c.id)" class="ftp-conn-live">● {{ t('connected') }}</span>
          </div>
          <div class="ftp-conn-host">{{ c.protocol.toUpperCase() }} · {{ c.user }}@{{ c.host }}</div>
        </div>
        <div class="ftp-conn-actions">
          <button
            class="ftp-pill" :class="connectedIds.has(c.id) ? 'off' : 'on'"
            @click.stop="toggleConnect(c)"
            :title="connectedIds.has(c.id) ? t('pCloseConn', { name: c.name }) : t('pOpenConn', { name: c.name })"
            :disabled="connectingId === c.id"
          >
            <span v-if="connectingId === c.id" class="conn-spin" />
            <template v-else>
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                <template v-if="connectedIds.has(c.id)"><path d="M18.36 6.64a9 9 0 11-12.73 0"/><line x1="12" y1="2" x2="12" y2="12"/></template>
                <template v-else><path d="M5 12h14"/><path d="M13 6l6 6-6 6"/></template>
              </svg>
              {{ connectedIds.has(c.id) ? 'Desconectar' : 'Conectar' }}
            </template>
          </button>
          <button class="ftp-icon-btn" @click.stop="openEditForm(c)" :title="t('pEditConn')">
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 013 3L7 19l-4 1 1-4 12.5-12.5z"/></svg>
          </button>
          <button class="ftp-icon-btn danger" @click.stop="removeConn(c.id)" :title="t('pDelConn')">
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6"/><path d="M10 11v6M14 11v6"/><path d="M9 6V4a1 1 0 011-1h4a1 1 0 011 1v2"/></svg>
          </button>
        </div>
      </div>
    </div>

    <!-- Add/Edit form overlay -->
    <div v-if="showForm" class="ftp-form-overlay" @click.self="cancelForm">
      <div class="ftp-form">
        <div class="ftp-form-title">{{ editingConn ? t('editConnection') : t('newConnection') }}</div>

        <label>{{ t('name') }}</label>
        <input v-model="form.name" class="ftp-input" placeholder="My server" />

        <label>{{ t('protocolLabel') }}</label>
        <div class="ftp-proto-group">
          <button class="ftp-proto-btn" :class="{ active: form.protocol === 'sftp' }"
            @click="form.protocol = 'sftp'; onProtocolChange()">SFTP</button>
          <button class="ftp-proto-btn" :class="{ active: form.protocol === 'ftp' }"
            @click="form.protocol = 'ftp'; onProtocolChange()">FTP</button>
          <button class="ftp-proto-btn" :class="{ active: form.protocol === 'ftps' }"
            @click="form.protocol = 'ftps'; onProtocolChange()">FTPS</button>
        </div>
        <div v-if="form.protocol === 'ftp'" class="ftp-form-error">{{ t('pPlainWarn') }}</div>
        <div v-else-if="form.protocol === 'ftps'" class="ftp-form-note">{{ t('pFtpsNote') }}</div>
        <div v-if="keychainBlocked" class="ftp-form-error">{{ t('pNoKeychain') }}</div>

        <div class="ftp-row2">
          <div>
            <label>{{ t('host') }}</label>
            <input v-model="form.host" class="ftp-input" placeholder="ftp.example.com" />
          </div>
          <div style="width:80px">
            <label>{{ t('port') }}</label>
            <input v-model.number="form.port" class="ftp-input" type="number" />
          </div>
        </div>

        <label>{{ t('user') }}</label>
        <input v-model="form.user" class="ftp-input" placeholder="oweeme" />

        <div v-if="form.protocol === 'sftp'">
          <label>{{ t('authentication') }}</label>
          <div class="ftp-proto-group">
            <button class="ftp-proto-btn" :class="{ active: form.authType === 'password' }"
              @click="form.authType = 'password'">{{ t('password') }}</button>
            <button class="ftp-proto-btn" :class="{ active: form.authType === 'key' }"
              @click="form.authType = 'key'">SSH Key</button>
          </div>
        </div>

        <label>{{ form.authType === 'key' ? t('passphraseOptional') : t('password') }}</label>
        <input v-model="form.pass" class="ftp-input" type="password"
          :placeholder="form.authType === 'key' ? '(ssh-agent)' : '••••••••'" />

        <label>{{ t('initialRemotePath') }}</label>
        <input v-model="form.root" class="ftp-input" placeholder="/public" />

        <div v-if="errorMsg" class="ftp-form-error">{{ errorMsg }}</div>

        <div class="ftp-form-footer">
          <button class="ftp-cancel-btn" @click="cancelForm">{{ t('cancel') }}</button>
          <button class="ftp-save-btn" @click="submitForm">
            {{ editingConn ? t('save') : t('create') }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ftp-panel {
  display: flex; flex-direction: column; height: 100%;
  font-family: var(--font-ui); position: relative;
}

.ftp-header {
  display: flex; align-items: center; justify-content: space-between;
  padding: 0 12px; height: 36px; flex-shrink: 0;
  border-bottom: 1px solid var(--border);
}
.ftp-title {
  font-size: 10px; font-weight: 700; letter-spacing: 1px;
  color: var(--fg-muted); text-transform: uppercase;
}
.ftp-add-btn {
  background: none; border: none; color: var(--fg-muted);
  cursor: pointer; padding: 3px; border-radius: 4px;
  display: flex; align-items: center; transition: color 0.12s;
}
.ftp-add-btn:hover { color: var(--fg); }

.ftp-error {
  margin: 8px 12px; padding: 8px 10px; background: rgba(248,81,73,0.12);
  border: 1px solid rgba(248,81,73,0.3); border-radius: 6px;
  font-size: 11px; color: #f85149; line-height: 1.5;
}
.ftp-error--ok {
  background: rgba(166,227,161,0.12); border-color: rgba(166,227,161,0.3); color: var(--green);
}

.ftp-list { flex: 1; overflow-y: auto; padding: 6px 0; }
.ftp-list::-webkit-scrollbar { width: 4px; }
.ftp-list::-webkit-scrollbar-thumb { background: var(--bg-active); border-radius: 2px; }

.ftp-empty {
  display: flex; flex-direction: column; align-items: center; gap: 10px;
  padding: 32px 16px; color: var(--fg-muted); font-size: 12px;
}
.ftp-new-big {
  background: rgba(208,208,216,0.12); border: 1px solid rgba(208,208,216,0.3);
  border-radius: 6px; color: var(--accent); font-size: 12px;
  padding: 6px 14px; cursor: pointer; transition: all 0.12s;
}
.ftp-new-big:hover { background: rgba(208,208,216,0.2); }

.ftp-conn-row {
  display: flex; align-items: center; gap: 8px;
  padding: 7px 12px 7px 9px; cursor: pointer; border-left: 3px solid transparent;
  transition: background 0.15s, border-color 0.15s; border-radius: 4px; margin: 1px 4px;
}
.ftp-conn-row:hover { background: var(--bg-hover); }
.ftp-conn-row.connected {
  background: rgba(166,227,161,0.08);
  border-left-color: var(--green);
}
.ftp-conn-row.connected:hover { background: rgba(166,227,161,0.13); }
.ftp-conn-row.connected .ftp-conn-icon { color: var(--green); }
.ftp-conn-row.connected .ftp-conn-name { color: var(--fg-bright); }

.ftp-conn-dot { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; background: var(--fg-muted); opacity: 0.4; }
.ftp-conn-dot.on { background: var(--green); opacity: 1; box-shadow: 0 0 5px rgba(166,227,161,0.7); }

.ftp-conn-icon { color: var(--fg-muted); flex-shrink: 0; }
.ftp-conn-info { flex: 1; min-width: 0; }
.ftp-conn-name { display: flex; align-items: center; gap: 6px; font-size: 12.5px; font-weight: 600; color: var(--fg); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.ftp-conn-live { font-size: 9px; font-weight: 700; color: var(--green); letter-spacing: 0.3px; flex-shrink: 0; }
.ftp-conn-host { font-size: 10.5px; color: var(--fg-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 1px; }

.ftp-conn-actions { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }
.ftp-pill { display: inline-flex; align-items: center; gap: 4px; font-size: 11px; font-weight: 600; padding: 3px 9px; border-radius: 12px; border: 1px solid; background: none; cursor: pointer; white-space: nowrap; }
.ftp-pill.on { color: #3fb950; border-color: #3fb95066; } .ftp-pill.on:hover { background: #3fb95022; }
.ftp-pill.off { color: #f0883e; border-color: #f0883e66; } .ftp-pill.off:hover { background: #f0883e22; }
.ftp-pill:disabled { opacity: .5; cursor: default; }
.ftp-icon-btn { width: 26px; height: 26px; display: inline-flex; align-items: center; justify-content: center; background: none; border: 1px solid transparent; border-radius: 6px; color: var(--fg); opacity: .8; cursor: pointer; }
.ftp-icon-btn:hover { background: var(--bg-active); border-color: var(--border); opacity: 1; }
.ftp-icon-btn.danger:hover { color: #f85149; border-color: #f8514966; background: #f8514914; }
.ftp-action-btn {
  width: 22px; height: 22px; background: none; border: none;
  border-radius: 4px; color: var(--fg-muted); cursor: pointer;
  display: flex; align-items: center; justify-content: center; transition: all 0.1s;
}
.ftp-action-btn:hover { background: var(--bg-active); color: var(--fg); }
.ftp-action-btn.connected { color: var(--green); }
.ftp-action-btn.connecting { opacity: 0.5; cursor: default; }
.ftp-action-del:hover { color: #f85149 !important; }
.conn-spin {
  width: 10px; height: 10px; border: 2px solid rgba(208,208,216,0.3);
  border-top-color: var(--accent); border-radius: 50%;
  animation: spin 0.7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

/* Form overlay */
.ftp-form-overlay {
  position: absolute; inset: 0; z-index: 100;
  background: rgba(0,0,0,0.7); display: flex; align-items: flex-start;
  padding: 16px 10px; overflow-y: auto;
}
.ftp-form {
  background: var(--bg-panel); border: 1px solid var(--border);
  border-radius: 10px; padding: 16px; width: 100%;
  display: flex; flex-direction: column; gap: 8px;
}
.ftp-form-title { font-size: 13px; font-weight: 700; color: var(--fg-bright); margin-bottom: 4px; }
.ftp-form label { font-size: 11px; color: var(--fg-dim); }
.ftp-input {
  width: 100%; background: var(--bg-mid); border: 1px solid var(--border);
  border-radius: 6px; color: var(--fg); font-size: 12px;
  padding: 5px 8px; outline: none; box-sizing: border-box;
  transition: border-color 0.12s;
}
.ftp-input:focus { border-color: var(--accent); }
.ftp-row2 { display: flex; gap: 8px; }
.ftp-row2 > div { flex: 1; display: flex; flex-direction: column; gap: 4px; }

.ftp-proto-group { display: flex; gap: 4px; }
.ftp-proto-btn {
  flex: 1; background: var(--bg-mid); border: 1px solid var(--border);
  border-radius: 5px; color: var(--fg-dim); font-size: 11px;
  padding: 4px; cursor: pointer; transition: all 0.12s;
}
.ftp-proto-btn.active {
  background: rgba(208,208,216,0.15); border-color: var(--accent);
  color: var(--accent); font-weight: 600;
}
.ftp-form-note { font-size: 11px; color: #3fb950; margin: 4px 0; }
.ftp-form-error {
  font-size: 11px; color: #f85149; padding: 6px 8px;
  background: rgba(248,81,73,0.1); border-radius: 5px;
}
.ftp-form-footer { display: flex; gap: 8px; margin-top: 4px; }
.ftp-cancel-btn {
  flex: 1; background: none; border: 1px solid var(--border);
  border-radius: 6px; color: var(--fg-muted); font-size: 12px;
  padding: 6px; cursor: pointer;
}
.ftp-cancel-btn:hover { border-color: var(--fg-muted); color: var(--fg); }
.ftp-save-btn {
  flex: 2; background: var(--accent); border: none; border-radius: 6px;
  color: var(--accent-fg); font-size: 12px; font-weight: 600;
  padding: 6px; cursor: pointer; transition: opacity 0.12s;
}
.ftp-save-btn:hover { opacity: 0.85; }
</style>
