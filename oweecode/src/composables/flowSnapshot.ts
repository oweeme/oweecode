// Draws the Flows tab (blocks + connections) as a picture, so it can be sent to
// the Oweeme chat like a screenshot. It paints straight onto a canvas from the
// flow data (not from the screen), so it works even when the Flows tab is closed
// and shows the whole flow, not only the part that fits in the window.
//
//   flowLayout(nodes, edges)   pure: boxes and connection points (tested)
//   renderFlowPng(nodes, edges, title)  → PNG Blob, then compressForChat() it
import { kindInfo } from './flowKinds'

export interface Box { id: string; x: number; y: number; w: number; h: number; label: string; desc: string; kind: string }
export interface Link { from: { x: number; y: number }; to: { x: number; y: number }; label: string }
export interface FlowLayout { boxes: Box[]; links: Link[]; width: number; height: number }

const PAD = 40
const DEFAULT_W = 200
const HEAD_H = 30
const DESC_H = 26

/** Places every visible block relative to the top-left of the flow, plus each connection's endpoints. */
export function flowLayout(nodes: any[], edges: any[]): FlowLayout {
  const visible = nodes.filter((n) => !n.hidden && n.position)
  if (!visible.length) return { boxes: [], links: [], width: 0, height: 0 }
  const raw = visible.map((n) => {
    const desc = String(n.data?.desc ?? '').trim()
    const w = n.dimensions?.width || n.width || DEFAULT_W
    const h = n.dimensions?.height || n.height || HEAD_H + (desc ? DESC_H : 0)
    return { id: String(n.id), x: n.position.x, y: n.position.y, w, h, label: String(n.data?.label ?? n.id), desc, kind: String(n.data?.kind ?? 'function') }
  })
  const minX = Math.min(...raw.map((b) => b.x))
  const minY = Math.min(...raw.map((b) => b.y))
  const boxes = raw.map((b) => ({ ...b, x: b.x - minX + PAD, y: b.y - minY + PAD }))
  const byId = new Map(boxes.map((b) => [b.id, b]))
  const links: Link[] = []
  for (const e of edges) {
    const s = byId.get(String(e.source))
    const t = byId.get(String(e.target))
    if (!s || !t || e.hidden) continue
    // condition blocks have two outputs: "yes" at 30 % of the height, "no" at 75 %
    const k = e.sourceHandle === 'yes' ? 0.3 : e.sourceHandle === 'no' ? 0.75 : 0.5
    links.push({ from: { x: s.x + s.w, y: s.y + s.h * k }, to: { x: t.x, y: t.y + t.h / 2 }, label: typeof e.label === 'string' ? e.label : '' })
  }
  const width = Math.max(...boxes.map((b) => b.x + b.w)) + PAD
  const height = Math.max(...boxes.map((b) => b.y + b.h)) + PAD
  return { boxes, links, width, height }
}

function ellipsis(ctx: CanvasRenderingContext2D, text: string, max: number) {
  if (ctx.measureText(text).width <= max) return text
  let s = text
  while (s.length > 1 && ctx.measureText(s + '…').width > max) s = s.slice(0, -1)
  return s + '…'
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath()
  ctx.moveTo(x + r, y)
  ctx.arcTo(x + w, y, x + w, y + h, r)
  ctx.arcTo(x + w, y + h, x, y + h, r)
  ctx.arcTo(x, y + h, x, y, r)
  ctx.arcTo(x, y, x + w, y, r)
  ctx.closePath()
}

