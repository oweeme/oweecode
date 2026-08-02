<script setup lang="ts">
import { inject, ref, computed, watch, nextTick, type Ref } from 'vue'

export interface TreeNodeData {
  name: string
  path: string
  is_dir: boolean
  children?: TreeNodeData[]
  expanded?: boolean
  loaded?: boolean
  isCreating?: boolean
}

const props = defineProps<{ node: TreeNodeData; depth: number }>()

const emit = defineEmits<{
  toggle: [node: TreeNodeData]
  contextmenu: [e: MouseEvent, node: TreeNodeData]
}>()

// Injected from FileExplorer
const renaming    = inject<Ref<{ node: TreeNodeData; name: string } | null>>('explorerRenaming')
const confirmRename = inject<() => void>('explorerConfirmRename', () => {})
const cancelRename  = inject<() => void>('explorerCancelRename',  () => {})
const confirmCreate = inject<(name: string) => void>('explorerConfirmCreate', () => {})
const cancelCreate  = inject<() => void>('explorerCancelCreate',  () => {})

// Active file highlight
const activeFilePath = inject<Ref<string | null>>('activeFilePath')
const isActive = computed(() => !props.node.is_dir && props.node.path === activeFilePath?.value)

// Keyboard focus highlight
const focusedPath = inject<Ref<string | null>>('focusedPath')
const isFocused = computed(() => props.node.path === focusedPath?.value)
const rowEl = ref<HTMLElement | null>(null)
watch(isActive, (val) => {
  if (val) nextTick(() => rowEl.value?.scrollIntoView({ block: 'nearest', behavior: 'smooth' }))
})

// Drag & drop
const draggedNode = inject<Ref<TreeNodeData | null>>('draggedNode')
const onDropNode  = inject<(target: TreeNodeData) => void>('onDropNode', () => {})

const isDragOver = ref(false)

function onDragStart(e: DragEvent) {
  if (draggedNode) draggedNode.value = props.node
  e.dataTransfer?.setData('text/plain', props.node.path)
}

function onDragOver(e: DragEvent) {
  if (!props.node.is_dir) return
  if (draggedNode?.value?.path === props.node.path) return
  e.preventDefault()
  isDragOver.value = true
}

function onDragLeave() { isDragOver.value = false }

function onDrop(e: DragEvent) {
  e.preventDefault()
  isDragOver.value = false
  if (!props.node.is_dir) return
  if (draggedNode?.value && draggedNode.value.path !== props.node.path) {
    onDropNode(props.node)
  }
}

const isRenaming = computed(() => renaming?.value?.node.path === props.node.path)

const createName = ref('')

function handleCreate() {
  if (createName.value.trim()) confirmCreate(createName.value)
  else cancelCreate()
}

const FOLDER_COLORS: Record<string, string> = {
  src:        '#89b4fa', source: '#89b4fa',
  components: '#cba6f7', comp: '#cba6f7',
  composables:'#cba6f7',
  assets:     '#f9e2af', public: '#f9e2af', static: '#f9e2af',
  node_modules:'#585b70',
  dist:       '#585b70', build: '#585b70', target: '#585b70',
  docs:       '#a6e3a1', doc: '#a6e3a1',
  tests:      '#f38ba8', test: '#f38ba8', __tests__: '#f38ba8',
  lib:        '#89dceb', libs: '#89dceb',
  config:     '#e8a838', cfg: '#e8a838',
  database:   '#e38c00', db: '#e38c00',
  api:        '#89b4fa', routes: '#89b4fa',
  images:     '#cba6f7', img: '#cba6f7', icons: '#cba6f7',
  lang:       '#a6e3a1', i18n: '#a6e3a1', locale: '#a6e3a1',
}

function getFolderColor(name: string): string {
  return FOLDER_COLORS[name.toLowerCase()] ?? '#e8a838'
}
</script>

