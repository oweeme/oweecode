<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useEditorStore } from '../composables/useEditorStore'
import { useI18n } from '../composables/useI18n'

const store = useEditorStore()
const { t } = useI18n()

interface GitFileEntry { path: string; staged: boolean; unstaged: boolean; untracked: boolean; conflicted: boolean; status_code: string }
interface GitStatusResult { is_repo: boolean; branch: string; upstream: string | null; ahead: number; behind: number; files: GitFileEntry[] }
interface GitBranch { name: string; current: boolean }
interface GitCommitInfo { hash: string; author: string; date: string; message: string }

const status = ref<GitStatusResult | null>(null)
const loading = ref(false)
const error = ref('')
const msg = ref('')

const commitMsg = ref('')
const committing = ref(false)
const pushing = ref(false)
const pulling = ref(false)
const fetching = ref(false)

const showBranchMenu = ref(false)
const branches = ref<GitBranch[]>([])
const newBranchName = ref('')

const showLog = ref(false)
const log = ref<GitCommitInfo[]>([])

const githubRepo = ref<{ owner: string; repo: string } | null>(null)

const showIdentityForm = ref(false)
const identityName  = ref('')
const identityEmail = ref('')
const identityGlobal = ref(true)
const identitySaving = ref(false)
const identityError = ref('')
const pendingCommitPush = ref(false)

function isIdentityError(msg: string): boolean {
  return /author identity unknown|please tell me who you are|unable to auto-detect email/i.test(msg)
}

async function openIdentityForm() {
  identityError.value = ''
  try { identityName.value = (await invoke<string | null>('git_config_get', { path: store.state.rootPath, key: 'user.name' })) ?? '' } catch { /* ignore */ }
  try { identityEmail.value = (await invoke<string | null>('git_config_get', { path: store.state.rootPath, key: 'user.email' })) ?? '' } catch { /* ignore */ }
  showIdentityForm.value = true
}

async function saveIdentity() {
  if (!identityName.value.trim() || !identityEmail.value.trim()) return
  identitySaving.value = true
  identityError.value = ''
  try {
    await invoke('git_config_set', { path: store.state.rootPath, key: 'user.name', value: identityName.value.trim(), global: identityGlobal.value })
    await invoke('git_config_set', { path: store.state.rootPath, key: 'user.email', value: identityEmail.value.trim(), global: identityGlobal.value })
    showIdentityForm.value = false
    if (commitMsg.value.trim()) await doCommit(pendingCommitPush.value)
  } catch (e: any) { identityError.value = String(e) }
  finally { identitySaving.value = false }
}

const staged = computed(() => status.value?.files.filter(f => f.staged && !f.conflicted) ?? [])
const unstaged = computed(() => status.value?.files.filter(f => f.unstaged && !f.untracked && !f.conflicted) ?? [])
const untracked = computed(() => status.value?.files.filter(f => f.untracked) ?? [])
const conflicted = computed(() => status.value?.files.filter(f => f.conflicted) ?? [])

function setMsg(m: string, isError = false) {
  if (isError) error.value = m; else msg.value = m
  setTimeout(() => { msg.value = ''; error.value = '' }, 5000)
}

function parseGithubRepo(url: string): { owner: string; repo: string } | null {
  const m = url.match(/github\.com[:/]([^/]+)\/([^/.]+?)(\.git)?$/)
  return m ? { owner: m[1], repo: m[2] } : null
}

