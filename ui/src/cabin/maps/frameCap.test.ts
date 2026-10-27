import { describe, expect, it } from 'vitest'
import { mapFrameDue } from './frameCap'

/** The display's frames over a second at `hz` (each a little early or late by `waver`, ms, in turn), and the
 * gaps between those the map is drawn on. */
function drawn(hz: number, waver = 0): number[] {
  let due = 0
  const at: number[] = []
  for (let tick = 0; tick < hz; tick++) {
    const now = (tick * 1000) / hz + (tick % 2 ? waver : -waver)
    const next = mapFrameDue(now, due)
    if (next === null) continue
    due = next
    at.push(now)
  }
  return at.slice(1).map((time, i) => Math.round(time - at[i]))
}

describe('the map drawn at most 60 a second', () => {
  it('draws every frame of a 60 Hz display, wavering or not', () => {
    expect(drawn(60).length).toBe(59)
    expect(drawn(60, 1).length).toBe(59)
  })

  it('draws 4 of 5 frames at 75 Hz, never skipping two in a row', () => {
    const gaps = drawn(75)
    expect(gaps.length + 1).toBe(60)
    expect(Math.max(...gaps)).toBe(27)
  })

  it('draws 60 of a 144 Hz display\'s, every 2nd or 3rd', () => {
    const gaps = drawn(144)
    expect(gaps.length + 1).toBeGreaterThanOrEqual(59)
    expect(new Set(gaps)).toEqual(new Set([14, 21]))
  })

  it('draws every frame of a display slower than 60', () => {
    expect(drawn(50).length).toBe(49)
  })

  it('counts on from now after a pause', () => {
    expect(mapFrameDue(5000, 100)).toBeCloseTo(5000 + 1000 / 60)
  })
})