<template>
  <div>
    <!-- Inline create pseudo-node -->
    <div
      v-if="node.isCreating"
      class="tree-row tree-row--creating"
      :style="{ paddingLeft: (8 + depth * 14) + 'px' }"
      @click.stop
    >
      <span class="tree-arrow-ph" />
      <span class="tree-icon">
        <svg v-if="node.is_dir" width="15" height="15" viewBox="0 0 16 16" fill="#e8a838">
          <path d="M1.75 1A1.75 1.75 0 000 2.75v10.5C0 14.22.784 15 1.75 15h12.5A1.75 1.75 0 0016 13.25v-8.5A1.75 1.75 0 0014.25 3H7.5L6.34 1.5A1.75 1.75 0 004.99 1H1.75z"/>
        </svg>
        <svg v-else width="14" height="14" viewBox="0 0 16 16" fill="#9da5b4">
          <path d="M2 1.75C2 .784 2.784 0 3.75 0h6.586c.464 0 .909.184 1.237.513l2.914 2.914c.329.328.513.773.513 1.237v9.586A1.75 1.75 0 0113.25 16h-9.5A1.75 1.75 0 012 14.25V1.75z"/>
        </svg>
      </span>
      <input
        v-model="createName"
        class="inline-edit"
        :placeholder="node.is_dir ? 'folder-name' : 'filename.ext'"
        @keydown.enter="handleCreate"
        @keydown.esc="cancelCreate()"
        autofocus
      />
    </div>

    <!-- Normal node row -->
    <div
      v-else
      ref="rowEl"
      class="tree-row"
      :class="{ 'tree-row--dragover': isDragOver, 'tree-row--active': isActive, 'tree-row--focused': isFocused }"
      :style="{ paddingLeft: (8 + depth * 14) + 'px' }"
      :data-path="node.path"
      draggable="true"
      @click="emit('toggle', node)"
      @contextmenu.stop="emit('contextmenu', $event, node)"
      @dragstart="onDragStart"
      @dragend="isDragOver = false"
      @dragover="onDragOver"
      @dragleave="onDragLeave"
      @drop="onDrop"
    >
      <span class="tree-arrow" v-if="node.is_dir">
        <svg v-if="node.expanded" width="9" height="9" viewBox="0 0 9 9" fill="currentColor">
          <path d="M0 2.5l4.5 4L9 2.5z"/>
        </svg>
        <svg v-else width="9" height="9" viewBox="0 0 9 9" fill="currentColor">
          <path d="M2.5 0l4 4.5-4 4z"/>
        </svg>
      </span>
      <span class="tree-arrow-ph" v-else />

      <span v-if="node.is_dir" class="tree-icon">
        <svg width="15" height="15" viewBox="0 0 16 16" :style="{ fill: getFolderColor(node.name) }">
          <path v-if="node.expanded" d="M1.75 2.5A.25.25 0 001.5 2.75v10.5c0 .138.112.25.25.25h12.5a.25.25 0 00.25-.25V5.5a.25.25 0 00-.25-.25H7.53L6.28 3.97a.75.75 0 00-.53-.22H1.75z"/>
          <path v-else d="M1.75 1A1.75 1.75 0 000 2.75v10.5C0 14.22.784 15 1.75 15h12.5A1.75 1.75 0 0016 13.25v-8.5A1.75 1.75 0 0014.25 3H7.5L6.34 1.5A1.75 1.75 0 004.99 1H1.75z"/>
        </svg>
      </span>
      <span v-else class="tree-icon file-icon-wrap">
        <FileIcon :name="node.name" />
      </span>

      <!-- Inline rename input OR label -->
      <input
        v-if="isRenaming"
        class="inline-edit"
        :value="renaming?.name"
        @input="(e) => { if (renaming) renaming.name = (e.target as HTMLInputElement).value }"
        @keydown.enter="confirmRename()"
        @keydown.esc="cancelRename()"
        @click.stop
        autofocus
      />
      <span v-else class="tree-label" :class="{ 'tree-label--dir': node.is_dir }">{{ node.name }}</span>
    </div>

    <!-- Children (recursive) -->
    <template v-if="node.is_dir && node.expanded && node.children">
      <TreeNode
        v-for="child in node.children"
        :key="child.path"
        :node="child"
        :depth="depth + 1"
        @toggle="emit('toggle', $event)"
        @contextmenu="(e, n) => emit('contextmenu', e, n)"
      />
    </template>
  </div>
</template>

<!-- FileIcon sub-component -->
<script lang="ts">
import { defineComponent, h } from 'vue'