/** Paints the flow on a dark canvas (same look as the Flows tab) and returns it as PNG. */
export async function renderFlowPng(nodes: any[], edges: any[], title: string): Promise<Blob> {
  const L = flowLayout(nodes, edges)
  if (!L.boxes.length) throw new Error('empty-flow')
  const TITLE_H = 34
  // 2× for sharp text on small flows; big project maps are capped so the canvas stays reasonable
  const scale = Math.max(0.5, Math.min(2, 4000 / Math.max(L.width, L.height + TITLE_H)))
  const canvas = document.createElement('canvas')
  canvas.width = Math.ceil(L.width * scale)
  canvas.height = Math.ceil((L.height + TITLE_H) * scale)
  const ctx = canvas.getContext('2d')!
  ctx.scale(scale, scale)
  const font = 'system-ui, -apple-system, "Segoe UI", sans-serif'

  ctx.fillStyle = '#141519'
  ctx.fillRect(0, 0, L.width, L.height + TITLE_H)
  ctx.fillStyle = '#2a2c33' // dotted grid, like the editor background
  for (let x = 10; x < L.width; x += 20) for (let y = TITLE_H + 10; y < L.height + TITLE_H; y += 20) ctx.fillRect(x, y, 1.2, 1.2)

  ctx.fillStyle = '#e6e6e6'
  ctx.font = `600 15px ${font}`
  ctx.textBaseline = 'middle'
  ctx.fillText(ellipsis(ctx, '🔀 ' + title, L.width - 2 * PAD), PAD, TITLE_H / 2 + 6)

  ctx.translate(0, TITLE_H)
  ctx.lineWidth = 1.6
  ctx.strokeStyle = '#7d8590'
  ctx.fillStyle = '#7d8590'
  for (const l of L.links) {
    const dx = Math.max(40, Math.abs(l.to.x - l.from.x) / 2)
    ctx.beginPath()
    ctx.moveTo(l.from.x, l.from.y)
    ctx.bezierCurveTo(l.from.x + dx, l.from.y, l.to.x - dx, l.to.y, l.to.x, l.to.y)
    ctx.stroke()
    ctx.beginPath() // arrow head
    ctx.moveTo(l.to.x, l.to.y)
    ctx.lineTo(l.to.x - 8, l.to.y - 4)
    ctx.lineTo(l.to.x - 8, l.to.y + 4)
    ctx.fill()
    if (l.label) {
      const mx = (l.from.x + l.to.x) / 2
      const my = (l.from.y + l.to.y) / 2
      ctx.font = `600 11px ${font}`
      const w = ctx.measureText(l.label).width + 10
      ctx.fillStyle = '#1b1c20'
      ctx.fillRect(mx - w / 2, my - 9, w, 18)
      ctx.fillStyle = '#e6e6e6'
      ctx.textAlign = 'center'
      ctx.fillText(l.label, mx, my)
      ctx.textAlign = 'left'
      ctx.fillStyle = '#7d8590'
    }
  }

  for (const b of L.boxes) {
    const k = kindInfo(b.kind)
    roundRect(ctx, b.x, b.y, b.w, b.h, 8)
    ctx.fillStyle = '#1b1c20'
    ctx.fill()
    ctx.save()
    ctx.clip()
    ctx.fillStyle = k.color + '33'
    ctx.fillRect(b.x, b.y, b.w, HEAD_H)
    ctx.restore()
    roundRect(ctx, b.x, b.y, b.w, b.h, 8)
    ctx.strokeStyle = k.color
    ctx.lineWidth = 1.5
    ctx.setLineDash(b.kind === 'note' ? [5, 4] : [])
    ctx.stroke()
    ctx.setLineDash([])

    ctx.font = `13px ${font}`
    ctx.fillStyle = k.color
    ctx.fillText(k.icon, b.x + 10, b.y + HEAD_H / 2)
    ctx.font = `600 12px ${font}`
    ctx.fillStyle = '#e6e6e6'
    ctx.fillText(ellipsis(ctx, b.label, b.w - 40), b.x + 30, b.y + HEAD_H / 2)
    if (b.desc && b.h > HEAD_H + 10) {
      ctx.font = `11px ${font}`
      ctx.fillStyle = '#9aa0a6'
      ctx.fillText(ellipsis(ctx, b.desc, b.w - 20), b.x + 10, b.y + HEAD_H + Math.min(DESC_H, b.h - HEAD_H) / 2)
    }
  }

  const blob = await new Promise<Blob | null>((r) => canvas.toBlob(r, 'image/png'))
  if (!blob) throw new Error('canvas')
  return blob
}
