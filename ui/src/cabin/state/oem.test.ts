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
  it('reaches 2 frame pixels into the strip, down the screen in the rows of its page', () => {
    // DAF 2021: its page the frame's size.
    expect(frameInsets({ left: 0, bottom: 106 }, 'daf-2021')).toEqual({ left: 0, bottom: 104 })
    // Scania Smart Dash: its page 784 tall, so the strip's 78 frame rows are 84.9 of its; across, the frame's.
    const smartDash = frameInsets({ left: 96, bottom: 80 }, 'scania-2025')
    expect(smartDash.left).toBe(94)
    expect(smartDash.bottom).toBeCloseTo((78 * 784) / 720)
  })

  it('never reaches past the screen edge', () => {
    expect(frameInsets({ left: 0, bottom: 106 }, null)).toEqual({ left: 0, bottom: 104 })
    expect(frameInsets({ left: 96, bottom: 0 }, 'scania-2025')).toEqual({ left: 94, bottom: 0 })
    expect(frameInsets({ left: 1, bottom: 0 }, 'generic')).toEqual({ left: 0, bottom: 0 })
  })
})
