import { describe, expect, it } from 'vitest'
import { DARK, goesLight, LIGHT, lightness, luminance, wordsOver } from './contrast'

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

describe('wordsOver', () => {
  // Now Playing's backdrop in the dark look: the cover under a 42% black veil.
  const veiled = { cover: 0.58, rest: [0, 0, 0] as [number, number, number] }
  const whitened = { cover: 0.55, rest: [255, 255, 255] as [number, number, number] }

  it('takes dark words over a light cover in the dark look, the backdrop as it is', () => {
    expect(wordsOver([235, 240, 245], 'dark', veiled)).toBe('light')
    expect(wordsOver([200, 220, 240], 'dark', veiled)).toBe('light')
  })

  it("keeps the look's own words where they read", () => {
    expect(wordsOver([40, 30, 60], 'dark', veiled)).toBe('dark')
    expect(wordsOver([128, 128, 128], 'dark', veiled)).toBe('dark')
    expect(wordsOver([30, 30, 30], 'light', whitened)).toBe('light')
  })
})