async function refresh() {
  if (!store.state.rootPath) { status.value = null; return }
  loading.value = true
  try {
    status.value = await invoke<GitStatusResult>('git_status', { path: store.state.rootPath })
    if (status.value.is_repo) {
      invoke<string | null>('git_remote_url', { path: store.state.rootPath })
        .then(url => { githubRepo.value = url ? parseGithubRepo(url) : null })
        .catch(() => { githubRepo.value = null })
    }
  } catch (e: any) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

async function initRepo() {
  if (!store.state.rootPath) return
  try {
    await invoke('git_init', { path: store.state.rootPath })
    await refresh()
  } catch (e: any) { setMsg(String(e), true) }
}

async function stageFile(path: string) {
  try { await invoke('git_stage', { path: store.state.rootPath, files: [path] }); await refresh() }
  catch (e: any) { setMsg(String(e), true) }
}
async function stageAll() {
  const paths = [...unstaged.value, ...untracked.value].map(f => f.path)
  if (!paths.length) return
  try { await invoke('git_stage', { path: store.state.rootPath, files: paths }); await refresh() }
  catch (e: any) { setMsg(String(e), true) }
}
async function stageAllUntracked() {
  const paths = untracked.value.map(f => f.path)
  if (!paths.length) return
  try { await invoke('git_stage', { path: store.state.rootPath, files: paths }); await refresh() }
  catch (e: any) { setMsg(String(e), true) }
}
async function unstageFile(path: string) {
  try { await invoke('git_unstage', { path: store.state.rootPath, files: [path] }); await refresh() }
  catch (e: any) { setMsg(String(e), true) }
}
async function unstageAll() {
  const paths = staged.value.map(f => f.path)
  if (!paths.length) return
  try { await invoke('git_unstage', { path: store.state.rootPath, files: paths }); await refresh() }
  catch (e: any) { setMsg(String(e), true) }
}
async function discardFile(path: string) {
  if (!confirm(`${t('confirmDiscard')} "${path}"? ${t('cannotUndo')}`)) return
  try { await invoke('git_discard', { path: store.state.rootPath, files: [path] }); await refresh() }
  catch (e: any) { setMsg(String(e), true) }
}
async function removeUntracked(path: string) {
  if (!confirm(`${t('confirmDeleteUntracked')} "${path}"? ${t('cannotUndo')}`)) return
  try { await invoke('git_clean_untracked', { path: store.state.rootPath, files: [path] }); await refresh() }
  catch (e: any) { setMsg(String(e), true) }
}

function openDiff(f: GitFileEntry) {
  if (f.untracked) return
  store.openGitDiff(f.path, f.staged)
}

async function doCommit(push = false) {
  if (!commitMsg.value.trim() || (staged.value.length === 0 && unstaged.value.length === 0)) return
  committing.value = true
  try {
    // Mirrors `git commit -a`: if nothing was explicitly staged, stage the already-tracked
    // modified/deleted files (never untracked ones — those need an explicit opt-in via "+")
    if (staged.value.length === 0 && unstaged.value.length > 0) {
      await invoke('git_stage', { path: store.state.rootPath, files: unstaged.value.map(f => f.path) })
    }
    await invoke('git_commit', { path: store.state.rootPath, message: commitMsg.value.trim() })
    commitMsg.value = ''
    setMsg(t('commitCreated'))
    await refresh()
    if (push) await doPush()
  } catch (e: any) {
    const message = String(e)
    if (isIdentityError(message)) {
      pendingCommitPush.value = push
      openIdentityForm()
    } else {
      setMsg(message, true)
    }
  } finally { committing.value = false }
}

async function doPush() {
  pushing.value = true
  try { await invoke('git_push', { path: store.state.rootPath }); setMsg(t('pushDone')); await refresh() }
  catch (e: any) { setMsg(String(e), true) }
  finally { pushing.value = false }
}
async function doPull() {
  pulling.value = true
  try { await invoke('git_pull', { path: store.state.rootPath }); setMsg(t('pullDone')); await refresh() }
  catch (e: any) { setMsg(String(e), true) }
  finally { pulling.value = false }
}
async function doFetch() {
  fetching.value = true
  try { await invoke('git_fetch', { path: store.state.rootPath }); setMsg(t('fetchDone')); await refresh() }
  catch (e: any) { setMsg(String(e), true) }
  finally { fetching.value = false }
}

async function toggleBranchMenu() {
  showBranchMenu.value = !showBranchMenu.value
  if (showBranchMenu.value) {
    try { branches.value = await invoke<GitBranch[]>('git_branches', { path: store.state.rootPath }) }
    catch (e: any) { setMsg(String(e), true) }
  }
}
async function checkoutBranch(name: string) {
  try {
    await invoke('git_checkout_branch', { path: store.state.rootPath, branch: name })
    showBranchMenu.value = false
    await refresh()
  } catch (e: any) { setMsg(String(e), true) }
}
async function createBranch() {
  if (!newBranchName.value.trim()) return
  try {
    await invoke('git_create_branch', { path: store.state.rootPath, name: newBranchName.value.trim() })
    newBranchName.value = ''
    showBranchMenu.value = false
    await refresh()
  } catch (e: any) { setMsg(String(e), true) }
}

async function toggleLog() {
  showLog.value = !showLog.value
  if (showLog.value) {
    try { log.value = await invoke<GitCommitInfo[]>('git_log', { path: store.state.rootPath, limit: 30 }) }
    catch (e: any) { setMsg(String(e), true) }
  }
}

function openInGithub() {
  if (!githubRepo.value) return
  openUrl(`https://github.com/${githubRepo.value.owner}/${githubRepo.value.repo}`)
}
function createPullRequest() {
  if (!githubRepo.value || !status.value) return
  openUrl(`https://github.com/${githubRepo.value.owner}/${githubRepo.value.repo}/pull/new/${status.value.branch}`)
}

function onFileSaved() { refresh() }

watch(() => store.state.rootPath, refresh)
onMounted(() => { refresh(); window.addEventListener('file-saved', onFileSaved) })
onBeforeUnmount(() => window.removeEventListener('file-saved', onFileSaved))
</script>

<template>
  <div class="git-panel">
    <div class="git-header">
      <span class="git-title">{{ t('sourceControl') }}</span>
      <div class="git-header-btns">
        <button class="git-icon-btn" @click="openIdentityForm" :disabled="!status?.is_repo" :title="t('gitIdentityTitle')">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 8a3 3 0 100-6 3 3 0 000 6zm2 1H6a4 4 0 00-4 4v1a1 1 0 001 1h10a1 1 0 001-1v-1a4 4 0 00-4-4z"/>
          </svg>
        </button>
        <button class="git-icon-btn" @click="doFetch" :disabled="fetching || !status?.is_repo" :title="t('fetch')">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2v1z"/>
            <path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466z"/>
          </svg>
        </button>
      </div>
    </div>

    <div v-if="!store.state.rootPath" class="git-empty">{{ t('openFolderForGit') }}</div>

    <template v-else-if="status && !status.is_repo">
      <div class="git-empty">
        <p>{{ t('notARepo') }}</p>
        <button class="git-init-btn" @click="initRepo">{{ t('initRepo') }}</button>
      </div>
    </template>

    <template v-else-if="status">
      <!-- Branch + sync row -->
      <div class="git-branch-row">
        <button class="git-branch-btn" @click="toggleBranchMenu">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor">
            <path d="M11.75 2.5a.75.75 0 100 1.5.75.75 0 000-1.5zm-2.25.75a2.25 2.25 0 113 2.122V6A2.5 2.5 0 0110 8.5H6a1 1 0 00-1 1v1.128a2.251 2.251 0 11-1.5 0V5.372a2.25 2.25 0 111.5 0v1.836A2.493 2.493 0 016 7h4a1 1 0 001-1v-.628A2.25 2.25 0 019.5 3.25z"/>
          </svg>
          {{ status.branch || '—' }}
        </button>
        <button v-if="status.ahead || status.behind" class="git-sync-btn" @click="status.behind ? doPull() : doPush()" :disabled="pushing || pulling" :title="`${status.ahead} ${t('ahead')} · ${status.behind} ${t('behind')}`">
          <span v-if="status.behind">↓{{ status.behind }}</span>
          <span v-if="status.ahead">↑{{ status.ahead }}</span>
        </button>
        <button class="git-icon-btn" @click="doPull" :disabled="pulling" :title="t('pull')">⭳</button>
        <button class="git-icon-btn" @click="doPush" :disabled="pushing" :title="t('push')">⭱</button>
      </div>

      <!-- Branch menu -->
      <div v-if="showBranchMenu" class="git-branch-menu">
        <div class="git-branch-new">
          <input v-model="newBranchName" class="git-input" :placeholder="t('newBranchPlaceholder')" @keyup.enter="createBranch" />
          <button class="git-mini-btn" @click="createBranch">{{ t('create') }}</button>
        </div>
        <button v-for="b in branches" :key="b.name" class="git-branch-item" :class="{ active: b.current }" @click="checkoutBranch(b.name)">
          {{ b.current ? '● ' : '' }}{{ b.name }}
        </button>
      </div>

      <!-- GitHub actions -->
      <div v-if="githubRepo" class="git-gh-row">
        <button class="git-gh-btn" @click="openInGithub">{{ t('openInGithub') }}</button>
        <button class="git-gh-btn" @click="createPullRequest">{{ t('createPR') }}</button>
      </div>

      <!-- Messages -->
      <div v-if="error" class="git-msg git-msg--err">{{ error }}</div>
      <div v-if="msg" class="git-msg git-msg--ok">{{ msg }}</div>

      <!-- Commit box -->
      <div class="git-commit-box">
        <textarea v-model="commitMsg" class="git-commit-input" :placeholder="t('commitMsgPlaceholder')" rows="2" />
        <div v-if="staged.length === 0 && unstaged.length > 0" class="git-autostage-hint">{{ t('autoStageHint') }}</div>
        <div class="git-commit-actions">
          <button class="git-commit-btn" :disabled="!commitMsg.trim() || (staged.length === 0 && unstaged.length === 0) || committing" @click="doCommit(false)">
            ✓ {{ t('commit') }} ({{ staged.length || unstaged.length }})
          </button>
          <button class="git-commit-btn git-commit-btn--push" :disabled="!commitMsg.trim() || (staged.length === 0 && unstaged.length === 0) || committing" @click="doCommit(true)" :title="t('commitAndPush')">
            ⭱
          </button>
        </div>
      </div>

      <div class="git-lists">
        <!-- Conflicted -->
        <div v-if="conflicted.length" class="git-section">
          <div class="git-section-label">{{ t('conflicts') }} ({{ conflicted.length }})</div>
          <div v-for="f in conflicted" :key="f.path" class="git-file-row">
            <span class="git-file-code git-code-conflict">{{ f.status_code }}</span>
            <span class="git-file-path">{{ f.path }}</span>
          </div>
        </div>

        <!-- Staged -->
        <div v-if="staged.length" class="git-section">
          <div class="git-section-label">
            {{ t('staged') }} ({{ staged.length }})
            <button class="git-section-action" @click="unstageAll" :title="t('unstageAllTitle')">−</button>
          </div>
          <div v-for="f in staged" :key="f.path" class="git-file-row" @click="openDiff(f)">
            <span class="git-file-code git-code-staged">{{ f.status_code[0] }}</span>
            <span class="git-file-path">{{ f.path }}</span>
            <button class="git-file-action" @click.stop="unstageFile(f.path)" :title="t('unstageTitle')">−</button>
          </div>
        </div>

        <!-- Unstaged / modified -->
        <div v-if="unstaged.length" class="git-section">
          <div class="git-section-label">
            {{ t('changes') }} ({{ unstaged.length }})
            <button class="git-section-action" @click="stageAll" :title="t('stageAllTitle')">+</button>
          </div>
          <div v-for="f in unstaged" :key="f.path" class="git-file-row" @click="openDiff(f)">
            <span class="git-file-code git-code-modified">{{ f.status_code[1] }}</span>
            <span class="git-file-path">{{ f.path }}</span>
            <button class="git-file-action" @click.stop="discardFile(f.path)" :title="t('discardTitle')">↺</button>
            <button class="git-file-action" @click.stop="stageFile(f.path)" :title="t('stageTitle')">+</button>
          </div>
        </div>

        <!-- Untracked -->
        <div v-if="untracked.length" class="git-section">
          <div class="git-section-label">
            {{ t('untracked') }} ({{ untracked.length }})
            <button class="git-section-action" @click="stageAllUntracked" :title="t('stageAllTitle')">+</button>
          </div>
          <div v-for="f in untracked" :key="f.path" class="git-file-row">
            <span class="git-file-code git-code-untracked">U</span>
            <span class="git-file-path">{{ f.path }}</span>
            <button class="git-file-action" @click.stop="removeUntracked(f.path)" :title="t('delete')">×</button>
            <button class="git-file-action" @click.stop="stageFile(f.path)" :title="t('stageTitle')">+</button>
          </div>
        </div>

        <div v-if="!loading && staged.length === 0 && unstaged.length === 0 && untracked.length === 0 && conflicted.length === 0" class="git-clean">
          ✓ {{ t('noPendingChanges') }}
        </div>
      </div>

      <!-- Recent commits -->
      <div class="git-log-toggle" @click="toggleLog">
        <span>{{ showLog ? '▾' : '▸' }} {{ t('recentCommits') }}</span>
      </div>
      <div v-if="showLog" class="git-log">
        <div v-for="c in log" :key="c.hash" class="git-log-item" :title="c.hash">
          <span class="git-log-msg">{{ c.message }}</span>
          <span class="git-log-meta">{{ c.author }} · {{ c.date }}</span>
        </div>
      </div>
    </template>

    <div v-if="showIdentityForm" class="git-identity-overlay" @click.self="showIdentityForm = false">
      <div class="git-identity-modal">
        <div class="git-identity-title">{{ t('gitIdentityTitle') }}</div>
        <p class="git-identity-hint">{{ t('gitIdentityHint') }}</p>

        <label class="git-identity-label">{{ t('name') }}</label>
        <input v-model="identityName" class="git-identity-input" placeholder="Ada Lovelace" />

        <label class="git-identity-label">{{ t('email') }}</label>
        <input v-model="identityEmail" class="git-identity-input" type="email" placeholder="ada@example.com" />

        <label class="git-identity-scope">
          <input type="checkbox" v-model="identityGlobal" />
          <span>{{ t('gitIdentityGlobalLabel') }}</span>
        </label>

        <div v-if="identityError" class="git-identity-error">{{ identityError }}</div>

        <div class="git-identity-actions">
          <button class="git-cancel-btn" @click="showIdentityForm = false">{{ t('cancel') }}</button>
          <button class="git-save-btn" @click="saveIdentity" :disabled="identitySaving || !identityName.trim() || !identityEmail.trim()">
            <span v-if="identitySaving" class="conn-spin" /> {{ identitySaving ? '' : t('save') }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.git-panel { display: flex; flex-direction: column; height: 100%; background: var(--bg-dark); font-family: var(--font-ui); overflow: hidden; }

.git-header {
  display: flex; align-items: center; justify-content: space-between;
  padding: 8px 12px; border-bottom: 1px solid var(--border); background: var(--bg-darkest); flex-shrink: 0;
}
.git-title { font-size: 10.5px; font-weight: 700; letter-spacing: 0.8px; color: var(--fg-muted); }
.git-header-btns { display: flex; gap: 4px; }
.git-icon-btn {
  width: 22px; height: 22px; border-radius: 5px; background: none; border: 1px solid var(--border);
  color: var(--fg-muted); font-size: 12px; cursor: pointer; display: flex; align-items: center; justify-content: center;
}
.git-icon-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--fg); border-color: var(--accent); }
.git-icon-btn:disabled { opacity: 0.4; cursor: default; }

