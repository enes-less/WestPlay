import { describe, expect, it } from 'vitest'
import { mapGrows } from './mapGrows'

describe("the Dashboard's map growing", () => {
  it('grows only driving at 30 km/h or more with the screen untouched 30 s', () => {
    expect(mapGrows(true, 30, 30_000)).toBe(true)
    expect(mapGrows(true, 85, 60_000)).toBe(true)
    // Slower, or stopping: no need for it.
    expect(mapGrows(true, 29, 60_000)).toBe(false)
    expect(mapGrows(true, 0, 600_000)).toBe(false)
    // Touched within the 30 s.
    expect(mapGrows(true, 80, 29_000)).toBe(false)
    // Reversing fast counts as driving.
    expect(mapGrows(true, -35, 30_000)).toBe(true)
  })

  it('never grows with the setting off (or nothing to make room from)', () => {
    expect(mapGrows(false, 90, 600_000)).toBe(false)
  })
})
