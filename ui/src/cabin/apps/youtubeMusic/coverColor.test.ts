import { describe, expect, it } from 'vitest'
import { colorOf } from './coverColor'

const pixels = (...colors: [number, number, number][]) =>
  new Uint8ClampedArray(colors.flatMap(([r, g, b]) => [r, g, b, 255]))

describe('colorOf', () => {
  it('keeps a cover hue, deep and muted', () => {
    expect(colorOf(pixels([230, 120, 110], [240, 130, 120]))).toMatch(/^hsl\((\d) \d+% (1[3-9]|2[0-4])%\)$/)
  })

  it('lets coloured pixels outweigh grey ones', () => {
    const hue = Number(/hsl\((\d+)/.exec(colorOf(pixels([128, 128, 128], [128, 128, 128], [40, 60, 220]))!)![1])
    expect(hue).toBeGreaterThan(200)
    expect(hue).toBeLessThan(260)
  })

  it('has none for a picture with nothing to see', () => {
    expect(colorOf(new Uint8ClampedArray([0, 0, 0, 0]))).toBeNull()
  })
})
