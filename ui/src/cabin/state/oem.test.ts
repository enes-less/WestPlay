import { describe, expect, it } from 'vitest'
import { frameInsets, resolveOemStrip } from './oem'

describe('resolveOemStrip', () => {
  it("keeps the screen profile's strip in auto mode", () => {
    expect(resolveOemStrip('auto', 'scania-2025')).toBe('scania-2025')
    expect(resolveOemStrip('auto', null)).toBeNull()
  })

  it('can give CarPlay the whole screen', () => {
    expect(resolveOemStrip('off', 'daf-2021')).toBeNull()
  })
})

describe('frameInsets', () => {
  it("reaches into the strip on each layout's stretched screen texture", () => {
    // DAF 2021: 721 texture rows over 720 units, so the strip edge at canvas row 614 is frame row 615; 2 px in.
    expect(frameInsets({ left: 0, bottom: 106 }, 'daf-2021')).toEqual({ left: 0, bottom: 103 })
    // Scania Smart Dash: 722 rows; canvas row 640 is frame row 642. The left edge is never stretched.
    expect(frameInsets({ left: 96, bottom: 80 }, 'scania-2025')).toEqual({ left: 94, bottom: 76 })
  })

  it('only overlaps on exact-size screens, and never past the screen edge', () => {
    expect(frameInsets({ left: 0, bottom: 106 }, 'generic')).toEqual({ left: 0, bottom: 104 })
    expect(frameInsets({ left: 0, bottom: 106 }, null)).toEqual({ left: 0, bottom: 104 })
    expect(frameInsets({ left: 96, bottom: 0 }, 'scania-2025')).toEqual({ left: 94, bottom: 0 })
    expect(frameInsets({ left: 1, bottom: 0 }, 'generic')).toEqual({ left: 0, bottom: 0 })
  })
})
