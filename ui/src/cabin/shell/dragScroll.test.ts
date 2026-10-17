import { describe, expect, it } from 'vitest'
import { DRAG_FROM, dragAxis, GLIDE_FALL, GLIDE_STOP, glideDistance, releaseSpeed } from './dragScroll'

describe('dragAxis', () => {
  it('is no drag until the cursor has moved far enough with the button held (a click)', () => {
    expect(dragAxis(3, 4)).toBeNull()
    expect(dragAxis(DRAG_FROM - 1, 0)).toBeNull()
  })

  it('goes the way the cursor mostly went', () => {
    expect(dragAxis(12, 3)).toBe('x')
    expect(dragAxis(-2, -14)).toBe('y')
    expect(dragAxis(6, -6)).toBe('y')
  })
})

describe('releaseSpeed', () => {
  it('is how fast the cursor went just before it was let go', () => {
    // 40 px over the last 80 ms; an older, slower part doesn't count.
    expect(
      releaseSpeed([
        [0, 0],
        [100, 10],
        [140, 30],
        [180, 50],
      ]),
    ).toBeCloseTo(0.5)
  })

  it('is nothing for a drag that stopped or barely moved', () => {
    expect(releaseSpeed([])).toBe(0)
    expect(releaseSpeed([[10, 40]])).toBe(0)
    expect(
      releaseSpeed([
        [10, 40],
        [10, 40],
      ]),
    ).toBe(0)
  })
})

describe('glideDistance', () => {
  it('carries a fling on, further the faster it was', () => {
    expect(glideDistance(1)).toBeCloseTo(1 / GLIDE_FALL)
    expect(glideDistance(-2)).toBeLessThan(glideDistance(-1))
  })

  it('stops a slow release where it is', () => {
    expect(glideDistance(GLIDE_STOP / 2)).toBe(0)
  })
})
