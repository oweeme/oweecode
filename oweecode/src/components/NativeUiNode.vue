<script setup lang="ts">
// One widget of the native designer preview (recursive). Draws an approximation of how the
// widget looks in Qt/GTK; the real look comes from the toolkit. Supports drag & drop to move.
import { type UiNode } from '../composables/nativeUi'
import { dragState } from '../composables/useNativeUiState'
import { useI18n } from '../composables/useI18n'
const { t } = useI18n()

const props = defineProps<{ node: UiNode; selectedId: string | null; parent?: 'v' | 'h' }>()
const emit = defineEmits<{ select: [id: string]; drop: [targetId: string] }>()

const items = (s: string) => s.split('\n').map(x => x.trim()).filter(Boolean)
function onDragStart(e: DragEvent) {
  if (props.node.type === 'window') return
  dragState.payload = { kind: 'move', id: props.node.id }
  e.dataTransfer?.setData('text/plain', props.node.id)
  e.stopPropagation()
}
function onDragOver(e: DragEvent) { if (dragState.payload) { e.preventDefault(); e.stopPropagation(); (e.currentTarget as HTMLElement).classList.add('over') } }
function onDragLeave(e: DragEvent) { (e.currentTarget as HTMLElement).classList.remove('over') }
function onDrop(e: DragEvent) { e.preventDefault(); e.stopPropagation(); (e.currentTarget as HTMLElement).classList.remove('over'); emit('drop', props.node.id) }
</script>

<template>
  <div class="nn" :class="[`t-${node.type}`, { sel: selectedId === node.id, grow: node.expand || node.type === 'spacer' }]"
    :draggable="node.type !== 'window'" @dragstart="onDragStart" @dragover="onDragOver" @dragleave="onDragLeave" @drop="onDrop"
    @click.stop="emit('select', node.id)">

    <template v-if="node.type === 'window'">
      <div class="win-title"><span>{{ node.text }}</span><span class="win-btns">— ▢ ✕</span></div>
      <div class="win-body col" :style="{ padding: (node.margin || 12) + 'px', gap: (node.spacing || 8) + 'px', minHeight: '260px' }">
        <NativeUiNode v-for="c in node.children" :key="c.id" :node="c" :selected-id="selectedId" parent="v" @select="emit('select', $event)" @drop="emit('drop', $event)" />
        <div v-if="!node.children.length" class="empty">{{ t('nPickWidget') }}</div>
      </div>
    </template>

    <template v-else-if="node.type === 'vbox' || node.type === 'hbox'">
      <div :class="node.type === 'vbox' ? 'col' : 'row'" :style="{ gap: (node.spacing || 8) + 'px' }" class="cont">
        <NativeUiNode v-for="c in node.children" :key="c.id" :node="c" :selected-id="selectedId" :parent="node.type === 'vbox' ? 'v' : 'h'" @select="emit('select', $event)" @drop="emit('drop', $event)" />
        <div v-if="!node.children.length" class="empty small">{{ t('nEmpty') }}</div>
      </div>
    </template>

    <fieldset v-else-if="node.type === 'frame'" class="group">
      <legend>{{ node.text }}</legend>
      <div class="col cont" :style="{ gap: (node.spacing || 8) + 'px' }">
        <NativeUiNode v-for="c in node.children" :key="c.id" :node="c" :selected-id="selectedId" parent="v" @select="emit('select', $event)" @drop="emit('drop', $event)" />
        <div v-if="!node.children.length" class="empty small">{{ t('nEmpty') }}</div>
      </div>
    </fieldset>

    <span v-else-if="node.type === 'label'" class="lbl">{{ node.text }}</span>
    <button v-else-if="node.type === 'button'" class="btn" tabindex="-1">{{ node.text }}</button>
    <div v-else-if="node.type === 'lineedit'" class="edit">{{ node.placeholder }}</div>
    <div v-else-if="node.type === 'textedit'" class="edit tall">{{ node.placeholder }}</div>
    <label v-else-if="node.type === 'checkbox'" class="chk"><span class="box" :class="{ on: node.value }" />{{ node.text }}</label>
    <div v-else-if="node.type === 'combobox'" class="edit combo">{{ items(node.items)[0] ?? '' }}<span>▾</span></div>
    <div v-else-if="node.type === 'slider'" class="slider"><i :style="{ left: ((node.value - node.min) / Math.max(1, node.max - node.min)) * 100 + '%' }" /></div>
    <div v-else-if="node.type === 'progress'" class="prog"><i :style="{ width: ((node.value - node.min) / Math.max(1, node.max - node.min)) * 100 + '%' }" /></div>
    <div v-else-if="node.type === 'spinbox'" class="edit combo">{{ node.value }}<span>⇅</span></div>
    <div v-else-if="node.type === 'listbox'" class="list"><div v-for="(it, i) in items(node.items)" :key="i" :class="{ on: i === 0 }">{{ it }}</div></div>
    <div v-else-if="node.type === 'image'" class="img">▣ {{ node.text || 'imagen' }}</div>
    <hr v-else-if="node.type === 'separator'" class="sep" />
    <div v-else-if="node.type === 'spacer'" class="spc">{{ parent === 'h' ? '↔' : '↕' }}</div>
  </div>