.git-empty { padding: 20px 14px; font-size: 12px; color: var(--fg-muted); text-align: center; }
.git-init-btn {
  margin-top: 10px; background: var(--accent); border: none; border-radius: 6px;
  color: var(--accent-fg); font-size: 12px; font-weight: 600; padding: 7px 14px; cursor: pointer;
}

.git-branch-row { display: flex; align-items: center; gap: 5px; padding: 8px 10px; border-bottom: 1px solid var(--border); flex-shrink: 0; }
.git-branch-btn {
  flex: 1; display: flex; align-items: center; gap: 6px; min-width: 0;
  background: var(--bg-mid); border: 1px solid var(--border); border-radius: 5px;
  color: var(--fg); font-size: 11.5px; font-weight: 600; padding: 4px 8px; cursor: pointer;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.git-branch-btn:hover { border-color: var(--accent); }
.git-sync-btn {
  display: flex; gap: 4px; background: var(--bg-mid); border: 1px solid var(--border); border-radius: 5px;
  color: var(--yellow); font-size: 10.5px; font-family: var(--font-mono); padding: 4px 7px; cursor: pointer; flex-shrink: 0;
}

.git-branch-menu { padding: 8px 10px; border-bottom: 1px solid var(--border); background: var(--bg-mid); flex-shrink: 0; max-height: 180px; overflow-y: auto; }
.git-branch-new { display: flex; gap: 5px; margin-bottom: 6px; }
.git-input { flex: 1; background: var(--bg-darkest); border: 1px solid var(--border); border-radius: 4px; color: var(--fg); font-size: 11.5px; padding: 4px 7px; outline: none; }
.git-mini-btn { background: var(--bg-hover); border: 1px solid var(--border); border-radius: 4px; color: var(--fg-dim); font-size: 11px; padding: 3px 9px; cursor: pointer; }
.git-branch-item { display: block; width: 100%; text-align: left; background: none; border: none; color: var(--fg-dim); font-size: 11.5px; padding: 4px 6px; border-radius: 4px; cursor: pointer; font-family: var(--font-mono); }
.git-branch-item:hover { background: var(--bg-hover); }
.git-branch-item.active { color: var(--green); font-weight: 700; }

.git-gh-row { display: flex; gap: 6px; padding: 8px 10px; border-bottom: 1px solid var(--border); flex-shrink: 0; }
.git-gh-btn { flex: 1; background: var(--bg-mid); border: 1px solid var(--border); border-radius: 5px; color: var(--fg-dim); font-size: 11px; padding: 5px 6px; cursor: pointer; }
.git-gh-btn:hover { border-color: var(--accent); color: var(--fg); }

.git-msg { margin: 6px 10px 0; padding: 6px 9px; border-radius: 5px; font-size: 11px; }
.git-msg--err { background: rgba(248,81,73,0.1); border: 1px solid rgba(248,81,73,0.3); color: #f85149; }
.git-msg--ok { background: rgba(166,227,161,0.1); border: 1px solid rgba(166,227,161,0.3); color: var(--green); }

.git-commit-box { padding: 8px 10px; border-bottom: 1px solid var(--border); flex-shrink: 0; }
.git-commit-input {
  width: 100%; background: var(--bg-mid); border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg); font-size: 12px; padding: 6px 8px; outline: none; resize: vertical; font-family: var(--font-ui);
  box-sizing: border-box;
}
.git-commit-input:focus { border-color: var(--accent); }
.git-autostage-hint { font-size: 10px; color: var(--fg-muted); padding: 3px 2px 0; }
.git-commit-actions { display: flex; gap: 5px; margin-top: 6px; }
.git-commit-btn {
  flex: 1; background: var(--accent); border: none; border-radius: 6px;
  color: var(--accent-fg); font-size: 12px; font-weight: 600; padding: 6px; cursor: pointer;
}
.git-commit-btn:disabled { opacity: 0.4; cursor: default; }
.git-commit-btn--push { flex: 0 0 34px; }

.git-lists { flex: 1; overflow-y: auto; }
.git-clean { padding: 20px; text-align: center; font-size: 12px; color: var(--fg-muted); }

.git-section { margin-bottom: 4px; }
.git-section-label {
  display: flex; align-items: center; justify-content: space-between;
  padding: 6px 10px 3px; font-size: 9.5px; font-weight: 700; letter-spacing: 0.6px; color: var(--fg-muted);
}
.git-section-action { background: none; border: 1px solid var(--border); border-radius: 3px; color: var(--fg-muted); width: 16px; height: 16px; font-size: 11px; cursor: pointer; line-height: 1; }
.git-section-action:hover { color: var(--fg); border-color: var(--accent); }

.git-file-row { display: flex; align-items: center; gap: 6px; padding: 3px 10px; cursor: pointer; border-radius: 3px; margin: 0 2px; }
.git-file-row:hover { background: var(--bg-hover); }
.git-file-code { width: 14px; text-align: center; font-size: 10px; font-weight: 800; font-family: var(--font-mono); flex-shrink: 0; }
.git-code-staged { color: var(--green); }
.git-code-modified { color: var(--yellow); }
.git-code-untracked { color: var(--accent); }
.git-code-conflict { color: var(--red); }
.git-file-path { flex: 1; min-width: 0; font-size: 12px; color: var(--fg-dim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.git-file-action {
  background: none; border: none; color: var(--fg-muted); font-size: 12px; cursor: pointer;
  width: 18px; height: 18px; border-radius: 3px; display: flex; align-items: center; justify-content: center; opacity: 0; flex-shrink: 0;
}
.git-file-row:hover .git-file-action { opacity: 1; }
.git-file-action:hover { background: var(--bg-active); color: var(--fg); }

.git-log-toggle { padding: 7px 10px; font-size: 10.5px; font-weight: 700; color: var(--fg-muted); cursor: pointer; border-top: 1px solid var(--border); flex-shrink: 0; }
.git-log-toggle:hover { color: var(--fg); }
.git-log { max-height: 160px; overflow-y: auto; padding: 0 10px 8px; flex-shrink: 0; }
.git-log-item { padding: 4px 0; border-bottom: 1px solid rgba(255,255,255,0.04); }
.git-log-msg { display: block; font-size: 11.5px; color: var(--fg-dim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.git-log-meta { display: block; font-size: 10px; color: var(--fg-muted); margin-top: 1px; }

.git-identity-overlay {
  position: fixed; inset: 0; z-index: 99999; background: rgba(0,0,0,0.6);
  display: flex; align-items: center; justify-content: center; backdrop-filter: blur(3px);
}
.git-identity-modal {
  background: var(--bg-panel); border: 1px solid var(--border); border-radius: 10px;
  padding: 16px; width: 320px; display: flex; flex-direction: column; gap: 8px;
  box-shadow: 0 20px 60px rgba(0,0,0,0.6); font-family: var(--font-ui);
}
.git-identity-title { font-size: 13.5px; font-weight: 700; color: var(--fg-bright); }
.git-identity-hint { font-size: 11px; color: var(--fg-muted); line-height: 1.4; margin: 0 0 4px; }
.git-identity-label { font-size: 10.5px; color: var(--fg-muted); font-weight: 600; }
.git-identity-input {
  width: 100%; box-sizing: border-box; background: var(--bg-mid); border: 1px solid var(--border);
  border-radius: 6px; color: var(--fg); font-size: 12px; padding: 6px 8px; outline: none; font-family: var(--font-ui);
}
.git-identity-input:focus { border-color: var(--accent); }
.git-identity-scope { display: flex; align-items: center; gap: 7px; font-size: 11.5px; color: var(--fg); margin-top: 2px; cursor: pointer; }
.git-identity-error {
  font-size: 11px; color: #f85149; padding: 6px 8px;
  background: rgba(248,81,73,0.1); border-radius: 5px;
}
.git-identity-actions { display: flex; gap: 8px; margin-top: 6px; }
.git-cancel-btn {
  flex: 1; background: none; border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg-muted); font-size: 12px; padding: 6px; cursor: pointer;
}
.git-cancel-btn:hover { border-color: var(--fg-muted); color: var(--fg); }
.git-save-btn {
  flex: 2; background: var(--accent); border: none; border-radius: 6px;
  color: var(--accent-fg); font-size: 12px; font-weight: 600; padding: 6px; cursor: pointer;
  display: flex; align-items: center; justify-content: center; gap: 6px;
}
.git-save-btn:disabled { opacity: 0.5; cursor: default; }
.conn-spin {
  width: 10px; height: 10px; border: 2px solid rgba(255,255,255,0.4);
  border-top-color: var(--accent-fg); border-radius: 50%; animation: gitspin 0.7s linear infinite; display: inline-block;
}
@keyframes gitspin { to { transform: rotate(360deg); } }
</style>
