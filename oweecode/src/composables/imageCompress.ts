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

export async function compressForChat(file: Blob, maxBytes = CHAT_IMAGE_MAX_BYTES, maxSide = 1600): Promise<{ dataUrl: string; bytes: number; width: number; height: number }> {
  if (!file.type.startsWith('image/')) throw new Error('not-image')
  const bitmap = await createImageBitmap(file)
  let { width, height } = fitWithin(bitmap.width, bitmap.height, maxSide)
  const canvas = document.createElement('canvas')
  const ctx = canvas.getContext('2d')!
  for (let round = 0; round < 5; round++) {
    canvas.width = width
    canvas.height = height
    ctx.fillStyle = '#fff'
    ctx.fillRect(0, 0, width, height)
    ctx.imageSmoothingQuality = 'high'
    ctx.drawImage(bitmap, 0, 0, width, height)
    for (const q of QUALITY_STEPS) {
      let blob = await encode(canvas, 'image/webp', q)
      if (!blob || blob.type !== 'image/webp') blob = await encode(canvas, 'image/jpeg', q)
      if (blob && blob.size <= maxBytes) {
        bitmap.close()
        return { dataUrl: await toDataUrl(blob), bytes: blob.size, width, height }
      }
    }
    width = Math.round(width * 0.8)
    height = Math.round(height * 0.8)
  }
  bitmap.close()
  throw new Error('too-large')
}