</template>

<style scoped>
.nn { position: relative; border: 1px dashed transparent; border-radius: 4px; font-size: 13px; color: #e6e6e6; box-sizing: border-box; }
.nn:hover { border-color: rgba(120, 160, 255, 0.45); }
.nn.sel { border-color: #4c9aff; box-shadow: 0 0 0 1px #4c9aff55; }
.nn.over { background: rgba(76, 154, 255, 0.12); }
.nn.grow { flex: 1 1 auto; }
.t-window { background: #2b2d31; border: 1px solid #44474d; border-radius: 8px; overflow: hidden; width: 100%; }
.win-title { display: flex; justify-content: space-between; padding: 6px 10px; background: #1f2023; font-size: 12px; color: #aaa; }
.win-btns { letter-spacing: 6px; }
.col { display: flex; flex-direction: column; } .row { display: flex; flex-direction: row; align-items: center; }
.cont { min-height: 20px; } .t-hbox > .row > .nn, .t-hbox .nn.grow { min-width: 0; }
.empty { color: #7b7f87; font-size: 12px; padding: 18px; text-align: center; border: 1px dashed #55585f; border-radius: 6px; }
.empty.small { padding: 6px; font-size: 11px; }
.group { border: 1px solid #55585f; border-radius: 6px; padding: 8px; margin: 0; min-width: 0; }
.group legend { font-size: 12px; padding: 0 6px; color: #bbb; }
.lbl { display: block; padding: 2px 0; }
.btn { background: #3a3d44; color: #fff; border: 1px solid #50545c; border-radius: 5px; padding: 5px 14px; font-size: 13px; pointer-events: none; }
.edit { background: #1c1d20; border: 1px solid #50545c; border-radius: 4px; padding: 5px 8px; color: #8a8e96; min-height: 18px; }
.edit.tall { min-height: 90px; height: 100%; }
.edit.combo { display: flex; justify-content: space-between; color: #ddd; }
.chk { display: flex; align-items: center; gap: 8px; } .box { width: 14px; height: 14px; border: 2px solid #4c9aff; border-radius: 3px; } .box.on { background: #4c9aff; }
.slider { height: 4px; background: #4c9aff55; border-radius: 2px; position: relative; margin: 10px 0; min-width: 120px; } .slider i { position: absolute; top: -5px; width: 14px; height: 14px; border-radius: 50%; background: #4c9aff; transform: translateX(-50%); }
.prog { height: 8px; background: #1c1d20; border-radius: 4px; overflow: hidden; min-width: 120px; } .prog i { display: block; height: 100%; background: #4c9aff; }
.list { background: #1c1d20; border: 1px solid #50545c; border-radius: 4px; min-height: 70px; height: 100%; } .list div { padding: 4px 8px; } .list .on { background: #4c9aff44; }
.img { background: #1c1d20; border: 1px dashed #55585f; border-radius: 4px; padding: 20px; text-align: center; color: #8a8e96; }
.sep { border: 0; border-top: 1px solid #55585f; width: 100%; margin: 4px 0; }
.spc { color: #5d6169; font-size: 11px; text-align: center; min-height: 18px; min-width: 18px; border: 1px dotted #444; border-radius: 3px; }
</style>
