// Compresses a screenshot before sending it to the Oweeme chat: resizes to at
// most 1600 px and re-encodes as WebP (JPEG fallback), lowering quality until
// it weighs less than 300 KB. Returns a data URL (what the API stores).
//
// Typical flow: OweeCapture (Ctrl+Alt+S) copies the capture to the clipboard →
// Ctrl+V in the chat box → this runs → the image is attached.

export const CHAT_IMAGE_MAX_BYTES = 300 * 1024

/** Scale (w, h) so the longest side is at most `max`, keeping the aspect ratio. */
export function fitWithin(w: number, h: number, max: number): { width: number; height: number } {
  const k = Math.min(1, max / Math.max(w, h))
  return { width: Math.max(1, Math.round(w * k)), height: Math.max(1, Math.round(h * k)) }
}

export const QUALITY_STEPS = [0.86, 0.8, 0.72, 0.64, 0.56, 0.48, 0.4]

const encode = (c: HTMLCanvasElement, type: string, q: number) => new Promise<Blob | null>((r) => c.toBlob(r, type, q))

function toDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const fr = new FileReader()
    fr.onload = () => resolve(String(fr.result))
    fr.onerror = () => reject(fr.error)
    fr.readAsDataURL(blob)
  })
}

export interface ChatCompressOptions {
  maxBytes?: number
  maxSide?: number
  /**
   * Diagrams (flow snapshots): try lossless PNG first, shrinking down to `pngMinSide`,
   * before falling back to WebP/JPEG. Text and thin lines stay sharp; JPEG (what
   * WebKitGTK uses, it cannot encode WebP) blurs them.
   */
  preferPng?: boolean
  pngMinSide?: number
}

export async function compressForChat(file: Blob, opts: ChatCompressOptions | number = {}, legacyMaxSide?: number): Promise<{ dataUrl: string; bytes: number; width: number; height: number }> {
  const o: ChatCompressOptions = typeof opts === 'number' ? { maxBytes: opts, maxSide: legacyMaxSide } : opts
  const maxBytes = o.maxBytes ?? CHAT_IMAGE_MAX_BYTES
  const maxSide = o.maxSide ?? 1600
  if (!file.type.startsWith('image/')) throw new Error('not-image')
  const bitmap = await createImageBitmap(file)
  const canvas = document.createElement('canvas')
  const ctx = canvas.getContext('2d')!
  const draw = (width: number, height: number) => {
    canvas.width = width
    canvas.height = height
    ctx.fillStyle = '#fff'
    ctx.fillRect(0, 0, width, height)
    ctx.imageSmoothingQuality = 'high'
    ctx.drawImage(bitmap, 0, 0, width, height)
  }
  const done = async (blob: Blob, width: number, height: number) => {
    bitmap.close()
    return { dataUrl: await toDataUrl(blob), bytes: blob.size, width, height }
  }

  if (o.preferPng) {
    for (const side of pngSides(maxSide, o.pngMinSide ?? 1200)) {
      const { width, height } = fitWithin(bitmap.width, bitmap.height, side)
      draw(width, height)
      const blob = await encode(canvas, 'image/png', 1)
      if (blob && blob.size <= maxBytes) return done(blob, width, height)
    }
  }

  let { width, height } = fitWithin(bitmap.width, bitmap.height, maxSide)
  for (let round = 0; round < 5; round++) {
    draw(width, height)
    for (const q of QUALITY_STEPS) {
      let blob = await encode(canvas, 'image/webp', q)
      if (!blob || blob.type !== 'image/webp') blob = await encode(canvas, 'image/jpeg', q)
      if (blob && blob.size <= maxBytes) return done(blob, width, height)
    }
    width = Math.round(width * 0.8)
    height = Math.round(height * 0.8)
  }
  bitmap.close()
  throw new Error('too-large')
}

/** Longest sides tried for a PNG: from maxSide down to minSide, 15 % smaller each step. */
export function pngSides(maxSide: number, minSide: number): number[] {
  const out: number[] = []
  for (let s = maxSide; s >= minSide; s = Math.round(s * 0.85)) out.push(s)
  return out
}
