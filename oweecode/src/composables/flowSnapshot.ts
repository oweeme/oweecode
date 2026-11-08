// Draws the Flows tab (blocks + connections) as a picture, so it can be sent to
// the Oweeme chat like a screenshot. It paints straight onto a canvas from the
// flow data (not from the screen), so the result is sharp and needs no permission.
//
// What gets drawn:
//   - the frame you are looking at in the Flows tab (same position and zoom), or
//   - the whole flow when the Flows tab is not on screen.
//
//   visibleFlowFrame()                       the on-screen frame, in flow coordinates
//   flowLayout(nodes, edges, frame?)         pure: boxes and connection points (tested)
//   renderFlowPng(nodes, edges, title, frame?)  → PNG Blob, then compressForChat() it
import { kindInfo } from './flowKinds'

export interface Box { id: string; x: number; y: number; w: number; h: number; label: string; desc: string; kind: string }
export interface Link { from: { x: number; y: number }; to: { x: number; y: number }; label: string }
export interface FlowLayout { boxes: Box[]; links: Link[]; width: number; height: number }
/** A rectangle in flow coordinates (`zoom` = how it looked on screen). */
export interface Frame { x: number; y: number; w: number; h: number; zoom: number }

/**
 * The part of the flow visible in the Flows tab right now, read from Vue Flow's
 * pane size and viewport transform. null when the tab is closed or hidden.
 */
export function visibleFlowFrame(): Frame | null {
  const pane = document.querySelector('.flow-view .vue-flow') as HTMLElement | null
  const vp = document.querySelector('.flow-view .vue-flow__transformationpane') as HTMLElement | null
  if (!pane || !vp) return null
  const rect = pane.getBoundingClientRect()
  if (rect.width < 20 || rect.height < 20) return null
  const m = new DOMMatrix(getComputedStyle(vp).transform)
  const zoom = m.a || 1
  return { x: -m.e / zoom, y: -m.f / zoom, w: rect.width / zoom, h: rect.height / zoom, zoom }
}

const PAD = 40
const DEFAULT_W = 200
const HEAD_H = 30
const DESC_H = 26

/**
 * Places the blocks relative to the picture's top-left corner, plus each connection's endpoints.
 * Without a frame: the whole flow with padding. With a frame: exactly that area, keeping only
 * the blocks inside it and the connections that touch it.
 */
export function flowLayout(nodes: any[], edges: any[], frame?: Frame | null): FlowLayout {
  const visible = nodes.filter((n) => !n.hidden && n.position)
  if (!visible.length) return { boxes: [], links: [], width: 0, height: 0 }
  const all = visible.map((n) => {
    const desc = String(n.data?.desc ?? '').trim()
    const w = n.dimensions?.width || n.width || DEFAULT_W
    const h = n.dimensions?.height || n.height || HEAD_H + (desc ? DESC_H : 0)
    return { id: String(n.id), x: n.position.x, y: n.position.y, w, h, label: String(n.data?.label ?? n.id), desc, kind: String(n.data?.kind ?? 'function') }
  })
  const ox = frame ? frame.x : Math.min(...all.map((b) => b.x)) - PAD
  const oy = frame ? frame.y : Math.min(...all.map((b) => b.y)) - PAD
  const inFrame = (b: Box) => !frame || (b.x < frame.x + frame.w && b.x + b.w > frame.x && b.y < frame.y + frame.h && b.y + b.h > frame.y)
  const placed = all.map((b) => ({ ...b, show: inFrame(b), x: b.x - ox, y: b.y - oy }))
  const byId = new Map(placed.map((b) => [b.id, b]))
  const links: Link[] = []
  for (const e of edges) {
    const s = byId.get(String(e.source))
    const t = byId.get(String(e.target))
    if (!s || !t || e.hidden || !(s.show || t.show)) continue
    // condition blocks have two outputs: "yes" at 30 % of the height, "no" at 75 %
    const k = e.sourceHandle === 'yes' ? 0.3 : e.sourceHandle === 'no' ? 0.75 : 0.5
    links.push({ from: { x: s.x + s.w, y: s.y + s.h * k }, to: { x: t.x, y: t.y + t.h / 2 }, label: typeof e.label === 'string' ? e.label : '' })
  }
  const boxes: Box[] = placed.filter((b) => b.show).map(({ show: _show, ...b }) => b)
  if (frame) return { boxes, links, width: frame.w, height: frame.h }
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
export async function renderFlowPng(nodes: any[], edges: any[], title: string, frame?: Frame | null): Promise<Blob> {
  const L = flowLayout(nodes, edges, frame)
  if (!L.boxes.length) throw new Error('empty-flow')
  const TITLE_H = 34
  // A frame keeps its on-screen zoom (×2 for sharp text); the whole flow is drawn at 2×.
  // Either way the canvas stays under 4000 px; compressForChat() then brings it to ≤ 1600 px.
  const want = frame ? frame.zoom * 2 : 2
  const scale = Math.max(0.3, Math.min(want, 4000 / Math.max(L.width, L.height + TITLE_H)))
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
  ctx.beginPath() // connections coming from outside the frame must not paint over the title
  ctx.rect(0, 0, L.width, L.height)
  ctx.clip()
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
