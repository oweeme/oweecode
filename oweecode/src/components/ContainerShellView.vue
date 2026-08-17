<script setup lang="ts">
// Interactive shell into a running container — `<runtime> exec -it <id> sh -c
// 'exec bash || exec sh'` tries bash first (nicer to use) and falls back to
// sh (present in nearly every image, including distroless-adjacent ones that
// lack bash) without the user needing to know which one the image ships.
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '../composables/useI18n'
import Terminal from './Terminal.vue'

const props = defineProps<{ containerId: string; containerName: string }>()
const { t } = useI18n()

const runtime = ref<string | null>(null)
const error = ref('')

const args = computed(() => [
  'exec', '-it',
  '-e', 'TERM=xterm-256color',
  props.containerId, 'sh', '-c', 'exec bash 2>/dev/null || exec sh',
])

// Podman shares the UTS namespace across a pod, so every member's own shell
// prompt shows the *pod's* hostname (e.g. "root@lizard#") — identical for
// every container in it, no way to tell which one you're actually in.
// Setting PS1 via `-e` doesn't survive this: most base images' /root/.bashrc
// unconditionally reassigns PS1 on login (only skipping when PS1 is *unset*,
// which ours isn't), clobbering whatever we pass in. Typing the assignment
// in as a command once the shell is already sitting at its prompt runs after
// that, so it sticks. `$(whoami)`/`$PWD` stay literal here — the container's
// own shell expands them, on every prompt redraw.
const postConnectCmd = computed(() => `export PS1="$(whoami)@${props.containerName}:$PWD# "`)

onMounted(async () => {
  try { runtime.value = await invoke<string>('container_runtime') }
  catch (e: any) { error.value = String(e) }
})
</script>

<template>
  <div class="csh-view">
    <div class="csh-header">
      <span class="csh-name">{{ containerName }}</span>
      <span class="csh-id">{{ containerId.slice(0, 12) }}</span>
    </div>
    <div v-if="error" class="csh-error">{{ error }}</div>
    <Terminal
      v-else-if="runtime"
      cwd="/"
      :command="runtime"
      :args="args"
      :post-connect-cmd="postConnectCmd"
    />
    <div v-else class="csh-loading">{{ t('loading') }}</div>
  </div>
</template>

<style scoped>
.csh-view {
  display: flex; flex-direction: column; height: 100%; background: #0d1117;
  font-family: var(--font-ui); overflow: hidden;
  /* Terminal.vue positions itself with `position: absolute; inset: 0`, which
     needs a positioned ancestor to confine to — without this, it escapes to
     the nearest one up the tree (or none, covering the whole window). */
  position: relative;
}
.csh-header {
  display: flex; align-items: center; gap: 8px; padding: 8px 12px;
  background: var(--bg-dark); border-bottom: 1px solid var(--border); flex-shrink: 0;
}
.csh-name { font-family: var(--font-mono); font-size: 12.5px; color: var(--fg-bright); font-weight: 600; }
.csh-id { font-family: var(--font-mono); font-size: 10.5px; color: var(--fg-muted); }
.csh-loading { padding: 20px; font-size: 12px; color: var(--fg-muted); }
.csh-error { padding: 20px; font-size: 12px; color: #f85149; white-space: pre-wrap; }
</style>
