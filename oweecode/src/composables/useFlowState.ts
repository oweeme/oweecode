// Flows tab state lives at module level so it survives switching to another
// editor tab (the tab's component is unmounted, but the work must not vanish).
import { reactive } from 'vue'
import type { Dialect } from './flowSeed'
import type { CodeLang } from './flowCodegen'

export interface LogLine { level: 'info' | 'ok' | 'err'; text: string }

export const flowState = reactive({
  mode: 'flow' as 'flow' | 'map',
  nodes: [] as any[],
  edges: [] as any[],
  flowName: 'mi-flujo',
  selectedId: null as string | null,
  framework: '',
  mapNotice: '',
  hiddenKinds: [] as string[],
  graph: null as any,
  flowBackup: { nodes: [] as any[], edges: [] as any[] },
  showAi: false,
  aiText: '',
  dialect: 'sqlite' as Dialect,
  seedRows: 10,
  codeLang: 'js' as CodeLang,
  runInput: '{}',
  runLog: [] as LogLine[],
  nodeStatus: {} as Record<string, 'running' | 'ok' | 'err'>,
  running: false,
  // what each block did the last time it ran (request/response, rows, stdout…)
  nodeResults: {} as Record<string, any>,
  resultTab: 'response' as 'response' | 'data' | 'request' | 'output',
  showLog: false,
  // camera per mode, so returning to the tab shows exactly what was on screen
  viewports: {} as Record<string, { x: number; y: number; zoom: number }>,
  // "relations view": one file in the middle, who uses it on the left, what it imports on the right
  isolated: null as string | null,
  isoDepth: 1,
  // DB block execution
  dbConnId: '',
})
