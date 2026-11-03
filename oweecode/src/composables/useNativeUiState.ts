// State of the native UI designer lives at module level so it survives switching editor tabs.
import { reactive } from 'vue'
import { newNode, type UiNode, type Target, type CodeLang } from './nativeUi'

export const nativeUi = reactive({
  active: false,                                   // the Design tab was showing the native designer
  name: 'interfaz',
  target: 'qtwidgets' as Target,
  lang: 'pyside6' as CodeLang,
  root: newNode('window') as UiNode,
  selectedId: null as string | null,
  fileIndex: 0,
  pendingImport: null as string | null,            // a .ui path to import when the designer opens (from the project map)
})

// HTML5 drag payload kept here: WebKit often hides dataTransfer contents during dragover
export const dragState: { payload: { kind: 'new'; type: string } | { kind: 'move'; id: string } | null } = { payload: null }
