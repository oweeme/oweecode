<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '../composables/useI18n'

const { t } = useI18n()
const props = defineProps<{ path: string; name: string }>()

const src = ref('')
const loading = ref(true)
const error = ref('')
const zoom = ref(1)
const isDragging = ref(false)
const offset = ref({ x: 0, y: 0 })
const dragStart = ref({ x: 0, y: 0 })

function getMime(name: string): string {
  const ext = name.split('.').pop()?.toLowerCase() ?? ''
  const m: Record<string, string> = {
    png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg',
    gif: 'image/gif', webp: 'image/webp', svg: 'image/svg+xml',
    ico: 'image/x-icon', bmp: 'image/bmp', avif: 'image/avif',
  }
  return m[ext] ?? 'image/png'
}

async function load() {
  loading.value = true
  error.value = ''
  src.value = ''
  zoom.value = 1
  offset.value = { x: 0, y: 0 }
  try {
    const b64 = await invoke<string>('read_image_base64', { path: props.path })
    src.value = `data:${getMime(props.name)};base64,${b64}`
  } catch (e) {
    error.value = String(e)
  }
  loading.value = false
}

onMounted(load)
watch(() => props.path, load)

function onWheel(e: WheelEvent) {
  e.preventDefault()
  const delta = e.deltaY > 0 ? 0.9 : 1.1
  zoom.value = Math.max(0.05, Math.min(20, zoom.value * delta))
}

function startDrag(e: MouseEvent) {
  isDragging.value = true
  dragStart.value = { x: e.clientX - offset.value.x, y: e.clientY - offset.value.y }
}
function onDrag(e: MouseEvent) {
  if (!isDragging.value) return
  offset.value = { x: e.clientX - dragStart.value.x, y: e.clientY - dragStart.value.y }
}
function stopDrag() { isDragging.value = false }
function reset() { zoom.value = 1; offset.value = { x: 0, y: 0 } }
function zoomIn() { zoom.value = Math.min(20, zoom.value * 1.25) }
function zoomOut() { zoom.value = Math.max(0.05, zoom.value / 1.25) }
</script>

<template>
  <div
    class="img-viewer"
    @wheel.prevent="onWheel"
    @mousedown="startDrag"
    @mousemove="onDrag"
    @mouseup="stopDrag"
    @mouseleave="stopDrag"
  >
    <!-- Toolbar -->
    <div class="img-toolbar" @mousedown.stop>
      <span class="img-filename">{{ name }}</span>
      <div class="img-controls">
        <button class="img-btn" @click="zoomOut" :title="t('zoomOutTitle')">－</button>
        <span class="img-zoom">{{ Math.round(zoom * 100) }}%</span>
        <button class="img-btn" @click="zoomIn" :title="t('zoomInTitle')">＋</button>
        <button class="img-btn" @click="reset" :title="t('resetTitle')">⊙</button>
      </div>
    </div>

    <!-- Viewer area -->
    <div class="img-canvas">
      <div v-if="loading" class="img-state">
        <span class="spinner" /> Loading…
      </div>
      <div v-else-if="error" class="img-state img-error">{{ error }}</div>
      <img
        v-else
        :src="src"
        :alt="name"
        class="img-el"
        :style="{
          transform: `translate(${offset.x}px, ${offset.y}px) scale(${zoom})`,
          cursor: isDragging ? 'grabbing' : 'grab',
        }"
        draggable="false"
      />
    </div>
  </div>
</template>

<style scoped>
.img-viewer {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: #12121580;
  user-select: none;
}

.img-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 14px;
  background: var(--bg-dark);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}
.img-filename {
  font-family: var(--font-ui);
  font-size: 12px;
  color: var(--fg-dim);
}
.img-controls {
  display: flex;
  align-items: center;
  gap: 4px;
}
.img-btn {
  background: var(--bg-hover);
  border: 1px solid var(--border);
  color: var(--fg);
  border-radius: 4px;
  padding: 2px 8px;
  font-size: 13px;
  cursor: pointer;
  transition: background 0.12s;
}
.img-btn:hover { background: var(--bg-active); }
.img-zoom {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--fg-dim);
  min-width: 42px;
  text-align: center;
}

.img-canvas {
  flex: 1;
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
  background: repeating-conic-gradient(#1a1b1e 0% 25%, #16171a 0% 50%) 0 0 / 24px 24px;
}

.img-el {
  max-width: none;
  max-height: none;
  transform-origin: center center;
  transition: transform 0.05s;
  image-rendering: pixelated;
}

.img-state {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--fg-muted);
  font-family: var(--font-ui);
  font-size: 13px;
}
.img-error { color: var(--red); }

.spinner {
  display: inline-block;
  width: 14px;
  height: 14px;
  border: 2px solid var(--fg-muted);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin 0.7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }
</style>
