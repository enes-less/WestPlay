import { describe, expect, it } from 'vitest'
import { DARK, goesLight, LIGHT, lightness, luminance } from './contrast'

describe('the glass bar over light things', () => {
  it('reads colours', () => {
    expect(luminance('#000000')).toBe(0)
    expect(luminance('#fff')).toBeCloseTo(1)
    expect(luminance('rgb(11 11 13)')).toBeLessThan(0.01)
    expect(luminance('rgb(255, 255, 255)')).toBeCloseTo(1)
    expect(luminance('not a colour')).toBeNull()
  })

  it('goes dark over light covers, not over the dark page or a dark cover', () => {
    const peach = luminance('#f4c7a8')!
    const page = luminance('#0b0b0d')!
    // Light covers under half the bar or more: dark words.
    expect(goesLight(lightness([peach, peach, peach, peach, page, page, page]), false)).toBe(true)
    expect(goesLight(lightness([page, page, page, page, page, page, page]), false)).toBe(false)
    const red = luminance('#8a1c22')!
    expect(goesLight(lightness([red, red, red, red, red, red, red]), false)).toBe(false)
    expect(lightness([])).toBe(0)
  })

  it('changes only past the other edge, never flickering between', () => {
    const between = (LIGHT + DARK) / 2
    expect(goesLight(between, false)).toBe(false)
    expect(goesLight(between, true)).toBe(true)
    expect(goesLight(LIGHT + 0.01, false)).toBe(true)
    expect(goesLight(DARK - 0.01, true)).toBe(false)
  })
})