const FileIcon = defineComponent({
  props: { name: { type: String, required: true } },
  setup(props) {
    return () => {
      const ext = props.name.split('.').pop()?.toLowerCase() ?? ''
      const full = props.name.toLowerCase()
      const colors: Record<string, string> = {
        js:'#f7df1e', mjs:'#f7df1e', jsx:'#61dafb', ts:'#3178c6', tsx:'#3178c6',
        vue:'#42b883', html:'#e44d26', htm:'#e44d26', css:'#264de4', scss:'#cc6699',
        sass:'#cc6699', php:'#8892be', go:'#00add8', py:'#3572a5', rs:'#ce422b',
        json:'#cbcb41', yaml:'#cc3e44', yml:'#cc3e44', toml:'#9c4221',
        xml:'#e37933', sql:'#e38c00', md:'#519aba', txt:'#888', pdf:'#e53935',
        png:'#a074c4', jpg:'#a074c4', jpeg:'#a074c4', gif:'#a074c4',
        webp:'#a074c4', svg:'#ffb13b', ico:'#a074c4', c:'#00599c', cpp:'#00599c',
        cc:'#00599c', h:'#a074c4', hpp:'#a074c4', sh:'#4eaa25', bash:'#4eaa25',
        zsh:'#4eaa25', lock:'#555', log:'#888', env:'#e8b449',
      }
      let color = '#9da5b4'
      if (full === 'dockerfile') color = '#2496ed'
      else if (full.includes('.gitignore') || full.includes('.git')) color = '#f05032'
      else if (full === 'makefile') color = '#6d8086'
      else if (colors[ext]) color = colors[ext]

      const badges: Record<string, string> = {
        js:'JS', mjs:'JS', jsx:'⚛', ts:'TS', tsx:'⚛', vue:'V', html:'H', htm:'H',
        css:'C', scss:'S', sass:'S', php:'P', go:'G', py:'Py', rs:'Rs', json:'{}',
        yaml:'Y', yml:'Y', toml:'T', sql:'DB', md:'M', pdf:'P', svg:'S',
        c:'C', cpp:'C+', h:'H', hpp:'H+', sh:'$', bash:'$', zsh:'$', env:'E',
      }
      const badge = badges[ext] ?? (ext.slice(0, 2).toUpperCase() || '?')

      return h('svg', { width: 16, height: 16, viewBox: '0 0 16 16' }, [
        h('rect', { x: 1, y: 0, width: 14, height: 16, rx: 2, fill: color, opacity: 0.15 }),
        h('rect', { x: 1, y: 0, width: 14, height: 16, rx: 2, fill: 'none', stroke: color, 'stroke-width': 1, opacity: 0.6 }),
        h('text', {
          x: 8, y: 11.5, 'text-anchor': 'middle', fill: color,
          'font-size': badge.length > 2 ? '4.5' : '5.5',
          'font-weight': '700', 'font-family': 'monospace',
        }, badge),
      ])
    }
  },
})
export default { components: { FileIcon } }
</script>

<style scoped>
.tree-row {
  display: flex; align-items: center; gap: 3px;
  height: 22px; cursor: pointer; user-select: none;
  white-space: nowrap; overflow: hidden;
  border-radius: 4px; margin: 1px 4px;
  transition: background 0.08s;
}
.tree-row:hover { background: var(--bg-hover); }
.tree-row--creating { cursor: default; background: rgba(208,208,216,0.06); border: 1px solid rgba(208,208,216,0.25); }
.tree-row--dragover { background: rgba(208,208,216,0.15) !important; outline: 1px dashed var(--accent); }
.tree-row--active { background: var(--bg-active) !important; }
.tree-row--active .tree-label { color: var(--fg-bright); font-weight: 500; }
.tree-row--focused { outline: 1px solid var(--accent); outline-offset: -1px; background: rgba(82,139,255,0.1) !important; }
.tree-row--focused .tree-label { color: var(--fg-bright); }

.tree-arrow {
  width: 14px; height: 14px;
  display: flex; align-items: center; justify-content: center;
  color: var(--fg-muted); flex-shrink: 0;
}
.tree-arrow-ph { width: 14px; flex-shrink: 0; }
.tree-icon { flex-shrink: 0; display: flex; align-items: center; justify-content: center; width: 16px; }
.file-icon-wrap { width: 16px; height: 16px; }

.tree-label { overflow: hidden; text-overflow: ellipsis; font-size: 12.5px; color: var(--fg); font-family: var(--font-ui); flex: 1; }
.tree-label--dir { font-weight: 500; }

.inline-edit {
  flex: 1; min-width: 0;
  background: var(--bg-darkest);
  border: 1px solid var(--accent);
  border-radius: 3px;
  color: var(--fg);
  font-size: 12px;
  font-family: var(--font-mono);
  padding: 1px 5px;
  outline: none;
  height: 18px;
}
</style>
