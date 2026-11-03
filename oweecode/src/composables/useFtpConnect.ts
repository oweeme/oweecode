// Connect / disconnect an FTP/SFTP/FTPS connection. Shared by the sidebar list and the FTP tab (reconnect button).
import { invoke } from '@tauri-apps/api/core'
import { ask } from '@tauri-apps/plugin-dialog'
import { useFtpStore, type FtpConn } from './useFtpStore'
import { useEditorStore } from './useEditorStore'
import { useI18n } from './useI18n'
import { errorText } from './errorText'

/** Opens the connection; asks the user about unknown SSH host keys and untrusted FTPS certificates. Throws on failure. */
export async function connectFtp(c: FtpConn): Promise<void> {
  const { connectedIds, save } = useFtpStore()
  const { t, locale } = useI18n()
  const args = { id: c.id, protocol: c.protocol, host: c.host, port: c.port, user: c.user, pass: c.pass, root: c.root }
  let acceptHost = false, established = false
  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      await invoke('remote_connect', { ...args, acceptHost, trustCert: c.trustedCert ?? null })
      established = true
      break
    } catch (err: any) {
      const msg = String(err)
      if (msg.startsWith('HOSTKEY_MISMATCH')) throw new Error(errorText(msg.split('|')[3] ?? msg, locale.value))
      if (msg.startsWith('HOSTKEY_UNKNOWN')) {
        const [, type, fp] = msg.split('|')
        const trust = await ask(
          t('cHostNew', { host: c.host, type, fp }),
          { title: t('cHostTitle'), kind: 'warning', okLabel: t('cHostOk'), cancelLabel: t('cCancel') })
        if (!trust) throw new Error(t('cHostRefused'))
        acceptHost = true
        continue
      }
      if (msg.startsWith('CERT_UNTRUSTED') || msg.startsWith('CERT_MISMATCH')) {
        const [kind, fp, why] = msg.split('|')
        const changed = kind === 'CERT_MISMATCH'
        const trust = await ask(
          changed
            ? t('cCertChanged', { host: c.host, fp })
            : t('cCertUntrusted', { host: c.host, why: why ?? '', fp }),
          { title: changed ? t('cCertTitleChanged') : t('cCertTitleUntrusted'), kind: 'warning', okLabel: changed ? t('cCertOkNew') : t('cCertOk'), cancelLabel: t('cCancel') })
        if (!trust) throw new Error(t('cCertRefused'))
        c.trustedCert = fp; save()
        continue
      }
      throw err
    }
  }
  if (!established) throw new Error(t('cNoSecure'))
  connectedIds.value.add(c.id)
  connectedIds.value = new Set(connectedIds.value)
  // an FTP tab that was already open only re-focuses: tell it to reload
  window.dispatchEvent(new CustomEvent('ftp-connected', { detail: c.id }))
}

export async function disconnectFtp(id: string): Promise<void> {
  const { connectedIds } = useFtpStore()
  try { await invoke('remote_disconnect', { id }) } catch { /* already closed */ }
  connectedIds.value.delete(id)
  connectedIds.value = new Set(connectedIds.value)
  window.dispatchEvent(new CustomEvent('ftp-disconnected', { detail: id }))
  useEditorStore().closeTab(`ftp://${id}`)   // a closed connection has nothing to show: close its tab
}
