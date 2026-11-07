import { describe, expect, it } from 'vitest'
import { fitWithin, QUALITY_STEPS } from '../src/composables/imageCompress'

describe('image compression plan', () => {
  it('keeps the aspect ratio and never upscales', () => {
    expect(fitWithin(4000, 3000, 1920)).toEqual({ width: 1920, height: 1440 })
    expect(fitWithin(1080, 1920, 512)).toEqual({ width: 288, height: 512 })
    expect(fitWithin(300, 200, 1920)).toEqual({ width: 300, height: 200 })
  })
  it('tries high quality first and goes down', () => {
    expect([...QUALITY_STEPS].sort((a, b) => b - a)).toEqual(QUALITY_STEPS)
    expect(QUALITY_STEPS[0]).toBeGreaterThanOrEqual(0.8)
  })
})
