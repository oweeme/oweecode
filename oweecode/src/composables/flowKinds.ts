// Block kinds of the Flows tab (business flow) and the project map, shared by
// FlowView and the chat snapshot (flowSnapshot.ts).
export interface BlockKind { key: string; label: string; icon: string; color: string }
export const FLOW_KINDS: BlockKind[] = [
  { key: 'trigger',   label: 'Inicio / Evento', icon: '⚡', color: '#e5a50a' },
  { key: 'function',  label: 'Función / Script', icon: 'ƒ', color: '#3b82f6' },
  { key: 'http',      label: 'API / HTTP',      icon: '🌐', color: '#8b5cf6' },
  { key: 'db',        label: 'Base de datos',   icon: '🗄', color: '#14b8a6' },
  { key: 'condition', label: 'Condición',       icon: '⑂',  color: '#f97316' },
  { key: 'email',     label: 'Notificación',    icon: '✉',  color: '#ec4899' },
  { key: 'note',      label: 'Nota',            icon: '✎',  color: '#94a3b8' },
  { key: 'end',       label: 'Fin',             icon: '■',  color: '#ef4444' },
]
export const MAP_KINDS: BlockKind[] = [
  { key: 'router',     label: 'Rutas',          icon: '⇢', color: '#e5a50a' },
  { key: 'controller', label: 'Controladores',  icon: '⚙', color: '#f59e0b' },
  { key: 'layout',     label: 'Layouts',        icon: '▤', color: '#94a3b8' },
  { key: 'page',       label: 'Páginas / HTML', icon: '▣', color: '#3b82f6' },
  { key: 'component',  label: 'Componentes',    icon: '◧', color: '#14b8a6' },
  { key: 'store',      label: 'Stores',         icon: '◉', color: '#8b5cf6' },
  { key: 'service',    label: 'Servicios/API',  icon: '🌐', color: '#ec4899' },
  { key: 'model',      label: 'Modelos / BD',   icon: '🗄', color: '#06b6d4' },
  { key: 'logic',      label: 'Lógica/utils',   icon: 'ƒ', color: '#f97316' },
  { key: 'asset',      label: 'Estilos',        icon: '◌', color: '#a3e635' },
  { key: 'module',     label: 'Otros',          icon: '·', color: '#64748b' },
]
export const ALL_KINDS = [...FLOW_KINDS, ...MAP_KINDS]
export const kindInfo = (k: string) => ALL_KINDS.find(x => x.key === k) ?? FLOW_KINDS[1]
